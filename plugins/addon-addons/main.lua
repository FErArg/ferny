-- Addons (ferny-addons): colección de pequeñas utilidades para Ferny.
-- Basado en el plugin "Addons" de Geany-Plugins (versión 2.1), escrito en C por
-- Enrico Tröger, Bert Vermeulen, Eugene Arshinov, Frank Lanitz.
-- Adaptado para Ferny usando la API de plugins Lua (API 0).
--
-- NOTA: Se eliminó require("json") porque no está disponible en el sandbox de Ferny.
-- La configuración se guarda en formato simple clave=valor.

-- Configuración persistente

local config = {
    strip_trailing_blank_lines = false,
    mark_word_on_double_click = false,
    mark_word_single_click_deselect = false,
    copy_file_path_enabled = true,
}

-- ============================================================
-- Parser JSON simple (sin require)
-- Solo soporta: strings, numbers, booleans, null, arrays, objects
-- No soporta: funciones, undefined, comillas simples
-- ============================================================

local JSON_ESCAPES = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\t"] = "\\t" }

-- Every control byte (and quote, backslash) is escaped, so the output is valid JSON.
local function json_escape(s)
    return (s:gsub('[%c"\\]', function(c)
        return JSON_ESCAPES[c] or string.format("\\u%04x", c:byte())
    end))
end

local function parse_json(text)
    -- Parseador recursivo muy básico
    text = text:gsub("%s+", "")

    local pos = 1

    local function peek()
        return text:sub(pos, pos)
    end

    local function consume()
        pos = pos + 1
    end

    local function skip_ws()
        while pos <= #text and text:sub(pos, pos):match("%s") do
            pos = pos + 1
        end
    end

    local function parse_value()
        skip_ws()
        local c = peek()

        if c == '"' then
            -- String
            consume()
            local result = {}
            while pos <= #text do
                local ch = text:sub(pos, pos)
                if ch == '\\' then
                    consume()
                    local next_ch = text:sub(pos, pos)
                    if next_ch == '"' then table.insert(result, '"')
                    elseif next_ch == '\\' then table.insert(result, '\\')
                    elseif next_ch == 'n' then table.insert(result, '\n')
                    elseif next_ch == 't' then table.insert(result, '\t')
                    else table.insert(result, next_ch)
                    end
                    consume()
                elseif ch == '"' then
                    consume()
                    return table.concat(result)
                else
                    table.insert(result, ch)
                    consume()
                end
            end
        elseif c == '{' then
            -- Object
            consume()
            local result = {}
            skip_ws()
            if peek() == '}' then
                consume()
                return result
            end
            while true do
                skip_ws()
                local key = parse_value()
                skip_ws()
                consume() -- colon
                skip_ws()
                local value = parse_value()
                result[key] = value
                skip_ws()
                if peek() == ',' then
                    consume()
                elseif peek() == '}' then
                    consume()
                    return result
                else
                    break
                end
            end
        elseif c == '[' then
            -- Array
            consume()
            local result = {}
            skip_ws()
            if peek() == ']' then
                consume()
                return result
            end
            while true do
                skip_ws()
                table.insert(result, parse_value())
                skip_ws()
                if peek() == ',' then
                    consume()
                elseif peek() == ']' then
                    consume()
                    return result
                else
                    break
                end
            end
        elseif c:match("%-?%d") then
            -- Number
            local start = pos
            while pos <= #text and text:sub(pos, pos):match("[%d.eE+-]") do
                pos = pos + 1
            end
            return tonumber(text:sub(start, pos - 1))
        end

        error("Unexpected character at position " .. pos)
    end

    return parse_value()
end

local function json_encode_simple(data)
    if data == nil then return "null" end
    if type(data) == "boolean" then return data and "true" or "false" end
    if type(data) == "number" then return tostring(data) end
    if type(data) == "string" then return '"' .. json_escape(data) .. '"' end
    if type(data) == "table" then
        -- Check if array
        local is_array = true
        local keys = {}
        for k in pairs(data) do
            if type(k) ~= "number" then is_array = false end
            table.insert(keys, k)
        end
        if is_array and #keys == 0 then return "[]" end
        if is_array then
            local parts = {}
            for _, v in ipairs(data) do
                table.insert(parts, json_encode_simple(v))
            end
            return "[" .. table.concat(parts, ",") .. "]"
        end
        -- Object
        local parts = {}
        for k, v in pairs(data) do
            table.insert(parts, json_encode_simple(tostring(k)) .. ":" .. json_encode_simple(v))
        end
        return "{" .. table.concat(parts, ",") .. "}"
    end
    return "null"
end

-- Cargar configuración
-- Only the known boolean options are read; anything else in the file is ignored.
local CONFIG_BOOLEANS = {
    "strip_trailing_blank_lines",
    "mark_word_on_double_click",
    "mark_word_single_click_deselect",
    "copy_file_path_enabled",
}

local function load_config()
    local content = ferny.config.read()
    if content then
        local data = parse_json(content)
        if type(data) == "table" then
            for _, key in ipairs(CONFIG_BOOLEANS) do
                if type(data[key]) == "boolean" then
                    config[key] = data[key]
                end
            end
        end
    end
end

-- Guardar configuración
local function save_config()
    local content = json_encode_simple(config)
    ferny.config.write(content)
end

-- ============================================================
-- 1. Strip trailing blank lines (líneas en blanco al final)
-- ============================================================

