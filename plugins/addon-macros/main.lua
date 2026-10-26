-- Macros (addon_macros): record and replay editor macros.
-- Modelled on Geany's geanymacro plugin; written for Ferny.

local Macro = {}
Macro.__index = Macro

function Macro:new()
    local m = {
        name = "",
        keyval = 0,
        state = 0,
        events = {},
        recording = false,
    }
    setmetatable(m, Macro)
    return m
end

function Macro:add_event(message, wparam, lparam)
    table.insert(self.events, {
        message = message,
        wparam = wparam,
        lparam = lparam,
    })
end

function Macro:clear()
    self.events = {}
end

function Macro:to_string()
    local parts = {}
    table.insert(parts, "name=" .. self.name)
    table.insert(parts, "keyval=" .. self.keyval)
    table.insert(parts, "state=" .. self.state)
    for _, e in ipairs(self.events) do
        local event_str = string.format("%d,%d,%s", e.message, e.wparam, e.lparam or "nil")
        table.insert(parts, event_str)
    end
    return table.concat(parts, "|")
end

-- Scintilla message constants (subset relevant for macros)
local SCI = {
    CUT = 2177,
    COPY = 2178,
    PASTE = 2179,
    CLEAR = 2180,
    LINECUT = 2337,
    LINECOPY = 2338,
    LINEDELETE = 2339,
    LINETRANSPOSE = 2340,
    LINEDUPLICATE = 2404,
    SELECTIONDUPLICATE = 2469,
    DELETEBACK = 2326,
    DELETEBACKNOTLINE = 2344,
    DELWORDLEFT = 2335,
    DELWORDRIGHT = 2336,
    DELWORDRIGHTEND = 2518,
    DELLINELEFT = 2395,
    DELLINERIGHT = 2396,
    VCHOME = 2331,
    HOME = 2312,
    LINEEND = 2313,
    DOCUMENTSTART = 2314,
    DOCUMENTEND = 2315,
    PAGEUP = 2316,
    PAGEDOWN = 2317,
    LINEUP = 2300,
    LINEDOWN = 2301,
    CHARLEFT = 2304,
    CHARRIGHT = 2305,
    WORDLEFT = 2308,
    WORDRIGHT = 2309,
    WORDPARTLEFT = 2390,
    WORDPARTRIGHT = 2391,
    HOMEDISPLAY = 2345,
    LINEENDDISPLAY = 2346,
    VCHOMEDISPLAY = 2652,
    PARADOWN = 2413,
    PARAUP = 2412,
    LINEDOWNEXTEND = 2302,
    LINEUPEXTEND = 2303,
    CHARLEFTEXTEND = 2306,
    CHARRIGHTEXTEND = 2307,
    WORDLEFTEXTEND = 2310,
    WORDRIGHTEXTEND = 2311,
    WORDPARTLEFTEXTEND = 2392,
    WORDPARTRIGHTEXTEND = 2393,
    HOMEEXTEND = 2318,
    LINEENDEXTEND = 2319,
    DOCUMENTSTARTEXTEND = 2320,
    DOCUMENTENDEXTEND = 2321,
    PAGEUPEXTEND = 2322,
    PAGEDOWNEXTEND = 2323,
    HOMEDISPLAYEXTEND = 2347,
    LINEENDDISPLAYEXTEND = 2348,
    VCHOMEEXTEND = 2332,
    PARADOWNEXTEND = 2415,
    PARAUPEXTEND = 2414,
    TAB = 2327,
    NEWLINE = 2329,
    REPLACESEL = 2170,
    LOWERCASE = 2340,
    UPPERCASE = 2341,
    CANCEL = 2171,
    EDITTOGGLEOVERTYPE = 2323,
    SEARCHANCHOR = 2366,
    SEARCHNEXT = 2367,
    SEARCHPREV = 2368,
    STARTRECORD = 2342,
    STOPRECORD = 2343,
}

