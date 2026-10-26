# Writing Ferny plugins (API 0)

API 0 is the pre-1.0 plugin API: additions may still change before Ferny
1.0 freezes it as 1.0. Type stubs for LuaLS:
[`data/plugin-api/ferny.d.lua`](../data/plugin-api/ferny.d.lua).

## Layout

A plugin is a folder in `plugins/` of Ferny's configuration folder (never
in a project):

```text
plugins/wordcount/
  plugin.toml
  main.lua
  locale/es.toml      # optional: "English text" = "translation"
```

```toml
# plugin.toml — id must equal the folder name ([a-z0-9_-], ≤ 64)
id = "wordcount"
name = "Word Count"
version = "1.0"
permissions = []      # config, fs.read.project, fs.read.any, fs.write.project,
                      # process.spawn, clipboard (net has no API yet)
```

The first time Ferny finds a plugin, or when its permissions change, it
asks whether to enable it and which permissions to grant. `--safe-mode`
starts without plugins.

## Bundled plugins and the manager

Ferny ships HTML Characters (`ferny-htmlchars`) and Class Builder
(`ferny-classbuilder`), compiled into the application and disabled until
you explicitly enable them in Tools → Plugins…. The manager lists every plugin with
its origin and status, lets you enable it and tick its permissions, makes
a plugin ask again before its first use of a sensitive permission, and
opens the plugins folder. Changes apply at the next start. Plugin ids
starting with `ferny-` are reserved.

The external `ferny-plugins` collection is not bundled automatically and is
not loadable unchanged from the plugins folder because its `ferny-*` IDs are
reserved for embedded plugins. It must be adapted, renamed, provenance-gated
and explicitly registered before distribution. Its
source and compatibility gate are recorded in the maintainer's
review; plugins remain excluded
until provenance, licence, API and permission checks pass.

The current source collection declares `GPL-2.0-or-later` in each README and
claims adaptation from Geany-Plugins, but provides no root licence file,
upstream commit mapping or authoritative per-plugin attribution. This is
insufficient for bundled inclusion; treat all 22 candidates as quarantined
until those records are independently verified.

Adapted test candidates use the non-reserved `addon-*` namespace. Current
registered entries are `addon-addons`, `addon-lipsum`, `addon-macros` and
`addon-geniuspaste`; they are bundled for development testing but remain
disabled by default. They are not release-approved until per-plugin
provenance, licence and resource evidence is complete.

## Sandbox and limits

- Lua 5.4 with `table`, `string`, `math` and `utf8` only: no `io`, `os`,
  `debug`, `coroutine`, `require`, `load` or bytecode.
- 50 ms per call (loading, an event, a command, a provider, a callback),
  64 MB of memory. `string.find`, `match`, `gmatch` and `gsub` are
  Ferny's own implementation of Lua 5.4 patterns, so that the limit also
  applies to them; results are the same, but an error from a tail call
  (`return s:find("%")`) has no position prefix. A plugin that runs out of time is disabled until Ferny
  restarts.
- `main.lua` runs once at start; `ferny.on`, `ferny.command` and
  `ferny.provide` work only then. Files, processes and permissions that
  need confirmation work from events, commands, providers and callbacks.

## API

| Call | Notes |
|---|---|
| `ferny.log(text)` | To `ferny.log`; never log document contents |
| `ferny.tr(text)` | The plugin's translation of `text` |
| `ferny.has_permission(name)` | |
| `ferny.on(event, fn)` | `app.startup_complete`, `document.opened` / `activated` / `saved` / `closed` (`fn({path, filetype})`), `document.before_save` (`fn({path, filetype, text})` may return a new text; on Save, not Save As) |
| `ferny.command{id, title, run}` | Under Tools → plugin name; id `plugin.<plugin>.<id>`; keys only from the user's `keybindings.toml` |
| `ferny.provide(kind, {filetypes, run})` | `completion` (list of strings, merged with Ferny's), `hover` (string), `format` (whole new text); `nil` lets Ferny's own providers answer; `filetypes = {"*"}` for all |
| `ferny.documents.current()` | `{path, filetype, line, column}` or `nil` |
| `ferny.editor.text()` / `selection()` / `insert(t)` / `replace_selection(t)` | In commands only; edits are one undo action, never saved |
| `ferny.ui.status(t)` / `ferny.ui.message(t)` | Shown as plain, escaped text |
| `ferny.fs.read(p)` / `list(p)` / `write(p, t)` | Relative paths from the open, trusted project; return `nil, error` on failure |
| `ferny.config.read()` / `write(value)` | Private per-plugin value, bounded and owner-only; requires `config`; no path argument |
| `ferny.spawn{program, args, on_exit}` | No shell; `on_exit({code, timed_out, stdout, stderr, error})`; 30 s, 512 KiB per stream, 4 at a time |
| `ferny.clipboard.set(t)` / `get()` | `get` in commands only |

Limits per call: 64 actions, 1 MiB of edits, 4 KiB messages, 256-byte
status texts; 500 completion items of 256 bytes; 4 KiB hovers; 16 MiB for
formatted or saved text; 32 commands, 64 handlers and 16 providers per
plugin.

## Example

```lua
ferny.command {
    id = "count",
    title = ferny.tr("Count Words"),
    run = function()
        local n = 0
        for _ in ferny.editor.text():gmatch("%S+") do n = n + 1 end
        ferny.ui.status(ferny.tr("Words:") .. " " .. n)
    end,
}

ferny.on("document.before_save", function(doc)
    return (doc.text:gsub("[ \t]+\n", "\n"))
end)
```
