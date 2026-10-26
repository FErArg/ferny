//! Budgeted Lua patterns (phase 3 audit, F1): same results as Lua 5.4's
//! C implementation, and a time limit the C version does not have.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::Lua;

/// Serialises the results of `pcall(chunk)` (types, values, error text).
const SER: &str = r#"
local function ser(ok, ...)
    local t = table.pack(...)
    local out = { tostring(ok) }
    for i = 1, t.n do
        local v = t[i]
        if not ok then v = tostring(v) end
        out[#out + 1] = type(v) .. ":" .. tostring(v)
    end
    return table.concat(out, "|")
end
return function(code) return ser(pcall(load(code))) end
"#;

fn runner(lua: &Lua) -> mlua::Function {
    lua.load(SER).eval().expect("runner")
}

fn budgeted(deadline: Option<Instant>) -> Lua {
    let lua = Lua::new();
    ferny_plugin::pattern::install(&lua, Arc::new(Mutex::new(deadline))).expect("install");
    lua
}

/// Runs every case in C Lua and in the port; returns the mismatches.
fn compare(cases: &[&str]) -> Vec<String> {
    let (c, rust) = (Lua::new(), budgeted(None));
    let (run_c, run_rust) = (runner(&c), runner(&rust));
    cases
        .iter()
        .filter_map(|code| {
            // Not a tail call: Lua drops the caller's frame of a tail-called
            // Lua function, and with it the error's position (documented).
            let code = match code.strip_prefix("return ") {
                Some(call) => {
                    format!("local r = table.pack({call}) return table.unpack(r, 1, r.n)")
                }
                None => (*code).to_owned(),
            };
            let code = code.as_str();
            let a: mlua::LuaString = run_c.call(code).expect("c");
            let b: mlua::LuaString = run_rust.call(code).expect("rust");
            let (a, b) = (a.as_bytes().to_vec(), b.as_bytes().to_vec());
            let (a, b) = (String::from_utf8_lossy(&a), String::from_utf8_lossy(&b));
            (a != b).then(|| format!("{code}\n  C:    {a}\n  Rust: {b}"))
        })
        .collect()
}

#[test]
fn results_match_lua() {
    let cases = [
        r#"return string.find("hello world", "o w")"#,
        r#"return string.find("hello world", "o", 6)"#,
        r#"return string.find("hello", "l", -2)"#,
        r#"return string.find("hello", "", 10)"#,
        r#"return string.find("hello", "", 6)"#,
        r#"return string.find("a.b", ".", 1, true)"#,
        r#"return string.find("hello", "(l)(l)")"#,
        r#"return string.find("hello", "()ll()")"#,
        r#"return string.find("  trim  ", "^%s*(.-)%s*$")"#,
        r#"return string.match("  trim me  ", "^%s*(.-)%s*$")"#,
        r#"return string.match("key = value", "(%w+)%s*=%s*(%w+)")"#,
        r#"return string.match("THE (quick) fox", "%((%a+)%)")"#,
        r#"return string.match("f(a(b)c)d", "%b()")"#,
        r#"return string.match("THE quick", "%f[%a]%a+", 4)"#,
        r#"return string.match("hello", ".-l")"#,
        r#"return string.match("hello", ".*l")"#,
        r#"return string.match("hello", "x?h")"#,
        r#"return string.match("aaa", "a+$")"#,
        r#"return string.match("abcabc", "(abc)%1")"#,
        r#"return string.match("[x]", "[%]]")"#,
        r#"return string.match("a-b", "[a%-]+")"#,
        r#"return string.match("Z9_", "[^%d]+")"#,
        r#"return string.match("x$y", "x$y")"#,
        r#"return string.match("abc", "^(a)(b)(c)$")"#,
        r#"return string.match(123456, "%d%d")"#,
        r#"return string.gsub("hello world", "o", "0")"#,
        r#"return string.gsub("hello world", "(o)", "[%1%0%%]")"#,
        r#"return string.gsub("hello", "", "-")"#,
        r#"return string.gsub("abc", "%w", "%0%0", 2)"#,
        r#"return string.gsub("$name is $age", "%$(%w+)", { name = "Ana", age = 7 })"#,
        r#"return string.gsub("a b c", "%w", function(c) if c ~= "b" then return c:upper() end end)"#,
        r#"return string.gsub("abc", "()", "%1")"#,
        r#"return string.gsub("hello", "^h", "H")"#,
        r#"return string.gsub("x = 1", "%s", 7)"#,
        r#"local t = {} for k, v in string.gmatch("a=1, b=2", "(%w+)=(%w+)") do t[#t + 1] = k .. v end return table.concat(t, ",")"#,
        r#"local t = {} for w in string.gmatch("one two  three", "%a+") do t[#t + 1] = w end return table.concat(t, ",")"#,
        r#"local t = {} for w in string.gmatch("abc", "") do t[#t + 1] = "." end return #t"#,
        r#"local t = {} for w in string.gmatch("hello", "l", 4) do t[#t + 1] = w end return #t"#,
        r#"return ("method"):find("th")"#,
        // Found by the lua_patterns fuzz target.
        r#"return string.gsub("\1\0\0\1\255\0z[", "[[%z]", "<%0>")"#,
        r#"return string.find("a\0b", "%z")"#,
        r#"return string.find("a\0b", "[%z]")"#,
        r#"return ("x"):rep(3):gsub("x", "y")"#,
        // Errors.
        r#"return string.find("a", "%")"#,
        r#"return string.find("a", "[a")"#,
        r#"return string.find("a", "(a")"#,
        r#"return string.find("a", "a)")"#,
        r#"return string.find("a", "%1")"#,
        r#"return string.find("a", "%b")"#,
        r#"return string.find("a", "%fa")"#,
        r#"return string.gsub("a", "a", "%2")"#,
        r#"return string.gsub("a", "a", "%")"#,
        r#"return string.gsub("a", "a", "%x")"#,
        r#"return string.gsub("a", "a", true)"#,
        r#"return string.gsub("a", "a", function() return {} end)"#,
        r#"return string.find("a", string.rep("(", 40) .. "a" .. string.rep(")", 40))"#,
        r#"return string.match(string.rep("a", 300), string.rep("a?", 300))"#,
    ];
    let mismatches = compare(&cases);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn backtracking_hits_the_time_limit() {
    let lua = budgeted(Some(Instant::now() + Duration::from_millis(50)));
    let started = Instant::now();
    let err = lua
        .load(r#"return string.rep("a", 3000):find(".-.-.-.-b")"#)
        .exec()
        .expect_err("limit");
    assert!(
        err.to_string().contains("ferny: plugin time limit"),
        "{err}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}