-- Human_readable descriptions for events
local EVENT_DESCRIPTIONS = {
    [SCI.CUT] = "Cut to Clipboard",
    [SCI.COPY] = "Copy to Clipboard",
    [SCI.PASTE] = "Paste from Clipboard",
    [SCI.CLEAR] = "Delete character to the right",
    [SCI.LINECUT] = "Cut current line to Clipboard",
    [SCI.LINECOPY] = "Copy current line to Clipboard",
    [SCI.LINEDELETE] = "Delete current line",
    [SCI.LINETRANSPOSE] = "Swap current line with one above",
    [SCI.LINEDUPLICATE] = "Insert duplicate of current line below",
    [SCI.SELECTIONDUPLICATE] = "Insert duplicate of selected text after selection",
    [SCI.DELETEBACK] = "Delete character to the left",
    [SCI.DELETEBACKNOTLINE] = "Delete character to the left (but not newline)",
    [SCI.DELWORDLEFT] = "Delete up to start of word to the Left",
    [SCI.DELWORDRIGHT] = "Delete up to start of word to the Right",
    [SCI.DELWORDRIGHTEND] = "Delete up to end of word to the Right",
    [SCI.DELLINELEFT] = "Delete to beginning of line",
    [SCI.DELLINERIGHT] = "Delete to end of line",
    [SCI.VCHOME] = "Move to first non_whitespace character of line",
    [SCI.HOME] = "Move to start of line",
    [SCI.LINEEND] = "Move to end of line",
    [SCI.DOCUMENTSTART] = "Move to first line of document",
    [SCI.DOCUMENTEND] = "Move to last line of document",
    [SCI.PAGEUP] = "Move up one page",
    [SCI.PAGEDOWN] = "Move down one page",
    [SCI.LINEUP] = "Move up one line",
    [SCI.LINEDOWN] = "Move down one line",
    [SCI.CHARLEFT] = "Move left one character",
    [SCI.CHARRIGHT] = "Move right one character",
    [SCI.WORDLEFT] = "Move to start of word to the left",
    [SCI.WORDRIGHT] = "Move to start of word to the right",
    [SCI.WORDPARTLEFT] = "Move to start of part of word to the left",
    [SCI.WORDPARTRIGHT] = "Move to start of part of word to the right",
    [SCI.HOMEDISPLAY] = "Move to first visible character",
    [SCI.LINEENDDISPLAY] = "Move to last visible character",
    [SCI.PARADOWN] = "Move to beginning of next paragraph",
    [SCI.PARAUP] = "Move up to beginning of current/previous paragraph",
    [SCI.LINEDOWNEXTEND] = "Extend selection down one line",
    [SCI.LINEUPEXTEND] = "Extend selection up one line",
    [SCI.CHARLEFTEXTEND] = "Extend selection left one character",
    [SCI.CHARRIGHTEXTEND] = "Extend selection right one character",
    [SCI.WORDLEFTEXTEND] = "Extend selection to start of word to the left",
    [SCI.WORDRIGHTEXTEND] = "Extend selection to start of word to the right",
    [SCI.HOMEEXTEND] = "Extend selection to start of line",
    [SCI.LINEENDEXTEND] = "Extend selection to end of line",
    [SCI.DOCUMENTSTARTEXTEND] = "Extend selection to start of document",
    [SCI.DOCUMENTENDEXTEND] = "Extend selection to end of document",
    [SCI.PAGEUPEXTEND] = "Extend selection up one page",
    [SCI.PAGEDOWNEXTEND] = "Extend selection down one page",
    [SCI.TAB] = "Tab",
    [SCI.NEWLINE] = "Newline",
    [SCI.REPLACESEL] = "Insert/replace text",
    [SCI.LOWERCASE] = "Change selected text to lowercase",
    [SCI.UPPERCASE] = "Change selected text to uppercase",
    [SCI.CANCEL] = "Cancel selection",
    [SCI.EDITTOGGLEOVERTYPE] = "Toggle insert/overwrite mode",
    [SCI.SEARCHANCHOR] = "Set search anchor",
    [SCI.SEARCHNEXT] = "Search next",
    [SCI.SEARCHPREV] = "Search previous",
}

