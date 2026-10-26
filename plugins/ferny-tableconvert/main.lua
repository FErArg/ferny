-- Table Convert (ferny-tableconvert): converts lists into tables for different formats.
-- Modelled on Geany's tableconvert plugin; written for Ferny.

local formats = {
    latex = {
        name = "LaTeX",
        start = "\\begin{table}[h]\n\\begin{tabular}{}\n",
        header_start = "\t",
        header_stop = "",
        header_columnsplit = " & ",
        header_linestart = "",
        header_lineend = "\\\\ \\hline",
        body_start = "",
        body_end = "",
        columnsplit = " & ",
        linestart = "\t",
        lineend = "\\\\",
        linesplit = "",
        end_str = "\\end{tabular}\n\\end{table}",
        has_header = true,
    },
    html = {
        name = "HTML",
        start = "<table>\n",
        header_start = "<thead>\n",
        header_stop = "</thead>\n",
        header_columnsplit = "</th>\n\t<th>",
        header_linestart = "<tr>\n\t<th>",
        header_lineend = "</th>\n</tr>\n",
        body_start = "<tbody>\n",
        body_end = "\n</tbody>",
        columnsplit = "</td>\n\t<td>",
        linestart = "<tr>\n\t<td>",
        lineend = "</td>\n</tr>",
        linesplit = "",
        end_str = "\n</table>",
        has_header = true,
    },
    sql = {
        name = "SQL",
        start = "",
        header_start = "",
        header_stop = "",
        header_columnsplit = "",
        header_linestart = "",
        header_lineend = "",
        body_start = "",
        body_end = "",
        columnsplit = "','",
        linestart = "\t('",
        lineend = "')",
        linesplit = ",",
        end_str = ";",
        has_header = false,
    },
    dokuwiki = {
        name = "DokuWiki",
        start = "",
        header_start = "",
        header_stop = "",
        header_columnsplit = " ^ ",
        header_linestart = "^ ",
        header_lineend = " ^\n",
        body_start = "",
        body_end = "",
        columnsplit = " | ",
        linestart = "| ",
        lineend = " |\n",
        linesplit = "",
        end_str = "",
        has_header = true,
    },
    markdown = {
        name = "Markdown",
        start = "",
        header_start = "",
        header_stop = "",
        header_columnsplit = " | ",
        header_linestart = "| ",
        header_lineend = " |\n|---",
        body_start = "",
        body_end = "",
        columnsplit = " | ",
        linestart = "| ",
        lineend = " |\n",
        linesplit = "",
        end_str = "",
        has_header = true,
    },
    csv = {
        name = "CSV",
        start = "",
        header_start = "",
        header_stop = "",
        header_columnsplit = ",",
        header_linestart = "",
        header_lineend = "\n",
        body_start = "",
        body_end = "",
        columnsplit = ",",
        linestart = "",
        lineend = "\n",
        linesplit = "",
        end_str = "",
        has_header = true,
    },
    mediawiki = {
        name = "MediaWiki",
        start = '{| class="wikitable"\n',
        header_start = "",
        header_stop = "",
        header_columnsplit = " !! ",
        header_linestart = "! ",
        header_lineend = "\n",
        body_start = "",
        body_end = "",
        columnsplit = " || ",
        linestart = "| ",
        lineend = "\n",
        linesplit = "",
        end_str = "|}\n",
        has_header = true,
    },
    rst = {
        name = "reStructuredText",
        start = "",
        header_start = "",
        header_stop = "",
        header_columnsplit = "   ",
        header_linestart = "",
        header_lineend = "\n",
        body_start = "",
        body_end = "",
        columnsplit = "   ",
        linestart = "",
        lineend = "\n",
        linesplit = "",
        end_str = "",
        has_header = true,
    },
}

local function parse_input(text, delimiter)
    local lines = {}
    local has_header = true
    
    -- Split into lines
    for line in text:gmatch("([^\n]*)\n?") do
        if line ~= "" then
            table.insert(lines, line)
        end
    end
    
    if #lines == 0 then
        return nil, "No input text"
    end
    
    -- Parse each line into columns
    local rows = {}
    local max_cols = 0
    
    for i, line in ipairs(lines) do
        local cols = {}
        local pattern = "([^" .. delimiter .. "]*)"
        local start = 1
        
        while start <= #line do
            local col_end = line:find(delimiter, start, true)
            if col_end then
                local col = line:sub(start, col_end - 1)
                table.insert(cols, col:match("^%s*(.-)%s*$")) -- trim
                start = col_end + 1
            else
                local col = line:sub(start)
                table.insert(cols, col:match("^%s*(.-)%s*$")) -- trim
                break
            end
        end
        
        if #cols > max_cols then
            max_cols = #cols
        end
        table.insert(rows, cols)
    end
    
    return rows, max_cols
