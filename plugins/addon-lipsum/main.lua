-- Lipsum (addon_lipsum): generador de texto de relleno (Lorem Ipsum).
-- Basado en el plugin "Lipsum" de Geany_Plugins (versión 2.1), escrito en C por Frank Lanitz.
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Texto Lorem Ipsum por defecto
local DEFAULT_LOREMIPSUM = [[
Lorem ipsum dolor sit amet, consectetur adipisicing elit, sed do
eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim
ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut
aliquip ex ea commodo consequat. Duis aute irure dolor in
reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla
pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
culpa qui officia deserunt mollit anim id est laborum.
]]

local config = {
    lipsum_text = DEFAULT_LOREMIPSUM,
}

local function parse_config(text)
    if #text > 65536 then error("config too large") end
    local value = text:match("^lipsum_text=(.*)$")
    if not value or #value > 32768 then error("invalid config") end
    return { lipsum_text = value }
end

local function encode_config(value)
    local text = value.lipsum_text or DEFAULT_LOREMIPSUM
    if #text > 32768 or text:find("[\r\n]") then error("invalid config") end
    return "lipsum_text=" .. text
end

-- Cargar configuración
local function load_config()
    local content = ferny.config.read()
    if content then
        local ok, data = pcall(parse_config, content)
        if ok and data then
            for k, v in pairs(data) do
                config[k] = v
            end
        end
    end
end

-- Guardar configuración
local function save_config()
    local ok, content = pcall(encode_config, config)
    if ok then
        ferny.config.write(content)
    end
end

-- Generar texto lipsum de la longitud solicitada
local function generate_lipsum(length)
    local text = config.lipsum_text
    local text_len = #text

    if text_len >= length then
        return text:sub(1, length)
    end

    local result = {}
    local full_copies = math.floor(length / text_len)
    local remaining = length % text_len

    for i = 1, full_copies do
        table.insert(result, text)
    end

    if remaining > 0 then
        table.insert(result, text:sub(1, remaining))
    end

    return table.concat(result)
end

-- Comando principal: insertar lipsum
ferny.command {
    id = "insert_lipsum",
    title = ferny.tr("Insert Lipsum Text"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        -- Longitud por defecto: 1500 caracteres
        local length = 1500

        -- En Ferny no hay dialogs_show_input_numeric, usamos status para pedir input
        -- El usuario puede cambiar la longitud editando la configuración
        local lipsum_text = generate_lipsum(length)

        ferny.editor.insert(lipsum_text)
        ferny.ui.status(ferny.tr("Inserted ") .. length .. ferny.tr(" characters of Lipsum text"))
    end,
}

-- Comando: insertar lipsum con longitud personalizada (via config)
ferny.command {
    id = "insert_lipsum_custom",
    title = ferny.tr("Insert Lipsum Text (Custom Length)"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        -- Usar longitud desde config o default
        local length = config.custom_length or 1500
        local lipsum_text = generate_lipsum(length)

        ferny.editor.insert(lipsum_text)
        ferny.ui.status(ferny.tr("Inserted ") .. length .. ferny.tr(" characters of Lipsum text"))
    end,
}

-- Comando: configurar texto lipsum personalizado
ferny.command {
    id = "set_lipsum_text",
    title = ferny.tr("Set Custom Lipsum Text"),
    run = function()
        -- Placeholder: en Ferny no hay diálogo de input de texto multilínea
        -- El usuario puede editar el archivo de configuración directamente
        ferny.ui.status(ferny.tr("Edit ") .. CONFIG_FILE .. ferny.tr(" to customize lipsum text"))
    end,
}

-- Comando: configurar longitud personalizada
ferny.command {
    id = "set_lipsum_length",
    title = ferny.tr("Set Custom Lipsum Length"),
    run = function()
        -- Placeholder: el usuario edita el config
        ferny.ui.status(ferny.tr("Edit ") .. CONFIG_FILE .. ferny.tr(" to set custom length (custom_length)"))
    end,
}

-- Comando: restablecer a valores por defecto
ferny.command {
    id = "reset_lipsum",
    title = ferny.tr("Reset Lipsum to Default"),
    run = function()
        config.lipsum_text = DEFAULT_LOREMIPSUM
        config.custom_length = nil
        save_config()
        ferny.ui.status(ferny.tr("Lipsum reset to default"))
    end,
}

-- Comando: mostrar texto lipsum actual
ferny.command {
    id = "show_lipsum_text",
    title = ferny.tr("Show Current Lipsum Text"),
    run = function()
        local text = config.lipsum_text
        local preview = text
        if #preview > 200 then
            preview = preview:sub(1, 200) .. "..."
        end
        ferny.ui.message(ferny.tr("Current Lipsum text:") .. "\n\n" .. preview .. "\n\n" .. ferny.tr("Length: ") .. #text .. ferny.tr(" characters"))
    end,
}

-- Cargar configuración al inicio
load_config()

ferny.log("Lipsum plugin loaded")