local macros = {}
local recording_macro = nil

local MAX_MACROS = 64
local MAX_EVENTS = 512
local MAX_NAME = 128
local MAX_CONFIG = 256 * 1024

local function field(value)
    value = tostring(value or "")
    if value:find("[|;=\r\n]") then error("invalid macro field") end
    return value
end

local function number(value)
    local n = tonumber(value)
    if not n or n % 1 ~= 0 or n < _2147483648 or n > 2147483647 then
        error("invalid macro number")
    end
    return n
end

local function decode_macros(text)
    if #text > MAX_CONFIG or text == "" then return {} end
    local result = {}
    for record in text:gmatch("([^|]+)") do
        if #result >= MAX_MACROS then error("too many macros") end
        local name, keyval, state, events = record:match("^name=([^;]*);keyval=([^;]*);state=([^;]*);events=(.*)$")
        if not name or #name > MAX_NAME then error("invalid macro") end
        local macro = {name = name, keyval = number(keyval), state = number(state), events = {}}
        if events ~= "" then
            for event in events:gmatch("([^;]+)") do
                if #macro.events >= MAX_EVENTS then error("too many events") end
                local message, wparam, lparam = event:match("^([^,]+),([^,]+),(.*)$")
                if not message then error("invalid event") end
                table.insert(macro.events, {message = number(message), wparam = number(wparam), lparam = lparam == "nil" and nil or field(lparam)})
            end
        end
        table.insert(result, macro)
    end
    return result
end

local function encode_macros(data)
    if #data > MAX_MACROS then error("too many macros") end
    local records = {}
    for _, macro in ipairs(data) do
        if #field(macro.name) > MAX_NAME or #macro.events > MAX_EVENTS then error("invalid macro") end
        local events = {}
        for _, event in ipairs(macro.events) do
            table.insert(events, number(event.message) .. "," .. number(event.wparam) .. "," .. field(event.lparam or "nil"))
        end
        table.insert(records, "name=" .. field(macro.name) .. ";keyval=" .. number(macro.keyval) .. ";state=" .. number(macro.state) .. ";events=" .. table.concat(events, ";"))
    end
    local result = table.concat(records, "|")
    if #result > MAX_CONFIG then error("config too large") end
    return result
end

-- Load macros from config
local function load_macros()
    local content = ferny.config.read()
    if not content then
        return {}
    end

    local ok, data = pcall(decode_macros, content)
    if not ok then
        return {}
    end

    for _, m_data in ipairs(data) do
        local m = Macro:new()
        m.name = m_data.name
        m.keyval = m_data.keyval
        m.state = m_data.state
        m.events = m_data.events or {}
        table.insert(macros, m)
    end

    return macros
end

-- Save macros to config
local function save_macros()
    local data = {}
    for _, m in ipairs(macros) do
        table.insert(data, {
            name = m.name,
            keyval = m.keyval,
            state = m.state,
            events = m.events,
        })
    end

    local ok, content = pcall(function() return encode_macros(data) end)
    if ok then
        ferny.config.write(content)
    end
end

-- Find macro by name
local function find_macro_by_name(name)
    for _, m in ipairs(macros) do
        if m.name == name then
            return m
        end
    end
    return nil
end

-- Find macro by key combination
local function find_macro_by_key(keyval, state)
    for _, m in ipairs(macros) do
        if m.keyval == keyval and m.state == state then
            return m
        end
    end
    return nil
end

-- Record a macro event
local function record_event(message, wparam, lparam)
    if recording_macro then
        recording_macro:add_event(message, wparam, lparam)
    end
end

-- Replay a macro
local function replay_macro(m)
    if not m or #m.events == 0 then
        ferny.ui.status(ferny.tr("Macro is empty"))
        return
    end

    ferny.editor.replace_selection("") -- Ensure no selection interferes

    for _, event in ipairs(m.events) do
        local desc = EVENT_DESCRIPTIONS[event.message] or ("Event " .. event.message)
        ferny.log("Replaying: " .. desc)

        -- We can't directly send Scintilla messages from Lua
        -- This would need to be handled by Ferny's native code
        -- For now, we log what would be replayed
    end

    ferny.ui.status(ferny.tr("Replayed macro: ") .. m.name)
