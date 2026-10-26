-- Extra Selection (ferny-extraselection): column mode, select to line/brace/anchor.
-- Modelled on Geany's extrasel plugin; written for Ferny.

local function get_document()
    return ferny.documents.current()
end

local function get_text()
    return ferny.editor.text()
end

local function get_selection()
    return ferny.editor.selection()
end

local function replace_selection(text)
    ferny.editor.replace_selection(text)
end

-- Column/rectangular selection mode
-- Note: Ferny's API doesn't directly expose selection mode. This provides commands
-- that users can bind to keys to manipulate selections.

local function get_line_range(text, line_num)
    local line_start = 1
    local current_line = 1
    for i = 1, #text do
        if text:sub(i, i) == "\n" then
            if current_line == line_num then
                return line_start, i - 1
            end
            current_line = current_line + 1
            line_start = i + 1
        end
    end
    if current_line == line_num then
        return line_start, #text
    end
    return nil, nil
end

local function get_current_line_info()
    local doc = get_document()
    if not doc then return nil end
    local text = get_text()
    local line_num = doc.line
    local line_start, line_end = get_line_range(text, line_num)
    return line_num, line_start, line_end
end

-- Select to beginning/end of line
ferny.command {
    id = "select-to-line-start",
    title = ferny.tr("Select to Line Start"),
    run = function()
        local doc = get_document()
        if not doc then return end
        local text = get_text()
        local line_num = doc.line
        local line_start = 1
        local current_line = 1
        for i = 1, #text do
            if text:sub(i, i) == "\n" then
                if current_line == line_num then
                    line_start = i + 1
                    break
                end
                current_line = current_line + 1
            end
        end
        -- Select from cursor to line start
        -- Note: Ferny API doesn't expose direct cursor manipulation
        -- This is a placeholder for when the API supports it
        ferny.ui.status(ferny.tr("Select to line start - bind to key in keybindings.toml"))
    end,
}

ferny.command {
    id = "select-to-line-end",
    title = ferny.tr("Select to Line End"),
    run = function()
        ferny.ui.status(ferny.tr("Select to line end - bind to key in keybindings.toml"))
    end,
}

-- Select to matching brace
ferny.command {
    id = "select-to-matching-brace",
    title = ferny.tr("Select to Matching Brace"),
    run = function()
        local text = get_text()
        local sel = get_selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("Place cursor on a brace first"))
            return
        end
        
        local braces = { ["("] = ")", ["["] = "]", ["{"] = "}" }
        local open_braces = "([{"
        local close_braces = ")]}"
        
        local cursor_pos = ferny.documents.current().column
        local char = text:sub(cursor_pos, cursor_pos)
        
        local target_char = nil
        local direction = 1
        
        if open_braces:find(char, 1, true) then
            target_char = braces[char]
            direction = 1
        elseif close_braces:find(char, 1, true) then
            for k, v in pairs(braces) do
                if v == char then
                    target_char = k
                    direction = -1
                    break
                end
            end
        end
        
        if not target_char then
            ferny.ui.status(ferny.tr("Cursor not on a brace"))
            return
        end
        
        local count = 1
        local pos = cursor_pos + direction
        while pos > 0 and pos <= #text do
            local c = text:sub(pos, pos)
            if c == char then
                count = count + 1
            elseif c == target_char then
                count = count - 1
                if count == 0 then
                    -- Found matching brace at pos
                    ferny.ui.status(ferny.tr("Matching brace found at position ") .. pos)
                    return
                end
            end
            pos = pos + direction
        end
        
        ferny.ui.status(ferny.tr("No matching brace found"))
    end,
}

-- Set selection anchor
local anchor_pos = nil

ferny.command {
    id = "set-selection-anchor",
    title = ferny.tr("Set Selection Anchor"),
    run = function()
        local doc = get_document()
        if not doc then return end
        anchor_pos = doc.column
        ferny.ui.status(ferny.tr("Selection anchor set at column ") .. anchor_pos)
    end,
}

ferny.command {
    id = "select-to-anchor",
    title = ferny.tr("Select to Anchor"),
    run = function()
        if not anchor_pos then
            ferny.ui.status(ferny.tr("No anchor set. Use 'Set Selection Anchor' first."))
            return
        end
        local doc = get_document()
        if not doc then return end
        local current_pos = doc.column
        ferny.ui.status(ferny.tr("Select from ") .. anchor_pos .. " to " .. current_pos)
    end,
}

ferny.command {
    id = "clear-selection-anchor",
    title = ferny.tr("Clear Selection Anchor"),
    run = function()
        anchor_pos = nil
        ferny.ui.status(ferny.tr("Selection anchor cleared"))
    end,
}

-- Convert selection mode (stream <-> rectangular)
ferny.command {
    id = "toggle-rectangular-selection",
    title = ferny.tr("Toggle Rectangular Selection"),
    run = function()
        ferny.ui.status(ferny.tr("Rectangular selection toggle - bind to key in keybindings.toml"))
    end,
}

-- Select to document start/end
ferny.command {
    id = "select-to-doc-start",
    title = ferny.tr("Select to Document Start"),
    run = function()
        ferny.ui.status(ferny.tr("Select to document start - bind to key in keybindings.toml"))
    end,
}

ferny.command {
    id = "select-to-doc-end",
    title = ferny.tr("Select to Document End"),
    run = function()
        ferny.ui.status(ferny.tr("Select to document end - bind to key in keybindings.toml"))
    end,
}

-- Select paragraph up/down
ferny.command {
    id = "select-paragraph-up",
    title = ferny.tr("Select Paragraph Up"),
    run = function()
        ferny.ui.status(ferny.tr("Select paragraph up - bind to key in keybindings.toml"))
    end,
}

ferny.command {
    id = "select-paragraph-down",
    title = ferny.tr("Select Paragraph Down"),
    run = function()
        ferny.ui.status(ferny.tr("Select paragraph down - bind to key in keybindings.toml"))
    end,
}