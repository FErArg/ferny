-- Overview (ferny-overview): mini mapa / vista general del documento.
-- Basado en el plugin "overview" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Nota: Ferny no expone API para paneles laterales ni mini mapas.
-- Este plugin proporciona comandos de navegación útil.

-- ============================================================
-- Comandos de navegación
-- ============================================================

ferny.command {
    id = "go-to-line",
    title = ferny.tr("Go to Line..."),
    run = function()
        -- Placeholder: en Ferny no hay diálogo de input numérico
        ferny.ui.status(ferny.tr("Use keybinding to navigate to line"))
    end,
}

ferny.command {
    id = "jump-to-definition",
    title = ferny.tr("Jump to Definition"),
    run = function()
        -- Placeholder para futuras capacidades de navegación
        ferny.ui.status(ferny.tr("Jump to definition - use LSP provider"))
    end,
}

ferny.command {
    id = "toggle-overview",
    title = ferny.tr("Toggle Overview Panel"),
    run = function()
        -- Placeholder: no hay API para paneles laterales
        ferny.ui.status(ferny.tr("Overview panel - requires UI extension"))
    end,
}

-- Evento para mostrar información del documento
ferny.on("document.activated", function(doc)
    if doc and doc.path then
        -- Contar líneas y palabras
        local text = ferny.editor.text()
        local line_count = 0
        local word_count = 0
        for _ in text:gmatch("[^\n]*\n") do
            line_count = line_count + 1
        end
        if text:match("[^\n]") then
            line_count = line_count + 1
        end
        for _ in text:gmatch("%S+") do
            word_count = word_count + 1
        end
        ferny.log("Document: " .. line_count .. " lines, " .. word_count .. " words")
    end
end)

ferny.log("Overview plugin loaded (navigation commands)")