end

-- Start recording
ferny.command {
    id = "start_recording",
    title = ferny.tr("Start Recording Macro"),
    run = function()
        if recording_macro then
            ferny.ui.status(ferny.tr("Already recording"))
            return
        end

        -- Ask for macro name and trigger
        -- For simplicity, we'll create a macro with a default name
        recording_macro = Macro:new()
        recording_macro.name = "Macro " .. (#macros + 1)
        recording_macro.recording = true

        ferny.ui.status(ferny.tr("Recording started. Use 'Stop Recording Macro' when done."))
    end,
}

-- Stop recording
ferny.command {
    id = "stop_recording",
    title = ferny.tr("Stop Recording Macro"),
    run = function()
        if not recording_macro then
            ferny.ui.status(ferny.tr("Not recording"))
            return
        end

        recording_macro.recording = false

        if #recording_macro.events == 0 then
            ferny.ui.status(ferny.tr("No events recorded, discarding"))
            recording_macro = nil
            return
        end

        table.insert(macros, recording_macro)
        save_macros()

        ferny.ui.status(ferny.tr("Recorded macro: ") .. recording_macro.name .. " (" .. #recording_macro.events .. " events)")
        recording_macro = nil
    end,
}

-- Play macro by name
ferny.command {
    id = "play_macro",
    title = ferny.tr("Play Macro"),
    run = function()
        if #macros == 0 then
            ferny.ui.status(ferny.tr("No macros recorded"))
            return
        end

        -- For now, play the first macro
        -- In a real implementation, you'd show a list to choose from
        replay_macro(macros[1])
    end,
}

-- List macros
ferny.command {
    id = "list_macros",
    title = ferny.tr("List Macros"),
    run = function()
        if #macros == 0 then
            ferny.ui.status(ferny.tr("No macros recorded"))
            return
        end

        local lines = { ferny.tr("Recorded macros:") }
        for i, m in ipairs(macros) do
            local key_str = ""
            if m.keyval > 0 then
                key_str = " [" .. m.keyval .. "]"
            end
            table.insert(lines, string.format("  %d. %s%s (%d events)", i, m.name, key_str, #m.events))
        end

        ferny.ui.message(table.concat(lines, "\n"))
    end,
}

-- Delete macro
ferny.command {
    id = "delete_macro",
    title = ferny.tr("Delete Macro"),
    run = function()
        if #macros == 0 then
            ferny.ui.status(ferny.tr("No macros to delete"))
            return
        end

        -- Delete the first macro for now
        local removed = table.remove(macros, 1)
        save_macros()
        ferny.ui.status(ferny.tr("Deleted macro: ") .. removed.name)
    end,
}

-- Edit macro name
ferny.command {
    id = "rename_macro",
    title = ferny.tr("Rename Macro"),
    run = function()
        if #macros == 0 then
            ferny.ui.status(ferny.tr("No macros to rename"))
            return
        end

        local m = macros[1]
        local old_name = m.name
        m.name = old_name .. " (renamed)"
        save_macros()
        ferny.ui.status(ferny.tr("Renamed macro: ") .. old_name .. " _> " .. m.name)
    end,
}

-- Set macro trigger key (placeholder)
ferny.command {
    id = "set_macro_trigger",
    title = ferny.tr("Set Macro Trigger Key"),
    run = function()
        ferny.ui.status(ferny.tr("Set trigger key _ bind in keybindings.toml"))
    end,
}

-- Clear all macros
ferny.command {
    id = "clear_macros",
    title = ferny.tr("Clear All Macros"),
    run = function()
        if #macros == 0 then
            ferny.ui.status(ferny.tr("No macros to clear"))
            return
        end

        macros = {}
        save_macros()
        ferny.ui.status(ferny.tr("All macros cleared"))
    end,
}

-- Load macros on startup
load_macros()

ferny.log("Macros plugin loaded. " .. #macros .. " macros available.")
