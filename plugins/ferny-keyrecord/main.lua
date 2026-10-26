-- Key Record (ferny-keyrecord): grabar macros de teclas.
-- Basado en el plugin "keyrecord" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Este plugin es similar a ferny-macros pero enfocado en grabación de pulsaciones

local recordings = {}
local current_recording = nil

ferny.command {
    id = "start-key-record",
    title = ferny.tr("Start Key Recording"),
    run = function()
        if current_recording then
            ferny.ui.status(ferny.tr("Already recording"))
            return
        end

        current_recording = { keystrokes = {} }
        ferny.ui.status(ferny.tr("Recording started - press keys"))
    end,
}

ferny.command {
    id = "stop-key-record",
    title = ferny.tr("Stop Key Recording"),
    run = function()
        if not current_recording then
            ferny.ui.status(ferny.tr("Not recording"))
            return
        end

        table.insert(recordings, current_recording)
        current_recording = nil
        ferny.ui.status(ferny.tr("Recording stopped. Total: ") .. #recordings)
    end,
}

ferny.command {
    id = "play-last-recording",
    title = ferny.tr("Play Last Recording"),
    run = function()
        if #recordings == 0 then
            ferny.ui.status(ferny.tr("No recordings"))
            return
        end

        local rec = recordings[#recordings]
        ferny.ui.status(ferny.tr("Playing recording (") .. #rec.keystrokes .. ferny.tr(" keystrokes)"))
    end,
}

ferny.command {
    id = "list-recordings",
    title = ferny.tr("List Recordings"),
    run = function()
        if #recordings == 0 then
            ferny.ui.status(ferny.tr("No recordings"))
            return
        end

        local lines = { ferny.tr("Recordings:") }
        for i, rec in ipairs(recordings) do
            table.insert(lines, string.format("  %d. %d keystrokes", i, #rec.keystrokes))
        end
        ferny.ui.message(table.concat(lines, "\n"))
    end,
}

ferny.log("Key Record plugin loaded")