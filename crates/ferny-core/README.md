# crates/ferny-core

Documents, rope mirror of Scintilla buffers, sessions, encodings, file watching, command registry. No Qt dependency.

Implemented: `command` (H0.5); `document`, `text`, `fsio` (H1.1) —
encoding detection (BOM, UTF-8, chardetng), BOM and EOL preservation,
bounded reads (256 MiB, "large" above 32 MiB), atomic saves and
external-change detection.
