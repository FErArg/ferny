-- XML Snippets (ferny-xmlsnippets): snippets XML/HTML.
-- Basado en el plugin "xmlsnippets" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Snippets predefinidos
local SNIPPETS = {
    html = {
        ["!"] = "<!DOCTYPE html>\n<html>\n<head>\n\t<meta charset=\"UTF-8\">\n\t<title>${1:Title}</title>\n</head>\n<body>\n\t${0}\n</body>\n</html>",
        div = "<div${1: class=\"${2}\"}>${0}</div>",
        span = "<span${1: class=\"${2}\"}>${0}</span>",
        p = "<p>${0}</p>",
        a = "<a href=\"${1:url}\">${0}</a>",
        img = "<img src=\"${1:src}\" alt=\"${2:alt}\" />",
        ul = "<ul>\n\t<li>${1}</li>\n\t<li>${2}</li>\n</ul>",
        ol = "<ol>\n\t<li>${1}</li>\n\t<li>${2}</li>\n</ol>",
        table = "<table>\n\t<tr>\n\t\t<th>${1:Header}</th>\n\t\t<th>${2:Header}</th>\n\t</tr>\n\t<tr>\n\t\t<td>${3:Cell}</td>\n\t\t<td>${4:Cell}</td>\n\t</tr>\n</table>",
        script = "<script>\n\t${0}\n</script>",
        style = "<style>\n\t${0}\n</style>",
    },

    xml = {
        ["?xml"] = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        elem = "<${1:element}>${0}</${1}>",
        ["empty"] = "<${1:element} />",
        attr = "${1:attr}=\"${2:value}\"",
        comment = "<!-- ${0} -->",
        cdata = "<![CDATA[${0}]]>",
    },
}

-- Obtener snippets para filetype
local function get_snippets(filetype)
    return SNIPPETS[filetype] or SNIPPETS.html
end

ferny.command {
    id = "insert-snippet",
    title = ferny.tr("Insert XML/HTML Snippet"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local filetype = doc.filetype or "html"
        local snippets = get_snippets(filetype)

        -- Listar snippets disponibles
        local names = {}
        for name in pairs(snippets) do
            table.insert(names, name)
        end
        table.sort(names)

        local msg = ferny.tr("Available snippets: ") .. table.concat(names, ", ")
        ferny.ui.message(msg)

        -- Insertar el primero como ejemplo
        local first_name = names[1]
        if first_name and snippets[first_name] then
            ferny.editor.insert(snippets[first_name])
        end
    end,
}

ferny.command {
    id = "snippet-html5",
    title = ferny.tr("Insert HTML5 Boilerplate"),
    run = function()
        local template = [[<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${1:Document Title}</title>
</head>
<body>
    ${0}
</body>
</html>]]
        ferny.editor.insert(template)
        ferny.ui.status(ferny.tr("HTML5 template inserted"))
    end,
}

ferny.command {
    id = "snippet-xml-decl",
    title = ferny.tr("Insert XML Declaration"),
    run = function()
        ferny.editor.insert("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
        ferny.ui.status(ferny.tr("XML declaration inserted"))
    end,
}

ferny.log("XML Snippets plugin loaded")