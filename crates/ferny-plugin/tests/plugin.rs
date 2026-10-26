//! Lua host (H3.2, threats T4, T5, T17).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ferny_plugin::{
    Action, Answer, CALL_BUDGET, DocInfo, Grants, MAX_STREAM_BYTES, Plugin, PluginError, bundled,
    discover, parse_manifest, run_process,
};

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-plugin-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn plugin(dir: &Path, id: &str, permissions: &str, code: &str) -> PathBuf {
    let path = dir.join(id);
    fs::create_dir_all(&path).expect("mkdir");
    fs::write(
        path.join("plugin.toml"),
        format!("id = \"{id}\"\nname = \"Test {id}\"\nversion = \"1.0\"\npermissions = [{permissions}]\n"),
    )
    .expect("manifest");
    fs::write(path.join("main.lua"), code).expect("code");
    path
}

/// Runs `code` as a plugin; returns the result and what it logged.
fn run(name: &str, code: &str) -> (Result<(), PluginError>, Vec<String>) {
    let dir = temp(name);
    let path = plugin(&dir, "p", "", code);
    let manifest = discover(&dir).0.remove(0).1;
    let logged = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&logged);
    let result = Plugin::start(&path, manifest, Vec::new(), "", move |m| {
        sink.lock().expect("lock").push(m.to_owned());
    })
    .map(|_| ());
    let logged = logged.lock().expect("lock").clone();
    (result, logged)
}

#[test]
fn sandbox_allows_the_safe_library_and_the_api() {
    let (result, logged) = run(
        "safe",
        "ferny.log(string.format('%d %s %s', math.max(1, 2), utf8.char(241), table.concat({'a','b'}, ',')))\n\
         ferny.log(tostring(ferny.api_version) .. ' ' .. tostring(ferny.has_permission('net')))",
    );
    assert_eq!(result, Ok(()));
    assert_eq!(logged, ["2 ñ a,b", "0 false"]);
}

#[test]
fn private_config_requires_grant_and_stays_per_plugin() {
    let dir = temp("config");
    let path = plugin(
        &dir,
        "p",
        "\"config\"",
        "local ok, err = ferny.config.write('secret')\nferny.log(tostring(ok) .. ' ' .. tostring(err))",
    );
    let manifest = discover(&dir).0.remove(0).1;
    let denied = Plugin::start(&path, manifest.clone(), Vec::new(), "", |_| {});
    assert!(matches!(denied, Err(PluginError::Runtime(_))));

    let logged = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&logged);
    let plugin = Plugin::start_from_with_config(
        &ferny_plugin::Source::Folder(path),
        manifest,
        vec!["config".to_owned()],
        "",
        Some(&dir),
        move |message| sink.lock().expect("lock").push(message.to_owned()),
    )
    .expect("config plugin");
    assert!(
        logged
            .lock()
            .expect("lock")
            .iter()
            .any(|m| m.contains("config used"))
    );
    drop(plugin);
    assert!(!dir.join("secret").exists());
    assert!(dir.join("plugin-config").join("p").join("data").exists());
}

#[test]
fn sandbox_blocks_escapes() {
    // Every one of these is nil in the sandbox: reporting proves it.
    let (result, logged) = run(
        "escapes",
        "for _, name in ipairs({'io', 'os', 'package', 'debug', 'coroutine', 'load', 'loadfile', \
         'dofile', 'require', 'collectgarbage', 'print'}) do \
           if _G[name] ~= nil then ferny.log('present ' .. name) end end\n\
         if string.dump ~= nil then ferny.log('present string.dump') end\n\
         ferny.log('done')",
    );
    assert_eq!(result, Ok(()));
    assert_eq!(logged, ["done"]);
    // Calling one is an error, not an escape.
    let (result, _) = run("os-execute", "os.execute('touch /tmp/ferny-pwned')");
    assert!(matches!(result, Err(PluginError::Runtime(_))));
    assert!(!Path::new("/tmp/ferny-pwned").exists());
    // Bytecode is refused (chunks are text only).
    let dir = temp("bytecode");
    let path = plugin(&dir, "p", "", "");
    fs::write(path.join("main.lua"), b"\x1bLuaT\x00\x19\x93").expect("write");
    let manifest = discover(&dir).0.remove(0).1;
    assert!(Plugin::start(&path, manifest, Vec::new(), "", |_| {}).is_err());
}

