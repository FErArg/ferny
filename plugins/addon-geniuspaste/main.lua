-- Genius Paste (addon_geniuspaste): pegar en pastebin.
-- Basado en el plugin "geniuspaste" de Geany_Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

ferny.command {
    id = "paste_to_clipboard",
    title = ferny.tr("Copy to Clipboard"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end

        ferny.clipboard.set(sel)
        ferny.ui.status(ferny.tr("Copied to clipboard"))
    end,
}

ferny.log("Genius Paste plugin loaded (clipboard_only)")