local function strip_trailing_blank_lines(text)
    if not config.strip_trailing_blank_lines then
        return text
    end

    local lines = {}
    for line in text:gmatch("([^\n]*)\n?") do
        table.insert(lines, line)
    end

    local last_non_empty = 0
    for i = #lines, 1, -1 do
        if lines[i]:match("%S") then
            last_non_empty = i
            break
        end
    end

    local result = {}
    for i = 1, math.min(last_non_empty + 1, #lines) do
        table.insert(result, lines[i])
    end

    return table.concat(result, "\n")
end

ferny.on("document.before_save", function(doc)
    if config.strip_trailing_blank_lines then
        local new_text = strip_trailing_blank_lines(doc.text)
        if new_text ~= doc.text then
            return new_text
        end
    end
    return nil
end)

-- ============================================================
-- 2. Mark word on double-click (marcar palabra al doble-clic)
-- ============================================================

local last_marked_word = nil
local marked_positions = {}

local function clear_markers()
    last_marked_word = nil
    marked_positions = {}
end

local function mark_word_occurrences(word)
    ferny.log("Mark word occurrences: " .. word)
    last_marked_word = word
end

ferny.on("document.opened", function(doc)
    -- Placeholder para mark word
end)

ferny.on("document.activated", function(doc)
    -- Placeholder para mark word
end)

-- ============================================================
-- 3. Copy file path (copiar ruta del archivo)
-- ============================================================

ferny.command {
    id = "copy_file_path",
    title = ferny.tr("Copy File Path"),
    run = function()
        local doc = ferny.documents.current()
        if not doc or not doc.path then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        ferny.clipboard.set(doc.path)
        ferny.ui.status(ferny.tr("File path copied to clipboard: ") .. doc.path)
    end,
}

-- ============================================================
-- 4. Tasks (lista de tareas TODO/FIXME)
-- ============================================================

local DEFAULT_TOKENS = {"TODO", "FIXME"}

local function scan_tasks(text, tokens)
    local tasks = {}
    local line_num = 1

    for line in text:gmatch("([^\n]*)\n?") do
        for _, token in ipairs(tokens) do
            local pattern = token .. "%s*:?%s*(.*)"
            local match = line:match(pattern)
            if match then
                table.insert(tasks, {
                    line = line_num,
                    token = token,
                    text = match,
                    full_line = line:match("^%s*(.*)")
                })
            end
        end
        line_num = line_num + 1
    end

    return tasks
end

ferny.command {
    id = "show_tasks",
    title = ferny.tr("Show Tasks (TODO/FIXME)"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local tasks = scan_tasks(text, DEFAULT_TOKENS)

        if #tasks == 0 then
            ferny.ui.status(ferny.tr("No tasks found"))
            return
        end

        local lines = { ferny.tr("Tasks found:") }
        for _, task in ipairs(tasks) do
            table.insert(lines, string.format("  Line %d [%s]: %s", task.line, task.token, task.text))
        end

        ferny.ui.message(table.concat(lines, "\n"))
    end,
}

-- ============================================================
-- 5. XML tagging (etiquetado XML) - placeholder
-- ============================================================

ferny.command {
    id = "xml_tagging",
    title = ferny.tr("XML Tagging for Selection"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end

        ferny.editor.replace_selection("<tag>" .. sel .. "</tag>")
        ferny.ui.status(ferny.tr("Wrapped selection in <tag>"))
    end,
}

-- ============================================================
-- 6. Enclose words (encerrar palabras)
-- ============================================================

ferny.command {
    id = "enclose_brackets",
    title = ferny.tr("Enclose Selection with Brackets"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection("(" .. sel .. ")")
    end,
}

ferny.command {
    id = "enclose_square",
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
    id = "enclose_curly",
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
    id = "enclose_double_quotes",
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
    id = "enclose_single_quotes",
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

-- ============================================================
-- Configuración (preferencias)
-- ============================================================

ferny.command {
    id = "toggle_strip_blank_lines",
    title = ferny.tr("Toggle: Strip Trailing Blank Lines on Save"),
    run = function()
        config.strip_trailing_blank_lines = not config.strip_trailing_blank_lines
        save_config()
        local status = config.strip_trailing_blank_lines and ferny.tr("enabled") or ferny.tr("disabled")
        ferny.ui.status(ferny.tr("Strip trailing blank lines: ") .. status)
    end,
}

ferny.command {
    id = "toggle_mark_word",
    title = ferny.tr("Toggle: Mark Word on Double-Click"),
    run = function()
        config.mark_word_on_double_click = not config.mark_word_on_double_click
        save_config()
        local status = config.mark_word_on_double_click and ferny.tr("enabled") or ferny.tr("disabled")
        ferny.ui.status(ferny.tr("Mark word on double-click: ") .. status)
    end,
}

ferny.command {
    id = "toggle_mark_word_deselect",
    title = ferny.tr("Toggle: Single-Click Deselect Marked Word"),
    run = function()
        config.mark_word_single_click_deselect = not config.mark_word_single_click_deselect
        save_config()
        local status = config.mark_word_single_click_deselect and ferny.tr("enabled") or ferny.tr("disabled")
        ferny.ui.status(ferny.tr("Single-click deselect: ") .. status)
    end,
}

-- Cargar configuración al inicio
load_config()

ferny.log("Addons plugin loaded (JSON parser: native Lua)")