#[test]
fn limits_stop_loops_and_memory_bombs() {
    for (name, code) in [
        ("loop", "while true do end"),
        (
            "pcall-loop",
            "local function f() while true do end end while true do pcall(f) end",
        ),
        (
            "xpcall-loop",
            "while true do xpcall(function() while true do end end, function(e) return e end) end",
        ),
        ("recursion", "local function f() return f() + 1 end f()"),
    ] {
        let start = Instant::now();
        let (result, _) = run(name, code);
        assert!(
            matches!(result, Err(PluginError::Runtime(_))),
            "{name}: {result:?}"
        );
        assert!(
            start.elapsed() < CALL_BUDGET * 20,
            "{name}: {:?}",
            start.elapsed()
        );
    }
    let (result, _) = run(
        "memory",
        "local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1024) .. i end",
    );
    assert!(matches!(result, Err(PluginError::Runtime(_))), "{result:?}");
}

#[test]
fn manifests_are_validated() {
    let ok = parse_manifest(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"1\"\npermissions = [\"net\", \"clipboard\", \"net\"]\n",
        "hello",
    )
    .expect("valid");
    assert_eq!(ok.permissions, ["clipboard", "net"]);
    for (text, folder) in [
        ("id = \"other\"\nname = \"x\"\nversion = \"1\"\n", "hello"),
        ("id = \"Bad Id\"\nname = \"x\"\nversion = \"1\"\n", "Bad Id"),
        (
            "id = \"x\"\nname = \"x\"\nversion = \"1\"\npermissions = [\"root\"]\n",
            "x",
        ),
        ("id = \"x\"\nname = \"x\"\nversion = \"1\"\napi = 9\n", "x"),
        (
            "id = \"x\"\nname = \"x\"\nversion = \"1\"\nentry = \"evil.lua\"\n",
            "x",
        ),
        ("id = \"x\"\nname = \"\\u0007\"\nversion = \"1\"\n", "x"),
        ("not toml", "x"),
    ] {
        assert_eq!(
            parse_manifest(text, folder),
            Err(PluginError::Manifest),
            "{text}"
        );
    }
}

#[test]
fn discovery_skips_links_and_bad_plugins() {
    let dir = temp("discover");
    plugin(&dir, "good", "", "");
    fs::create_dir_all(dir.join("broken")).expect("mkdir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("good"), dir.join("linked")).expect("link");
    let (found, warnings) = discover(&dir);
    assert_eq!(
        found.iter().map(|(_, m)| m.id.as_str()).collect::<Vec<_>>(),
        ["good"]
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn grants_are_tied_to_the_manifest() {
    let dir = temp("grants");
    let path = dir.join("plugin-grants.toml");
    let manifest = parse_manifest(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"1\"\npermissions = [\"net\"]\n",
        "hello",
    )
    .expect("valid");
    let mut grants = Grants::default();
    assert!(grants.decision(&manifest).is_none());
    // Only permissions of the manifest can be granted.
    grants.decide(
        &manifest,
        true,
        &["net".to_owned(), "process.spawn".to_owned()],
    );
    assert!(grants.save(&path));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&path).expect("meta").mode() & 0o777, 0o600);
    }
    let loaded = Grants::load(&path);
    assert_eq!(
        loaded.decision(&manifest).expect("decided").granted,
        ["net"]
    );
    // A manifest asking for more needs a new decision.
    let wider = parse_manifest(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"2\"\npermissions = [\"net\", \"process.spawn\"]\n",
        "hello",
    )
    .expect("valid");
    assert!(loaded.decision(&wider).is_none());
    // A tampered file granting more than requested is ignored.
    fs::write(
        &path,
        "[hello]\nenabled = true\nrequested = [\"net\"]\ngranted = [\"net\", \"clipboard\"]\n",
    )
    .expect("write");
    assert!(Grants::load(&path).decision(&manifest).is_none());
}

#[test]
fn grants_persist_explicit_disable_and_reenable() {
    let dir = temp("grant-toggle");
    let path = dir.join("plugin-grants.toml");
    let manifest = parse_manifest(
        "id = \"toggle\"\nname = \"Toggle\"\nversion = \"1\"\npermissions = [\"clipboard\"]\n",
        "toggle",
    )
    .expect("valid");
    let mut grants = Grants::default();

    grants.decide(&manifest, true, &["clipboard".to_owned()]);
    assert!(grants.save(&path));
    assert!(
        Grants::load(&path)
            .decision(&manifest)
            .is_some_and(|grant| grant.enabled && grant.granted == ["clipboard"])
    );

    grants.decide(&manifest, false, &["clipboard".to_owned()]);
    assert!(grants.save(&path));
    assert!(
        Grants::load(&path)
            .decision(&manifest)
            .is_some_and(|grant| !grant.enabled && grant.granted.is_empty())
    );

    grants.decide(&manifest, true, &["clipboard".to_owned()]);
    assert!(grants.save(&path));
    assert!(
        Grants::load(&path)
            .decision(&manifest)
            .is_some_and(|grant| grant.enabled && grant.granted == ["clipboard"])
    );
}