end

local function build_table(rows, max_cols, format, has_header)
    local fmt = formats[format]
    if not fmt then
        return nil, "Unknown format: " .. format
    end
    
    local output = {}
    
    -- Start
    if fmt.start ~= "" then
        table.insert(output, fmt.start)
    end
    
    -- Header
    if has_header and fmt.has_header and #rows > 0 then
        if fmt.header_start ~= "" then
            table.insert(output, fmt.header_start)
        end
        
        local header = rows[1]
        local header_parts = {}
        for i = 1, max_cols do
            local val = header[i] or ""
            table.insert(header_parts, val)
        end
        
        local header_line = fmt.header_linestart .. table.concat(header_parts, fmt.header_columnsplit) .. fmt.header_lineend
        table.insert(output, header_line)
        
        if fmt.header_stop ~= "" then
            table.insert(output, fmt.header_stop)
        end
    end
    
    -- Body
    if fmt.body_start ~= "" then
        table.insert(output, fmt.body_start)
    end
    
    local start_row = (has_header and fmt.has_header) and 2 or 1
    for i = start_row, #rows do
        local row = rows[i]
        local row_parts = {}
        for j = 1, max_cols do
            local val = row[j] or ""
            table.insert(row_parts, val)
        end
        
        local row_line = fmt.linestart .. table.concat(row_parts, fmt.columnsplit) .. fmt.lineend
        table.insert(output, row_line)
    end
    
    if fmt.body_end ~= "" then
        table.insert(output, fmt.body_end)
    end
    
    -- End
    if fmt.end_str ~= "" then
        table.insert(output, fmt.end_str)
    end
    
    return table.concat(output, "")
end

local function convert_selection(format_name, delimiter)
    local sel = ferny.editor.selection()
    if sel == "" then
        ferny.ui.status(ferny.tr("Select text to convert first"))
        return
    end
    
    local rows, max_cols = parse_input(sel, delimiter)
    if not rows then
        ferny.ui.status(ferny.tr(max_cols)) -- max_cols is error message here
        return
    end
    
    local result, err = build_table(rows, max_cols, format_name, true)
    if not result then
        ferny.ui.status(ferny.tr(err))
        return
    end
    
    ferny.editor.replace_selection(result)
    ferny.ui.status(ferny.tr("Converted to ") .. formats[format_name].name)
end

-- Commands for each format
local delimiter = "\t" -- default to tab

ferny.command {
    id = "tableconvert-set-delimiter",
    title = ferny.tr("Set Column Delimiter"),
    run = function()
        ferny.ui.status(ferny.tr("Current delimiter: ') .. delimiter .. ferny.tr(''. Use keybinding to change."))
    end,
}

ferny.command {
    id = "tableconvert-latex",
    title = ferny.tr("Convert to LaTeX Table"),
    run = function()
        convert_selection("latex", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-html",
    title = ferny.tr("Convert to HTML Table"),
    run = function()
        convert_selection("html", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-sql",
    title = ferny.tr("Convert to SQL INSERT"),
    run = function()
        convert_selection("sql", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-dokuwiki",
    title = ferny.tr("Convert to DokuWiki Table"),
    run = function()
        convert_selection("dokuwiki", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-markdown",
    title = ferny.tr("Convert to Markdown Table"),
    run = function()
        convert_selection("markdown", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-csv",
    title = ferny.tr("Convert to CSV"),
    run = function()
        convert_selection("csv", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-mediawiki",
    title = ferny.tr("Convert to MediaWiki Table"),
    run = function()
        convert_selection("mediawiki", delimiter)
    end,
}

ferny.command {
    id = "tableconvert-rst",
    title = ferny.tr("Convert to reStructuredText Table"),
    run = function()
        convert_selection("rst", delimiter)
    end,
}

-- Helper to detect delimiter from selection
local function detect_delimiter(text)
    local first_line = text:match("([^\n]*)")
    if not first_line then return "\t" end
    
    local delimiters = { "\t", ",", ";", "|", " " }
    local max_count = 0
    local best_delim = "\t"
    
    for _, delim in ipairs(delimiters) do
        local count = 0
        for _ in first_line:gmatch(delim) do
            count = count + 1
        end
        if count > max_count then
            max_count = count
            best_delim = delim
        end
    end
    
    return best_delim
end

ferny.command {
    id = "tableconvert-auto",
    title = ferny.tr("Auto-detect and Convert to Markdown"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("Select text to convert first"))
            return
        end
        
        local delim = detect_delimiter(sel)
        convert_selection("markdown", delim)
    end,
}