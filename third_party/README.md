# third_party

Vendored dependencies at pinned versions. Versions, licences, origins and
archive hashes are recorded in [docs/DEPENDENCIES.md](../docs/DEPENDENCIES.md).

| Directory | Version | Manifest |
|---|---|---|
| `scintilla/` | 5.6.7 (includes the Qt layer `qt/ScintillaEdit`) | `scintilla.sha256` |
| `lexilla/` | 5.5.4 | `lexilla.sha256` |
| `corrosion/` | 0.6.1 (`cmake/` and top-level files; tests and docs omitted) | `corrosion.sha256` |
| `cxx-qt-cmake/` | 0.10.0 (`.github/` omitted) | `cxx-qt-cmake.sha256` |
| `geany/` | 2.1.0 subset: `COPYING`, `AUTHORS`, `data/filedefs/`, `data/filetype_extensions.conf`, `data/colorschemes/`, `data/templates/`, `data/snippets.conf` (H2.5b), `src/highlightingmappings.h`, `src/filetypes.c` (read by `tools/geany-import`, not compiled) | `geany.sha256` |

`third_party/CMakeLists.txt` is Ferny's own build of Scintilla (core + Qt
layer `ScintillaEditBase`) and Lexilla as static libraries, with their
warnings silenced; it is not vendored code and has no manifest.

Rules:

- Never edit vendored files. Ferny-specific code goes in `ui/` or
  `crates/`. If a patch is unavoidable, keep it as a separate `.patch` file
  with a reason and record it in `docs/DEPENDENCIES.md`.
- Files are stored byte for byte (`.gitattributes`: `-text -whitespace`),
  so the manifests match on every platform.
- The validation gate (`scripts/check.*`) verifies every file against its
  manifest.

## Updating a component

Corrosion and cxx-qt-cmake come from git tags: clone the tag shallowly into
an empty temporary directory, confirm the tag commit with
`git ls-remote`, copy the same subset of files and regenerate the
manifest. cxx-qt-cmake must match the version of the cxx-qt crates. For
Scintilla and Lexilla:

1. Download the new release archive from scintilla.org into an empty
   temporary directory (downloads are untrusted).
2. Cross-check it: Scintilla against the SourceForge release file (same
   SHA-256), Lexilla against the GitHub tag `ScintillaOrg/lexilla`
   `rel-X-Y-Z` (same tree except VCS files).
3. Read the release notes for security-relevant fixes.
4. Replace the directory, regenerate the manifest from `third_party/`:
   `find scintilla -type f | LC_ALL=C sort | xargs shasum -a 256 > scintilla.sha256`
5. Update `docs/DEPENDENCIES.md` (version, archive SHA-256, cross-check); run
   the validation gate.