/// Starts `code` as plugin `p` in its own folder; `files` adds files.
fn start(name: &str, code: &str, files: &[(&str, &str)]) -> Plugin {
    let dir = temp(name);
    let path = plugin(&dir, "p", "", code);
    for (file, text) in files {
        let file = path.join(file);
        fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        fs::write(file, text).expect("write");
    }
    let manifest = discover(&dir).0.remove(0).1;
    Plugin::start(&path, manifest, Vec::new(), "es", |_| {}).expect("start")
}

fn doc() -> DocInfo {
    DocInfo {
        path: "/w/a.c".to_owned(),
        filetype: "c".to_owned(),
        line: 3,
        column: 2,
        text: "int a;".to_owned(),
        selection: "a".to_owned(),
        clipboard: String::new(),
        word: "in".to_owned(),
    }
}

#[test]
fn events_run_and_see_no_text() {
    let p = start(
        "events",
        r#"ferny.on("document.opened", function(d)
               ferny.ui.status(d.path .. " " .. d.filetype)
               local ok = pcall(ferny.editor.text)
               ferny.ui.message(tostring(ok))
           end)"#,
        &[],
    );
    let actions = p.emit("document.opened", Some(&doc())).expect("emit");
    assert_eq!(
        actions,
        [
            Action::Status("/w/a.c c".to_owned()),
            Action::Message("false".to_owned())
        ]
    );
    assert!(
        p.emit("document.saved", Some(&doc()))
            .expect("emit")
            .is_empty()
    );
}

#[test]
fn edits_only_in_commands() {
    let p = start(
        "commands",
        r#"ferny.command{id = "up", title = "Upper", run = function()
               local d = ferny.documents.current()
               ferny.editor.replace_selection(ferny.editor.selection():upper())
               ferny.editor.insert(ferny.editor.text() .. d.line .. ":" .. d.column)
           end}
           ferny.on("document.saved", function() ferny.editor.insert("x") end)"#,
        &[],
    );
    assert_eq!(p.commands()[0].id, "plugin.p.up");
    assert_eq!(p.commands()[0].title, "Upper");
    let actions = p.run_command("plugin.p.up", Some(doc())).expect("run");
    assert_eq!(
        actions,
        [
            Action::ReplaceSelection("A".to_owned()),
            Action::Insert("int a;3:2".to_owned())
        ]
    );
    assert!(p.emit("document.saved", Some(&doc())).is_err());
    assert!(p.run_command("plugin.p.nope", Some(doc())).is_err());
}

#[test]
fn commands_are_validated() {
    for spec in [
        r#"{id = "Bad.Id", title = "T", run = function() end}"#,
        r#"{id = "ok", title = "evil\u{202E}txt", run = function() end}"#,
        r#"{id = "ok", title = "", run = function() end}"#,
        r#"{id = "ok", title = "T"}"#,
    ] {
        let dir = temp("validate");
        let path = plugin(&dir, "p", "", &format!("ferny.command{spec}"));
        let manifest = discover(&dir).0.remove(0).1;
        assert!(
            Plugin::start(&path, manifest, Vec::new(), "", |_| {}).is_err(),
            "{spec}"
        );
    }
    // Registration only while loading; duplicate ids refused.
    let p = start(
        "late",
        r#"ferny.command{id = "a", title = "A", run = function()
               ferny.command{id = "b", title = "B", run = function() end}
           end}
           ferny.on("app.startup_complete", function() ferny.on("document.saved", function() end) end)
           assert(not pcall(ferny.command, {id = "a", title = "A", run = function() end}))
           assert(not pcall(ferny.on, "no.such_event", function() end))"#,
        &[],
    );
    assert!(p.run_command("plugin.p.a", None).is_err());
    assert!(p.emit("app.startup_complete", None).is_err());
    assert_eq!(p.commands().len(), 1);
}

#[test]
fn actions_are_bounded_and_texts_neutralised() {
    let p = start(
        "bounds",
        r#"ferny.command{id = "many", title = "M", run = function()
               for i = 1, 1000 do ferny.ui.status("s") end
           end}
           ferny.command{id = "long", title = "L", run = function()
               ferny.ui.status(string.rep("x", 300))
           end}
           ferny.command{id = "big", title = "B", run = function()
               local s = string.rep("x", 600 * 1024)
               ferny.editor.insert(s) ferny.editor.insert(s)
           end}
           ferny.command{id = "bidi", title = "N", run = function()
               ferny.ui.message("a\u{202E}b\27[31m")
           end}"#,
        &[],
    );
    for id in ["many", "long", "big"] {
        assert!(
            p.run_command(&format!("plugin.p.{id}"), Some(doc()))
                .is_err(),
            "{id}"
        );
    }
    let actions = p.run_command("plugin.p.bidi", Some(doc())).expect("run");
    let [Action::Message(text)] = actions.as_slice() else {
        panic!("{actions:?}");
    };
    assert!(
        !text.contains('\u{202E}') && !text.contains('\u{1b}'),
        "{text}"
    );
}

