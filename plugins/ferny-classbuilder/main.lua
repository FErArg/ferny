-- Class Builder (bundled with Ferny, H3.4): creates <Name>.h and
-- <Name>.cpp for the C++ class named by the selection, in the root of the
-- open, trusted project; existing files are never replaced.
-- Behaviour modelled on Geany's classbuilder plugin; written for Ferny.

local function header(name)
    local guard = name:upper() .. "_H"
    return table.concat({
        "#ifndef " .. guard,
        "#define " .. guard,
        "",
        "class " .. name .. " {",
        "public:",
        "    " .. name .. "();",
        "    ~" .. name .. "();",
        "};",
        "",
        "#endif  // " .. guard,
        "",
    }, "\n")
end

local function source(name)
    return table.concat({
        "#include \"" .. name .. ".h\"",
        "",
        name .. "::" .. name .. "() {",
        "}",
        "",
        name .. "::~" .. name .. "() {",
        "}",
        "",
    }, "\n")
end

ferny.command {
    id = "cpp_class",
    title = ferny.tr("Create C++ Class from Selection"),
    run = function()
        local name = ferny.editor.selection()
        if not name:match("^[%a_][%w_]*$") or #name > 64 then
            ferny.ui.status(ferny.tr("Select a valid class name first."))
            return
        end
        local files = { [name .. ".h"] = header(name), [name .. ".cpp"] = source(name) }
        for file in pairs(files) do
            local text, err = ferny.fs.read(file)
            if text then
                ferny.ui.message(string.format(ferny.tr("%s already exists; nothing created."), file))
                return
            elseif err ~= "not found" then
                ferny.ui.message(string.format(ferny.tr("Cannot create the class: %s."), err))
                return
            end
        end
        for file, text in pairs(files) do
            local ok, err = ferny.fs.write(file, text)
            if not ok then
                ferny.ui.message(string.format(ferny.tr("Cannot create the class: %s."), err))
                return
            end
        end
        ferny.ui.status(string.format(ferny.tr("Created %s.h and %s.cpp."), name, name))
    end,
}
