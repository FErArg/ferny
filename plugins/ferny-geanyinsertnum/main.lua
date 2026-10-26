-- Insert Numbers (ferny-geanyinsertnum): insertar números secuenciales.
-- Basado en el plugin "geanyinsertnum" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

ferny.command {
    id = "insert-numbers",
    title = ferny.tr("Insert Sequential Numbers"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        -- En Ferny no hay diálogo de input numérico, usar valores por defecto
        local start = 1
        local step = 1
        local count = 10

        local lines = {}
        for i = 0, count - 1 do
            table.insert(lines, tostring(start + i * step))
        end

        ferny.editor.replace_selection(table.concat(lines, "\n"))
        ferny.ui.status(ferny.tr("Inserted ") .. count .. ferny.tr(" numbers"))
    end,
}

ferny.command {
    id = "insert-numbers-custom",
    title = ferny.tr("Insert Numbers (Custom)"),
    run = function()
        -- Placeholder: editar config para personalizar
        ferny.ui.status(ferny.tr("Edit config for custom range"))
    end,
}

ferny.log("Insert Numbers plugin loaded")