#[test]
fn slow_events_hit_the_time_limit() {
    let p = start(
        "slow",
        r#"ferny.on("document.activated", function() while true do end end)"#,
        &[],
    );
    let started = Instant::now();
    let err = p.emit("document.activated", None).expect_err("limit");
    assert!(err.is_time_limit());
    assert!(started.elapsed() < CALL_BUDGET * 10);
}

#[test]
fn locale_is_bounded() {
    let p = start(
        "locale",
        r#"ferny.command{id = "t", title = ferny.tr("Hello"), run = function() end}"#,
        &[("locale/es.toml", "\"Hello\" = \"Hola\"\n")],
    );
    assert_eq!(p.commands()[0].title, "Hola");
    for bad in ["not toml [", "Hello = 3\n"] {
        let p = start(
            "locale-bad",
            r#"ferny.command{id = "t", title = ferny.tr("Hello"), run = function() end}"#,
            &[("locale/es.toml", bad)],
        );
        assert_eq!(p.commands()[0].title, "Hello");
    }
    let huge = format!("\"Hello\" = \"{}\"\n", "x".repeat(70 * 1024));
    let p = start(
        "locale-huge",
        r#"ferny.command{id = "t", title = ferny.tr("Hello"), run = function() end}"#,
        &[("locale/es.toml", &huge)],
    );
    assert_eq!(p.commands()[0].title, "Hello");
}

/// Starts plugin `p` with `permissions` granted (all asked in the
/// manifest) and `confirmed` sensitive ones.
fn start_with(
    name: &str,
    permissions: &[&str],
    confirmed: &[&str],
    code: &str,
) -> (Plugin, PathBuf) {
    let dir = temp(name);
    let list: Vec<String> = permissions.iter().map(|p| format!("\"{p}\"")).collect();
    let path = plugin(&dir.join("plugins"), "p", &list.join(", "), code);
    let manifest = discover(&dir.join("plugins")).0.remove(0).1;
    let granted = permissions.iter().map(|p| (*p).to_owned()).collect();
    let p = Plugin::start(&path, manifest, granted, "", |_| {}).expect("start");
    let confirmed: Vec<String> = confirmed.iter().map(|p| (*p).to_owned()).collect();
    p.set_confirmations(&confirmed, &[]);
    (p, dir)
}

fn command_messages(p: &Plugin) -> Vec<Action> {
    p.run_command("plugin.p.go", Some(doc())).expect("run")
}

const ECHO_RESULTS: &str = r#"
local function show(...) local t = table.pack(...)
    ferny.ui.message(tostring(t[1] ~= nil) .. " " .. tostring(t[2])) end
"#;

#[test]
fn fs_reads_inside_the_trusted_project_only() {
    let code = format!(
        r#"{ECHO_RESULTS}
        ferny.command{{id = "go", title = "Go", run = function()
            show(ferny.fs.read("a.txt"))
            show(ferny.fs.read(OUTSIDE))
            show(ferny.fs.list("."))
            show(ferny.fs.read("link"))
        end}}"#
    );
    let (p, dir) = start_with("fs-read", &["fs.read.project"], &[], "");
    let project = dir.join("proj");
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join("a.txt"), "hi").expect("write");
    fs::write(dir.join("secret"), "s").expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("secret"), project.join("link")).expect("link");
    let code = code.replace(
        "OUTSIDE",
        &format!("{:?}", dir.join("secret").to_string_lossy()),
    );
    fs::write(dir.join("plugins/p/main.lua"), code).expect("code");
    let manifest = discover(&dir.join("plugins")).0.remove(0).1;
    let p2 = Plugin::start(
        &dir.join("plugins/p"),
        manifest,
        vec!["fs.read.project".into()],
        "",
        |_| {},
    )
    .expect("start");
    drop(p);
    // Without a trusted project nothing relative can be read.
    let msgs = command_messages(&p2);
    assert_eq!(msgs[0], Action::Message("false no trusted project".into()));
    p2.set_project(Some(&project));
    let msgs = command_messages(&p2);
    assert_eq!(msgs[0], Action::Message("true nil".into()));
    assert_eq!(msgs[1], Action::Message("false permission denied".into()));
    assert_eq!(msgs[2], Action::Message("true nil".into()));
    assert_eq!(
        msgs[3],
        Action::Message("false links are not followed".into())
    );
}

