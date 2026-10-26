-- Spell Check (ferny-spellcheck): corrección ortográfica.
-- Basado en el plugin "spellcheck" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Nota: Sin diccionario externo, usa lista de palabras comunes.
-- Para un spellcheck real se necesita integración con hunspell/aspell.

local COMMON_WORDS = {
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i",
    "it", "for", "not", "on", "with", "he", "as", "you", "do", "at",
    "this", "but", "his", "by", "from", "they", "we", "say", "her", "she",
    "or", "an", "will", "my", "one", "all", "would", "there", "their", "what",
    "so", "up", "out", "if", "about", "who", "get", "which", "go", "me",
    "el", "la", "de", "que", "y", "a", "en", "un", "ser", "se",
    "no", "te", "lo", "le", "da", "su", "por", "son", "con", "para",
    "al", "del", "los", "las", "una", "uno", "este", "esta", "esos", "esas",
    "mi", "tu", "su", "nos", "vos", "me", "ti", "se", "le", "les",
    "como", "más", "menos", "muy", "poco", "mucho", "bien", "mal",
    "sí", "no", "también", "tampoco", "nunca", "siempre", "ahora", "luego",
    "aquí", "allí", "allá", "acá", "hoy", "ayer", "mañana", "antes", "después",
}

-- Verificar si una palabra es conocida
local function is_known(word)
    word = word:lower()
    for _, w in ipairs(COMMON_WORDS) do
        if w == word then
            return true
        end
    end
    -- Palabras con números o guiones
    if word:match("^%d+$") or word:match("^%w+%-%w+$") then
        return true
    end
    return false
end

-- Extraer palabras del texto
local function extract_words(text)
    local words = {}
    for word in text:gmatch("[%w']+") do
        if #word > 1 then
            table.insert(words, word)
        end
    end
    return words
end

ferny.command {
    id = "spellcheck-document",
    title = ferny.tr("Spell Check Document"),
    run = function()
        local text = ferny.editor.text()
        local words = extract_words(text)
        local unknown = {}
        local seen = {}

        for _, word in ipairs(words) do
            if not seen[word:lower()] and not is_known(word) then
                table.insert(unknown, word)
                seen[word:lower()] = true
            end
        end

        if #unknown == 0 then
            ferny.ui.status(ferny.tr("No unknown words found"))
        else
            local msg = ferny.tr("Unknown words: ") .. table.concat(unknown, ", ")
            ferny.ui.message(msg)
        end
    end,
}

ferny.command {
    id = "spellcheck-selection",
    title = ferny.tr("Spell Check Selection"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end

        local words = extract_words(sel)
        local unknown = {}
        local seen = {}

        for _, word in ipairs(words) do
            if not seen[word:lower()] and not is_known(word) then
                table.insert(unknown, word)
                seen[word:lower()] = true
            end
        end

        if #unknown == 0 then
            ferny.ui.status(ferny.tr("No unknown words in selection"))
        else
            local msg = ferny.tr("Unknown words: ") .. table.concat(unknown, ", ")
            ferny.ui.message(msg)
        end
    end,
}

ferny.command {
    id = "spellcheck-add-word",
    title = ferny.tr("Add Word to Dictionary"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("Select a word to add"))
            return
        end

        local word = sel:match("^%s*(.-)%s*$") -- trim
        table.insert(COMMON_WORDS, word:lower())
        ferny.ui.status(ferny.tr("Added word: ") .. word)
    end,
}

ferny.log("Spell Check plugin loaded (basic wordlist)")