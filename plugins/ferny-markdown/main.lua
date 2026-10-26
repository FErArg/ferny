-- Markdown (ferny-markdown): soporte para Markdown.
-- Basado en el plugin "markdown" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Nota: Ferny ya tiene resaltado de sintaxis. Este plugin añade funcionalidades extra.

-- Convertir Markdown a HTML (básico)
local function markdown_to_html(text)
    local result = text

    -- Headers
    result = result:gsub("^### (.+)$", "<h3>%1</h3>")
    result = result:gsub("^## (.+)$", "<h2>%1</h2>")
    result = result:gsub("^# (.+)$", "<h1>%1</h1>")

    -- Bold e italic
    result = result:gsub("%*%*(.+)%*%*", "<strong>%1</strong>")
    result = result:gsub("%*(.+)%*", "<em>%1</em>")
    result = result:gsub("__(.+)__", "<strong>%1</strong>")
    result = result:gsub("_(.+)_", "<em>%1</em>")

    -- Code inline
    result = result:gsub("`([^`]+)`", "<code>%1</code>")

    -- Links
    result = result:gsub("%[([^%]]+)%]%(([^%)]+)%)", "<a href=\"%2\">%1</a>")

    -- Images
    result = result:gsub("!%[([^%]]*)%]%(([^%)]+)%)", "<img src=\"%2\" alt=\"%1\" />")

    -- Code blocks
    result = result:gsub("```([^\n]*)\n(.-)\n```", "<pre><code class=\"%1\">%2</code></pre>")

    -- Paragraphs (simplificado)
    -- En un caso real se haría más complejo

    return result
end

-- Convertir HTML a Markdown (básico)
local function html_to_markdown(text)
    local result = text

    result = result:gsub("<h1>(.-)</h1>", "# %1")
    result = result:gsub("<h2>(.-)</h2>", "## %1")
    result = result:gsub("<h3>(.-)</h3>", "### %1")
    result = result:gsub("<strong>(.-)</strong>", "**%1**")
    result = result:gsub("<em>(.-)</em>", "*%1*")
    result = result:gsub("<code>(.-)</code>", "`%1`")
    result = result:gsub("<a href=\"(.-)\">(.-)</a>", "[%2](%1)")
    result = result:gsub("<img src=\"(.-)\" alt=\"(.-)\" />", "![%2](%1)")
    result = result:gsub("<pre><code(.-)>(.-)</code></pre>", "```%1\n%2\n```")
    result = result:gsub("<br%s*/?>", "\n")
    result = result:gsub("<p>(.-)</p>", "%1")

    return result
end

ferny.command {
    id = "md-to-html",
    title = ferny.tr("Convert Markdown to HTML"),
    run = function()
        local text = ferny.editor.text()
        local html = markdown_to_html(text)
        ferny.editor.replace_selection(html)
        ferny.ui.status(ferny.tr("Converted to HTML"))
    end,
}

ferny.command {
    id = "html-to-md",
    title = ferny.tr("Convert HTML to Markdown"),
    run = function()
        local text = ferny.editor.text()
        local md = html_to_markdown(text)
        ferny.editor.replace_selection(md)
        ferny.ui.status(ferny.tr("Converted to Markdown"))
    end,
}

ferny.command {
    id = "md-preview",
    title = ferny.tr("Preview Markdown as HTML"),
    run = function()
        local text = ferny.editor.text()
        local html = markdown_to_html(text)
        ferny.ui.message(html)
    end,
}

ferny.command {
    id = "md-toc",
    title = ferny.tr("Generate Table of Contents"),
    run = function()
        local text = ferny.editor.text()
        local headers = {}

        for line in text:gmatch("([^\n]*)\n?") do
            local level, title = line:match("^(#+)%s+(.+)$")
            if level then
                table.insert(headers, {level = #level, title = title})
            end
        end

        if #headers == 0 then
            ferny.ui.status(ferny.tr("No headers found"))
            return
        end

        local toc = {}
        for _, h in ipairs(headers) do
            local indent = string.rep("  ", h.level - 1)
            table.insert(toc, string.format("%s- [%s](#%s)", indent, h.title, h.title:lower():gsub("%s+", "-"):gsub("[^%w%-]", "")))
        end

        ferny.editor.insert(table.concat(toc, "\n") .. "\n\n")
        ferny.ui.status(ferny.tr("TOC generated"))
    end,
}

ferny.log("Markdown plugin loaded")