#[test]
fn fs_read_any_needs_confirmation() {
    let dir = temp("fs-any-data");
    fs::write(dir.join("secret"), "s").expect("write");
    let code = format!(
        r#"{ECHO_RESULTS}
        ferny.command{{id = "go", title = "Go", run = function() show(ferny.fs.read({:?})) end}}"#,
        dir.join("secret").to_string_lossy()
    );
    let (p, _) = start_with("fs-any", &["fs.read.any"], &[], &code);
    let msgs = command_messages(&p);
    assert_eq!(
        msgs,
        [
            Action::Confirm("fs.read.any".into()),
            Action::Message("false confirmation pending".into())
        ]
    );
    p.set_confirmations(&["fs.read.any".into()], &[]);
    assert_eq!(command_messages(&p), [Action::Message("true nil".into())]);
    p.set_confirmations(&[], &["fs.read.any".into()]);
    assert_eq!(
        command_messages(&p),
        [Action::Message("false permission denied".into())]
    );
}

#[test]
fn fs_writes_are_confined() {
    let code = format!(
        r#"{ECHO_RESULTS}
        ferny.command{{id = "go", title = "Go", run = function()
            show(ferny.fs.write("new.txt", "data"))
            show(ferny.fs.write("../escape.txt", "x"))
            show(ferny.fs.write("sub/../../escape.txt", "x"))
            show(ferny.fs.write("link", "x"))
            show(ferny.fs.write(string.rep("x", 10), string.rep("x", 5 * 1024 * 1024)))
        end}}"#
    );
    let (p, dir) = start_with("fs-write", &["fs.write.project"], &[], &code);
    let project = dir.join("proj");
    fs::create_dir_all(project.join("sub")).expect("mkdir");
    fs::write(dir.join("target"), "keep").expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("target"), project.join("link")).expect("link");
    assert_eq!(
        command_messages(&p)[0],
        Action::Message("false no trusted project".into())
    );
    p.set_project(Some(&project));
    let msgs = command_messages(&p);
    assert_eq!(msgs[0], Action::Message("true nil".into()));
    assert_eq!(
        fs::read_to_string(project.join("new.txt")).expect("read"),
        "data"
    );
    for m in &msgs[1..3] {
        assert_eq!(*m, Action::Message("false outside the project".into()));
    }
    assert_eq!(msgs[3], Action::Message("false not a regular file".into()));
    assert_eq!(msgs[4], Action::Message("false too large".into()));
    assert!(!dir.join("escape.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.join("target")).expect("read"),
        "keep"
    );
}

#[test]
fn spawn_is_requested_bounded_and_delivered() {
    let code = format!(
        r#"{ECHO_RESULTS}
        ferny.command{{id = "go", title = "Go", run = function()
            show(ferny.spawn{{program = "echo", args = {{"a; rm -rf /", "$HOME"}},
                on_exit = function(r) ferny.ui.message(r.code .. ":" .. r.stdout) end}})
            ferny.ui.message(tostring(pcall(ferny.spawn, {{program = "x", args = {{true}}}})))
            for i = 1, 4 do show(ferny.spawn{{program = "true"}}) end
        end}}"#
    );
    let (p, _) = start_with("spawn", &["process.spawn"], &["process.spawn"], &code);
    let msgs = command_messages(&p);
    assert_eq!(msgs[0], Action::Message("true nil".into()));
    assert_eq!(msgs[1], Action::Message("false".into()));
    assert_eq!(msgs[5], Action::Message("false too many processes".into()));
    let spawns = p.take_spawns();
    assert_eq!(spawns.len(), 4);
    assert_eq!(spawns[0].args, ["a; rm -rf /", "$HOME"]);
    // The arguments reach the program verbatim: no shell.
    let outcome = run_process(
        Path::new("/bin/echo"),
        &spawns[0].args,
        &temp("spawn-cwd"),
        &[],
        Duration::from_secs(5),
    );
    assert_eq!(outcome.code, Some(0));
    assert_eq!(outcome.stdout, b"a; rm -rf / $HOME\n");
    assert_eq!(
        p.deliver(spawns[0].token, &outcome).expect("deliver"),
        [Action::Message("0:a; rm -rf / $HOME\\u{000A}".into())]
    );
    // Delivered tokens are gone; a slot is free again.
    assert!(
        p.deliver(spawns[0].token, &outcome)
            .expect("again")
            .is_empty()
    );
}

#[test]
fn processes_time_out_and_output_is_bounded() {
    let started = Instant::now();
    let slow = run_process(
        Path::new("/bin/sleep"),
        &["10".to_owned()],
        &temp("proc"),
        &[],
        Duration::from_millis(200),
    );
    assert!(slow.timed_out && slow.code.is_none());
    assert!(started.elapsed() < Duration::from_secs(5));
    let big = run_process(
        Path::new("/usr/bin/yes"),
        &[],
        &temp("proc2"),
        &[],
        Duration::from_millis(300),
    );
    assert_eq!(big.stdout.len(), MAX_STREAM_BYTES);
    let missing = run_process(
        Path::new("/no/such/program"),
        &[],
        &temp("proc3"),
        &[],
        Duration::from_secs(1),
    );
    assert!(missing.error.is_some());
}

