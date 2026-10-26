-- Line Operations (ferny-lineoperations): operaciones de línea (eliminar duplicados, ordenar, etc.)
-- Basado en el plugin "lineoperations" de Geany-Plugins (versión 2.1), escrito en C por Sylvan Mostert.
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- ============================================================
-- Funciones auxiliares
-- ============================================================

local function get_lines(text)
    local lines = {}
    for line in text:gmatch("([^\n]*)\n?") do
        table.insert(lines, line)
    end
    return lines
end

local function sort_table(t, ascending)
    local sorted = {}
    for _, v in ipairs(t) do
        table.insert(sorted, v)
    end
    table.sort(sorted, function(a, b)
        if ascending then
            return a < b
        else
            return a > b
        end
    end)
    return sorted
end

local function remove_duplicates_sorted(lines)
    local unique = {}
    local seen = {}
    for _, line in ipairs(lines) do
        if not seen[line] then
            seen[line] = true
            table.insert(unique, line)
        end
    end
    return sort_table(unique, true)
end

local function remove_duplicates_ordered(lines)
    local unique = {}
    local seen = {}
    for _, line in ipairs(lines) do
        if not seen[line] then
            seen[line] = true
            table.insert(unique, line)
        end
    end
    return unique
end

local function remove_unique(lines)
    local count = {}
    for _, line in ipairs(lines) do
        count[line] = (count[line] or 0) + 1
    end
    local result = {}
    for _, line in ipairs(lines) do
        if count[line] > 1 then
            table.insert(result, line)
        end
    end
    return result
end

local function keep_unique(lines)
    local count = {}
    for _, line in ipairs(lines) do
        count[line] = (count[line] or 0) + 1
    end
    local result = {}
    for _, line in ipairs(lines) do
        if count[line] == 1 then
            table.insert(result, line)
        end
    end
    return result
end

local function remove_empty(lines)
    local result = {}
    for _, line in ipairs(lines) do
        if line:match("%S") then
            table.insert(result, line)
        end
    end
    return result
end

local function remove_whitespace(lines)
    local result = {}
    for _, line in ipairs(lines) do
        if line:match("%S") then
            table.insert(result, line)
        end
    end
    return result
end

local function sort_ascending(lines)
    return sort_table(lines, true)
end

local function sort_descending(lines)
    return sort_table(lines, false)
end

-- ============================================================
-- Comandos
-- ============================================================

ferny.command {
    id = "remove-dup-sorted",
    title = ferny.tr("Remove Duplicate Lines (Sorted)"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = remove_duplicates_sorted(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Removed ") .. (#lines - #result) .. ferny.tr(" duplicate lines (sorted)"))
    end,
}

ferny.command {
    id = "remove-dup-ordered",
    title = ferny.tr("Remove Duplicate Lines (Ordered)"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = remove_duplicates_ordered(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Removed ") .. (#lines - #result) .. ferny.tr(" duplicate lines (ordered)"))
    end,
}

ferny.command {
    id = "remove-unique",
    title = ferny.tr("Remove Unique Lines"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = remove_unique(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Removed ") .. (#lines - #result) .. ferny.tr(" unique lines"))
    end,
}

ferny.command {
    id = "keep-unique",
    title = ferny.tr("Keep Unique Lines"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = keep_unique(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Kept ") .. #result .. ferny.tr(" unique lines"))
    end,
}

ferny.command {
    id = "remove-empty",
    title = ferny.tr("Remove Empty Lines"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = remove_empty(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Removed ") .. (#lines - #result) .. ferny.tr(" empty lines"))
    end,
}

ferny.command {
    id = "remove-whitespace",
    title = ferny.tr("Remove Whitespace-Only Lines"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = remove_whitespace(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Removed ") .. (#lines - #result) .. ferny.tr(" whitespace-only lines"))
    end,
}

ferny.command {
    id = "sort-ascending",
    title = ferny.tr("Sort Lines Ascending"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = sort_ascending(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Lines sorted ascending"))
    end,
}

ferny.command {
    id = "sort-descending",
    title = ferny.tr("Sort Lines Descending"),
    run = function()
        local text = ferny.editor.text()
        local lines = get_lines(text)
        local result = sort_descending(lines)
        ferny.editor.replace_selection(table.concat(result, "\n"))
        ferny.ui.status(ferny.tr("Lines sorted descending"))
    end,
}

ferny.log("Line Operations plugin loaded")