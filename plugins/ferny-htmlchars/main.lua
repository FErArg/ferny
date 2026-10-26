-- HTML Characters (bundled with Ferny, H3.4): encodes the non-ASCII
-- characters of the selection as HTML entities and decodes entities back.
-- Behaviour modelled on Geany's htmlchars plugin; written for Ferny.

-- Latin-1 names, U+00A0 to U+00FF in order (HTML 4 / HTML5).
local latin1 = {
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect",
    "uml", "copy", "ordf", "laquo", "not", "shy", "reg", "macr",
    "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot",
    "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil",
    "Egrave", "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml",
    "ETH", "Ntilde", "Ograve", "Oacute", "Ocirc", "Otilde", "Ouml", "times",
    "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute", "THORN", "szlig",
    "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml",
    "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml", "divide",
    "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
}

local names = {
    [8211] = "ndash", [8212] = "mdash", [8216] = "lsquo", [8217] = "rsquo",
    [8220] = "ldquo", [8221] = "rdquo", [8226] = "bull", [8230] = "hellip",
    [8364] = "euro", [8482] = "trade",
}
for i, name in ipairs(latin1) do
    names[159 + i] = name
end

local codes = { amp = 38, lt = 60, gt = 62, quot = 34, apos = 39 }
for code, name in pairs(names) do
    codes[name] = code
end

local function encode(text)
    local out = {}
    for _, code in utf8.codes(text) do
        if code < 128 then
            out[#out + 1] = utf8.char(code)
        elseif names[code] then
            out[#out + 1] = "&" .. names[code] .. ";"
        else
            out[#out + 1] = "&#" .. code .. ";"
        end
    end
    return table.concat(out)
end

local function decode(text)
    return (text:gsub("&(#?[xX]?%w+);", function(entity)
        local code
        if entity:sub(1, 2):lower() == "#x" then
            code = tonumber(entity:sub(3), 16)
        elseif entity:sub(1, 1) == "#" then
            code = tonumber(entity:sub(2))
        else
            code = codes[entity]
        end
        if code and code > 0 and code <= 0x10FFFF and not (code >= 0xD800 and code <= 0xDFFF) then
            return utf8.char(code)
        end
    end))
end

local function on_selection(transform)
    return function()
        local text = ferny.editor.selection()
        if text == "" then
            ferny.ui.status(ferny.tr("Select some text first."))
            return
        end
        ferny.editor.replace_selection(transform(text))
    end
end

ferny.command { id = "encode", title = ferny.tr("Encode HTML Entities"), run = on_selection(encode) }
ferny.command { id = "decode", title = ferny.tr("Decode HTML Entities"), run = on_selection(decode) }