#[test]
fn clipboard_reads_only_in_commands() {
    let code = r#"
        ferny.command{id = "go", title = "Go", run = function()
            ferny.clipboard.set("copied")
            ferny.ui.message(ferny.clipboard.get())
        end}
        ferny.on("document.saved", function() ferny.clipboard.get() end)"#;
    let (p, _) = start_with("clipboard", &["clipboard"], &[], code);
    let mut d = doc();
    d.clipboard = "pasted".into();
    assert_eq!(
        p.run_command("plugin.p.go", Some(d)).expect("run"),
        [
            Action::ClipboardSet("copied".into()),
            Action::Message("pasted".into())
        ]
    );
    assert!(p.emit("document.saved", Some(&doc())).is_err());
    let (q, _) = start_with("clipboard-none", &[], &[], code);
    assert!(q.run_command("plugin.p.go", Some(doc())).is_err());
}

#[test]
fn confirmations_are_remembered_until_the_manifest_changes() {
    let dir = temp("confirm");
    let path = dir.join("plugin-grants.toml");
    let m1 = parse_manifest(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"1\"\npermissions = [\"process.spawn\", \"clipboard\"]\n",
        "hello",
    )
    .expect("manifest");
    let mut grants = Grants::default();
    assert!(!grants.confirm(&m1, "process.spawn", true));
    grants.decide(&m1, true, &["process.spawn".into(), "clipboard".into()]);
    assert!(!grants.confirm(&m1, "clipboard", true)); // not sensitive
    assert!(grants.confirm(&m1, "process.spawn", true));
    assert!(grants.save(&path));
    let loaded = Grants::load(&path);
    assert_eq!(
        loaded.decision(&m1).expect("grant").confirmed,
        ["process.spawn"]
    );
    let m2 = parse_manifest(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"2\"\npermissions = [\"process.spawn\"]\n",
        "hello",
    )
    .expect("manifest");
    assert!(loaded.decision(&m2).is_none());
    // A hand-edited file confirming an ungranted permission is dropped.
    fs::write(
        &path,
        "[hello]\nenabled = true\nrequested = [\"net\"]\ngranted = []\nconfirmed = [\"net\"]\n",
    )
    .expect("write");
    assert!(Grants::load(&path).decision(&m1).is_none());
}

#[test]
fn providers_answer_for_their_filetypes() {
    let p = start(
        "providers",
        r#"ferny.provide("completion", {filetypes = {"c"}, run = function(ctx)
               return {ctx.word .. "t", ctx.word .. "line", "bad\nitem", ctx.word .. "t"}
           end})
           ferny.provide("hover", {filetypes = {"*"}, run = function(ctx)
               if ctx.line == 3 then return "line 3\ncol " .. ctx.column .. "\u{202E}" end
           end})
           ferny.provide("format", {filetypes = {"c"}, run = function(ctx)
               return ctx.text:gsub("%s+", " ")
           end})"#,
        &[],
    );
    assert!(p.has_provider("completion", "c") && !p.has_provider("completion", "rust"));
    assert!(p.has_provider("hover", "rust"));
    let d = doc();
    assert_eq!(
        p.provide("completion", &d).expect("completion").0,
        Some(Answer::Items(vec!["int".into(), "inline".into()]))
    );
    let Some(Answer::Text(hover)) = p.provide("hover", &d).expect("hover").0 else {
        panic!("no hover");
    };
    assert!(
        hover.starts_with("line 3\ncol 2") && !hover.contains('\u{202E}'),
        "{hover}"
    );
    let other = DocInfo { line: 9, ..doc() };
    assert_eq!(p.provide("hover", &other).expect("hover").0, None);
    assert_eq!(
        p.provide("format", &d).expect("format").0,
        Some(Answer::Text("int a;".into()))
    );
    let rust = DocInfo {
        filetype: "rust".into(),
        ..doc()
    };
    assert_eq!(p.provide("format", &rust).expect("format").0, None);
}

