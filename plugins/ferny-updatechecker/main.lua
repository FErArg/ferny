-- Update Checker (ferny-updatechecker): verificar actualizaciones.
-- Basado en el plugin "updatechecker" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

ferny.command {
    id = "check-updates",
    title = ferny.tr("Check for Updates"),
    run = function()
        -- Placeholder: en Ferny las actualizaciones se gestionan por el gestor de paquetes
        ferny.ui.status(ferny.tr("Updates managed by package manager"))
    end,
}

ferny.command {
    id = "check-plugin-updates",
    title = ferny.tr("Check Plugin Updates"),
    run = function()
        ferny.ui.status(ferny.tr("Plugin updates - check repository"))
    end,
}

ferny.log("Update Checker plugin loaded")