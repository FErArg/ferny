-- Auto Close (ferny-autoclose): automatically closes brackets, quotes, and tags.
-- Modelled on Geany's autoclose plugin; written for Ferny.

local pairs = {
    ["("] = ")",
    ["["] = "]",
    ["{"] = "}",
    ['"'] = '"',
    ["'"] = "'",
    ["`"] = "`",
    ["<"] = ">",
}

local open_chars = "([{\"'`<"
local close_chars = ")]}>\"'`"

local function is_open_char(c)
    return open_chars:find(c, 1, true) ~= nil
end

local function is_close_char(c)
    return close_chars:find(c, 1, true) ~= nil
end

local function get_closing_char(open_char)
    return pairs[open_char]
end

local function get_opening_char(close_char)
    for k, v in pairs(pairs) do
        if v == close_char then
            return k
        end
    end
    return nil
end

local function is_matching_pair(open_char, close_char)
    return pairs[open_char] == close_char
end

-- Check if we're inside a string or comment (simplified)
local function get_char_at(text, pos)
    if pos < 1 or pos > #text then
        return nil
    end
    return text:sub(pos, pos)
end

local function should_auto_close(text, cursor_pos, char)
    -- Don't auto-close if next character is already the closing char
    local next_char = get_char_at(text, cursor_pos + 1)
    if next_char == get_closing_char(char) then
        return false
    end
    
    -- Don't auto-close quotes if preceded by backslash (escaped)
    if (char == '"' or char == "'" or char == "`") and cursor_pos > 1 then
        local prev_char = get_char_at(text, cursor_pos)
        if prev_char == "\\" then
            return false
        end
    end
    
    -- Don't auto-close if we have selected text (will be handled by selection enclosure)
    local sel = ferny.editor.selection()
    if sel ~= "" then
        return false
    end
    
    return true
end

local function on_char_typed(char)
    local text = ferny.editor.text()
    local cursor_pos = ferny.documents.current().column
    
    if is_open_char(char) and should_auto_close(text, cursor_pos, char) then
        local closing = get_closing_char(char)
        if closing then
            ferny.editor.insert(closing)
            -- Move cursor back between the pair
            ferny.command {
                id = "autoclose-move-back",
                title = "",
                run = function()
                    ferny.editor.replace_selection("")
                end
            }
            -- We need to move cursor left. This is a workaround since we can't directly move cursor.
            -- The insert will place cursor after the closing char, so we need a command to move back.
        end
    end
    
    -- Handle Enter key for brace indentation
    if char == "\n" then
        local line_start = cursor_pos
        while line_start > 1 and get_char_at(text, line_start - 1) ~= "\n" do
            line_start = line_start - 1
        end
        local line_text = text:sub(line_start, cursor_pos - 1)
        local indent = line_text:match("^(%s*)")
        local next_char = get_char_at(text, cursor_pos)
        
        if indent and next_char == "}" then
            -- Insert newline with proper indentation
            ferny.editor.insert("\n" .. indent .. "  ")
            -- Move cursor to the indented position
        end
    end
    
    -- Handle backspace to remove pairs
    if char == "\b" then -- backspace
        local prev_char = get_char_at(text, cursor_pos)
        local next_char = get_char_at(text, cursor_pos + 1)
        if prev_char and next_char and is_matching_pair(prev_char, next_char) then
            -- Delete both characters
            ferny.editor.replace_selection("")
        end
    end
end

-- Handle selection enclosure: when user types an opening char with selection, wrap selection
local function on_selection_enclose(char)
    if not is_open_char(char) then
        return
    end
    
    local sel = ferny.editor.selection()
    if sel == "" then
        return
    end
    
    local closing = get_closing_char(char)
    if closing then
        ferny.editor.replace_selection(char .. sel .. closing)
    end
end

-- Main command to handle auto-close behavior
ferny.on("document.before_save", function(doc)
    -- Could add cleanup of trailing whitespace here
    return nil
end)

-- Note: Ferny's API doesn't directly expose keypress events for auto-close.
-- This would need to be implemented via commands or when the API adds keybindings support.
-- For now, we provide commands that users can bind to keys.

ferny.command {
    id = "enclose-selection",
    title = ferny.tr("Enclose Selection with Brackets"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        -- Default to parentheses
        ferny.editor.replace_selection("(" .. sel .. ")")
    end,
}

ferny.command {
    id = "enclose-selection-square",
    title = ferny.tr("Enclose Selection with Square Brackets"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection("[" .. sel .. "]")
    end,
}

ferny.command {
    id = "enclose-selection-curly",
    title = ferny.tr("Enclose Selection with Curly Braces"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection("{" .. sel .. "}")
    end,
}

ferny.command {
    id = "enclose-selection-double-quote",
    title = ferny.tr("Enclose Selection with Double Quotes"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection('"' .. sel .. '"')
    end,
}

ferny.command {
    id = "enclose-selection-single-quote",
    title = ferny.tr("Enclose Selection with Single Quotes"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection("'" .. sel .. "'")
    end,
}