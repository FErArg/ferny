-- Hostile test plugin (H3.3a): never returns from an event; it is disabled.
ferny.on("document.opened", function() while true do end end)