#[test]
fn provider_answers_are_bounded() {
    let p = start(
        "provider-bounds",
        r#"ferny.provide("completion", {filetypes = {"*"}, run = function()
               local t = {} for i = 1, 2000 do t[i] = "w" .. i end
               t[1] = string.rep("x", 300) return t end})
           ferny.provide("hover", {filetypes = {"*"}, run = function()
               return string.rep("h", 5000) end})
           ferny.provide("format", {filetypes = {"*"}, run = function() return 42 end})"#,
        &[],
    );
    let Some(Answer::Items(items)) = p.provide("completion", &doc()).expect("completion").0 else {
        panic!("no items");
    };
    assert_eq!(items.len(), 500);
    assert_eq!(items[0], "w2");
    assert!(p.provide("hover", &doc()).is_err());
    assert!(p.provide("format", &doc()).is_err());
    for bad in [
        r#"ferny.provide("definition", {filetypes = {"*"}, run = function() end})"#,
        r#"ferny.provide("hover", {filetypes = {}, run = function() end})"#,
        r#"ferny.provide("hover", {filetypes = {"c c"}, run = function() end})"#,
    ] {
        let dir = temp("provider-bad");
        let path = plugin(&dir, "p", "", bad);
        let manifest = discover(&dir).0.remove(0).1;
        assert!(
            Plugin::start(&path, manifest, Vec::new(), "", |_| {}).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn before_save_chains_and_hits_the_time_limit() {
    let p = start(
        "before-save",
        r#"ferny.on("document.before_save", function(d) return (d.text:gsub(" +\n", "\n")) end)
           ferny.on("document.before_save", function(d) return d.text .. "// end\n" end)
           ferny.on("document.before_save", function(d) end)"#,
        &[],
    );
    assert!(p.has_before_save());
    let d = DocInfo {
        text: "a   \nb\n".into(),
        ..doc()
    };
    assert_eq!(
        p.before_save(&d).expect("save").0.as_deref(),
        Some("a\nb\n// end\n")
    );
    let slow = start(
        "before-save-slow",
        r#"ferny.on("document.before_save", function() while true do end end)"#,
        &[],
    );
    assert!(slow.before_save(&doc()).expect_err("limit").is_time_limit());
    let none = start("before-save-none", "", &[]);
    assert_eq!(none.before_save(&doc()).expect("none").0, None);
}

/// Every API function the stubs declare exists, and the reverse.
#[test]
fn stubs_match_the_api() {
    let stubs = include_str!("../../../data/plugin-api/ferny.d.lua");
    let mut declared: Vec<String> = stubs
        .lines()
        .filter_map(|l| l.strip_prefix("function "))
        .filter_map(|l| l.split('(').next())
        .map(str::to_owned)
        .collect();
    declared.sort();
    let dir = temp("stubs");
    let path = plugin(
        &dir,
        "p",
        "",
        r#"local names = {}
           for k, v in pairs(ferny) do
               if type(v) == "function" then names[#names + 1] = "ferny." .. k end
               if type(v) == "table" then
                   for k2, v2 in pairs(v) do
                       if type(v2) == "function" then names[#names + 1] = "ferny." .. k .. "." .. k2 end
                   end
               end
           end
           table.sort(names)
           ferny.log(table.concat(names, ","))"#,
    );
    let manifest = discover(&dir).0.remove(0).1;
    let logged = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&logged);
    Plugin::start(&path, manifest, Vec::new(), "", move |m| {
        *sink.lock().expect("lock") = m.to_owned();
    })
    .expect("start");
    let actual: Vec<String> = logged
        .lock()
        .expect("lock")
        .split(',')
        .map(str::to_owned)
        .collect();
    assert_eq!(declared, actual);
}

fn bundled_plugin(id: &str, granted: &[&str], language: &str) -> Plugin {
    let (source, manifest) = bundled()
        .into_iter()
        .find(|(_, m)| m.id == id)
        .expect("bundled");
    let granted = granted.iter().map(|p| (*p).to_owned()).collect();
    Plugin::start_from(&source, manifest, granted, language, |_| {}).expect("start")
}

#[test]
fn bundled_plugins_load() {
    let ids: Vec<String> = bundled().into_iter().map(|(_, m)| m.id).collect();
    assert_eq!(
        ids,
        [
            "ferny-classbuilder",
            "ferny-htmlchars",
            "addon-addons",
            "addon-geniuspaste",
            "addon-lipsum",
            "addon-macros"
        ]
    );
    let html = bundled_plugin("ferny-htmlchars", &[], "es");
    assert_eq!(html.commands()[0].title, "Codificar entidades HTML");
    let run = |id: &str, selection: &str| {
        let d = DocInfo {
            selection: selection.into(),
            ..doc()
        };
        html.run_command(&format!("plugin.ferny-htmlchars.{id}"), Some(d))
            .expect("run")
    };
    assert_eq!(
        run("encode", "a\u{a0}é ÿ€ ✓<"),
        [Action::ReplaceSelection(
            "a&nbsp;&eacute; &yuml;&euro; &#10003;<".into()
        )]
    );
    assert_eq!(
        run("decode", "&eacute;&#233;&#xE9;&amp;&bogus;&#xD800;&#0;"),
        [Action::ReplaceSelection("ééé&&bogus;&#xD800;&#0;".into())]
    );
    assert_eq!(
        run("encode", ""),
        [Action::Status("Primero seleccione texto.".into())]
    );
}

#[test]
fn adapted_plugins_start_only_with_declared_permissions() {
    for (id, permissions) in [
        (
            "addon-addons",
            vec!["clipboard".to_owned(), "config".to_owned()],
        ),
        ("addon-geniuspaste", vec!["clipboard".to_owned()]),
        ("addon-lipsum", vec!["config".to_owned()]),
        ("addon-macros", vec!["config".to_owned()]),
    ] {
        let (source, manifest) = bundled()
            .into_iter()
            .find(|(_, m)| m.id == id)
            .expect("bundled");
        let plugin = Plugin::start_from_with_config(
            &source,
            manifest,
            permissions,
            "",
            Some(&temp(&format!("adapted-{id}"))),
            |_| {},
        );
        assert!(
            plugin.is_ok(),
            "{id} failed to start: {}",
            match plugin {
                Ok(_) => "ok".to_owned(),
                Err(error) => error.to_string(),
            }
        );
    }
}

#[test]
fn classbuilder_writes_new_files_in_the_project_only() {
    let project = temp("classbuilder");
    let cb = bundled_plugin(
        "ferny-classbuilder",
        &["fs.read.project", "fs.write.project"],
        "",
    );
    let run = |selection: &str| {
        let d = DocInfo {
            selection: selection.into(),
            ..doc()
        };
        cb.run_command("plugin.ferny-classbuilder.cpp_class", Some(d))
            .expect("run")
    };
    assert_eq!(
        run("Widget"),
        [Action::Message(
            "Cannot create the class: no trusted project.".into()
        )]
    );
    cb.set_project(Some(&project));
    assert_eq!(
        run("Widget"),
        [Action::Status("Created Widget.h and Widget.cpp.".into())]
    );
    let header = fs::read_to_string(project.join("Widget.h")).expect("header");
    assert!(header.contains("class Widget {") && header.contains("#ifndef WIDGET_H"));
    assert!(
        fs::read_to_string(project.join("Widget.cpp"))
            .expect("source")
            .contains("Widget::~Widget()")
    );
    fs::write(project.join("Widget.h"), "mine").expect("write");
    assert!(matches!(&run("Widget")[0], Action::Message(m) if m.contains("already exists")));
    assert_eq!(
        fs::read_to_string(project.join("Widget.h")).expect("header"),
        "mine"
    );
    assert_eq!(
        run("../x"),
        [Action::Status("Select a valid class name first.".into())]
    );
}

#[test]
fn reserved_ids_are_refused() {
    let dir = temp("reserved");
    plugin(&dir, "ferny-htmlchars", "", "ferny.log('fake')");
    plugin(&dir, "mine", "", "");
    let (found, warnings) = discover(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.id, "mine");
    assert_eq!(
        warnings,
        ["plugin ferny-htmlchars: reserved id, not loaded"]
    );
}

#[test]
fn new_decisions_keep_answers_and_reset_clears_them() {
    let m = parse_manifest(
        "id = \"p\"\nname = \"P\"\nversion = \"1\"\npermissions = [\"process.spawn\", \"fs.read.any\"]\n",
        "p",
    )
    .expect("manifest");
    let mut grants = Grants::default();
    grants.decide(&m, true, &["process.spawn".into(), "fs.read.any".into()]);
    assert!(grants.confirm(&m, "process.spawn", true));
    assert!(grants.confirm(&m, "fs.read.any", false));
    grants.decide(&m, true, &["process.spawn".into()]);
    let g = grants.decision(&m).expect("grant");
    assert_eq!(
        (g.confirmed.as_slice(), g.refused.len()),
        (["process.spawn".to_owned()].as_slice(), 0)
    );
    assert!(grants.reset_confirmations(&m));
    assert!(grants.decision(&m).expect("grant").confirmed.is_empty());
}

#[cfg(unix)]
#[test]
fn processes_and_their_children_end() {
    // A child that leaves a grandchild holding the pipes: the timeout ends
    // both (audit F2).
    let started = Instant::now();
    let outcome = run_process(
        Path::new("/bin/sh"),
        &["-c".to_owned(), "sleep 41 & echo $!; wait".to_owned()],
        &temp("group"),
        &[],
        Duration::from_millis(300),
    );
    assert!(outcome.timed_out);
    assert!(started.elapsed() < Duration::from_secs(5));
    let grandchild = String::from_utf8_lossy(&outcome.stdout).trim().to_owned();
    std::thread::sleep(Duration::from_millis(200));
    let alive = std::process::Command::new("/bin/kill")
        .args(["-0", &grandchild])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill");
    assert!(!alive.success(), "grandchild {grandchild} survived");
    // Quitting kills what still runs.
    let worker = std::thread::spawn(|| {
        run_process(
            Path::new("/bin/sleep"),
            &["30".to_owned()],
            &temp("quit"),
            &[],
            Duration::from_secs(60),
        )
    });
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    ferny_plugin::kill_all_processes();
    let outcome = worker.join().expect("join");
    assert!(outcome.code.is_none() && started.elapsed() < Duration::from_secs(5));
}
