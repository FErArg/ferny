---@meta
-- LuaLS type stubs for the Ferny plugin API 0 (H3.2–H3.3c). Put this file
-- in your editor's Lua library path; plugins never load it. Checked
-- against the real API by crates/ferny-plugin/tests/plugin.rs.

---@class ferny
ferny = {}

---API version (0 until Ferny 1.0).
---@type integer
ferny.api_version = 0

---Writes a line to Ferny's local log (`ferny.log`); never log contents.
---@param message string
function ferny.log(message) end

---Whether the user granted `permission` to this plugin.
---@param permission string
---@return boolean
function ferny.has_permission(permission) end

---Translates `text` with the plugin's `locale/<language>.toml`.
---@param text string
---@return string
function ferny.tr(text) end

---Handles an event (only while `main.lua` loads). `document.before_save`
---handlers get `{path, filetype, text}` and may return a new text.
---@param event "app.startup_complete"|"document.opened"|"document.activated"|"document.before_save"|"document.saved"|"document.closed"
---@param handler fun(doc: {path: string, filetype: string, text?: string}?): string?
function ferny.on(event, handler) end

---Adds a command under Tools (only while `main.lua` loads). Its id is
---`plugin.<plugin id>.<id>`; users bind keys to it in `keybindings.toml`.
---@param spec {id: string, title: string, run: fun()}
function ferny.command(spec) end

---Provides completion, hover or formatting for filetypes (only while
---`main.lua` loads). `run` returns a list of strings (completion), a
---string (hover, or the whole formatted text), or nil to let Ferny's own
---providers answer.
---@param kind "completion"|"hover"|"format"
---@param spec {filetypes: string[], run: fun(ctx: {path: string, filetype: string, line: integer, column: integer, text: string, word: string}): (string[]|string|nil)}
function ferny.provide(kind, spec) end

ferny.documents = {}

---The current document, or nil.
---@return {path: string, filetype: string, line: integer, column: integer}?
function ferny.documents.current() end

---Editing the current document; inside commands only.
ferny.editor = {}

---@return string
function ferny.editor.text() end

---@return string
function ferny.editor.selection() end

---@param text string
function ferny.editor.insert(text) end

---@param text string
function ferny.editor.replace_selection(text) end

ferny.ui = {}

---@param text string
function ferny.ui.status(text) end

---@param text string
function ferny.ui.message(text) end

---Files; relative paths start at the open, trusted project. Permissions
---`fs.read.project`, `fs.read.any` (confirmed at first use) and
---`fs.write.project`.
ferny.fs = {}

---@param path string
---@return string? text
---@return string? error
function ferny.fs.read(path) end

---@param path string
---@return {name: string, kind: "file"|"dir"|"link"|"other"}[]? entries
---@return string? error
function ferny.fs.list(path) end

---@param path string
---@param text string
---@return boolean? ok
---@return string? error
function ferny.fs.write(path, text) end

---Private per-plugin configuration (permission `config`).
ferny.config = {}
---@return string? value
function ferny.config.read() end
---@param value string
---@return boolean ok
function ferny.config.write(value) end

---Runs a program without a shell (permission `process.spawn`, confirmed
---at first use); the result arrives later in `on_exit`.
---@param spec {program: string, args?: string[], on_exit?: fun(result: {code: integer?, timed_out: boolean, stdout: string, stderr: string, error: string?})}
---@return boolean? ok
---@return string? error
function ferny.spawn(spec) end

---Clipboard (permission `clipboard`); `get` inside commands only.
ferny.clipboard = {}

---@param text string
function ferny.clipboard.set(text) end

---@return string
function ferny.clipboard.get() end
