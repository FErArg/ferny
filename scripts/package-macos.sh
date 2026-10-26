#!/usr/bin/env bash
# Build the macOS DMG of Ferny from a committed ref (H1.6c).
#
#   scripts/package-macos.sh [--ref <tag-or-commit>] [--out <dir>]
#
# Defaults: --ref HEAD, --out bin. The ref is exported with `git archive`
# into a temporary directory, so uncommitted changes never reach the
# artefact. Only the files whose content changed are copied into a cache
# under ~/Library/Caches/ferny-package, which also keeps the build tree, so
# repeated runs rebuild only what changed. The build is offline. Output:
#   <out>/ferny-v<VERSION>-macos-arm64.dmg, .dmg.sha256 and .cdx.json (SBOM)
# Existing artefacts are never overwritten or deleted.
#
# Release signing (H4.3) comes only from the environment of whoever runs
# the script; identities, profiles and keys are never stored or printed:
#   FERNY_SIGN_IDENTITY   codesign identity ("Developer ID Application: …");
#                         without it the bundle is signed ad-hoc
#   FERNY_NOTARY_PROFILE  `xcrun notarytool` keychain profile (needs the
#                         identity): the DMG is notarised and stapled
#   FERNY_MINISIGN_KEY    minisign secret key file: signs the .sha256
#                         (minisign asks for its password)
set -euo pipefail

usage() {
  echo "usage: scripts/package-macos.sh [--ref <tag-or-commit>] [--out <dir>]" >&2
  exit 2
}
die() {
  echo "package-macos: $*" >&2
  exit 1
}

ref="HEAD"
out="bin"
while [ $# -gt 0 ]; do
  case "$1" in
    --ref) [ $# -ge 2 ] || usage; ref="$2"; shift 2 ;;
    --out) [ $# -ge 2 ] || usage; out="$2"; shift 2 ;;
    -h | --help) usage ;;
    *) usage ;;
  esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# --- Inputs -------------------------------------------------------------------
commit="$(git rev-parse --verify --quiet "${ref}^{commit}")" || die "unknown ref: $ref"
short="$(git rev-parse --short "$commit")"
version="$(git show "$commit:Cargo.toml" | sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' | head -n 1)"
[ -n "$version" ] || die "no version in Cargo.toml at $ref"
if git rev-parse --verify --quiet "refs/tags/$ref" >/dev/null; then
  [ "$ref" = "v$version" ] || die "tag $ref does not match version $version"
fi

[ "$(uname -s)" = "Darwin" ] || die "macOS only"
[ "$(uname -m)" = "arm64" ] || die "arm64 only (Homebrew Qt is arm64-only)"

name="ferny-v${version}-macos-arm64"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
dmg="$out/$name.dmg"
for artefact in "$dmg" "$dmg.sha256" "$dmg.sha256.minisig" "$out/$name.cdx.json"; do
  [ ! -e "$artefact" ] || die "$artefact exists; earlier artefacts are never overwritten"
done

# Release signing: checked before anything is built (fail closed).
identity="${FERNY_SIGN_IDENTITY:-}"
notary="${FERNY_NOTARY_PROFILE:-}"
minisign_key="${FERNY_MINISIGN_KEY:-}"
[ -z "$notary" ] || [ -n "$identity" ] || die "FERNY_NOTARY_PROFILE needs FERNY_SIGN_IDENTITY"
if [ -n "$notary" ]; then
  for tool in xcrun spctl; do
    command -v "$tool" >/dev/null 2>&1 || die "missing tool: $tool"
  done
fi
if [ -n "$minisign_key" ]; then
  [ -f "$minisign_key" ] || die "FERNY_MINISIGN_KEY is not a file"
  command -v minisign >/dev/null 2>&1 || die "missing tool: minisign"
fi

for tool in cmake cargo qmake macdeployqt install_name_tool hdiutil codesign shasum sips iconutil otool lipo file rsync; do
  command -v "$tool" >/dev/null 2>&1 || die "missing tool: $tool"
done

# The toolchain CMake and Corrosion call must be the pinned one. A different
# cargo or rustc first on PATH fails late, after most of the build.
pinned="$(git show "$commit:rust-toolchain.toml" 2>/dev/null | sed -n 's/^channel = "\(.*\)"$/\1/p' | head -n 1)"
[ -n "$pinned" ] || die "no pinned Rust toolchain in rust-toolchain.toml at $ref"
found="$(rustc --version 2>/dev/null | cut -d' ' -f2)"
[ "$found" = "$pinned" ] || die "rustc ${found:-missing} on PATH, but rust-toolchain.toml pins $pinned (put rustup's toolchain first on PATH)"

# --- Source tree and build tree, kept between runs ----------------------------
cache="$HOME/Library/Caches/ferny-package"
src="$cache/src"
build="$cache/build-release-macos"
mkdir -p "$cache"
chmod 700 "$cache"
# One run at a time: two runs would share and corrupt the build tree.
mkdir "$cache/lock" 2>/dev/null || die "another run holds $cache/lock (remove it if no run is active)"
work="$(mktemp -d "${TMPDIR:-/tmp}/ferny-package.XXXXXX")"
mount_point=""
cleanup() {
  if [ -n "$mount_point" ]; then hdiutil detach -quiet "$mount_point" || true; fi
  rm -rf "$work"
  rmdir "$cache/lock" 2>/dev/null || true
}
trap cleanup EXIT

echo "package-macos: $ref ($short), version $version"
mkdir -p "$work/export" "$src"
git archive --format=tar "$commit" | tar -x -C "$work/export"
# Copy by content (-c) and without times (no -t): unchanged files keep their
# modification time, so CMake and Cargo do not rebuild them.
rsync -rcp --delete "$work/export/" "$src/"

# --- Build (offline, Release, bundle) -----------------------------------------
export CARGO_NET_OFFLINE=true
# ccache is optional and is not a dependency of Ferny. Its cache keys include
# file paths, so the stable $src path above is what makes hits possible.
launcher=()
if command -v ccache >/dev/null 2>&1; then
  export CCACHE_DIR="$cache/ccache" CCACHE_BASEDIR="$src" CCACHE_MAXSIZE=5G
  launcher=(-DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache)
fi
generator=()
if command -v ninja >/dev/null 2>&1; then generator=(-G Ninja); fi
cmake -S "$src" -B "$build" "${generator[@]-}" "${launcher[@]-}" \
  -DCMAKE_BUILD_TYPE=Release -DFERNY_MACOS_BUNDLE=ON -DFERNY_WARNINGS_AS_ERRORS=ON
cmake --build "$build" --parallel --target ferny

built="$build/ui/ferny.app"
[ -d "$built" ] || die "bundle not built"
stage="$work/stage"
app="$stage/Ferny.app"
mkdir -p "$stage"
cp -R "$built" "$app"

# Icon from the embedded application image.
iconset="$work/ferny.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$src/imagenes/ferny-icon-app.png" \
    --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$src/imagenes/ferny-icon-app.png" \
    --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
mkdir -p "$app/Contents/Resources"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/ferny.icns"

# Only the Qt plugins Ferny uses: the Cocoa platform, the offscreen
# platform (smoke tests), the macOS style, and the SVG icon engine
# (for toolbar icons from :/lucide/*.svg resources). macdeployqt would
# otherwise copy every plugin installed in Homebrew (QML, PDF, virtual
# keyboard, …).
plugin_dir="$(qmake -query QT_INSTALL_PLUGINS)"
extra=()
for plugin in platforms/libqcocoa.dylib platforms/libqoffscreen.dylib styles/libqmacstyle.dylib iconengines/libqsvgicon.dylib; do
  mkdir -p "$app/Contents/PlugIns/$(dirname "$plugin")"
  cp "$plugin_dir/$plugin" "$app/Contents/PlugIns/$plugin"
  chmod u+w "$app/Contents/PlugIns/$plugin"
  extra+=("-executable=$app/Contents/PlugIns/$plugin")
done
# Qt frameworks and the libraries they need, for the app and those plugins.
macdeployqt "$app" -always-overwrite -no-plugins "${extra[@]}"
# Plugins and Qt's own translations from inside the bundle only: without
# this file QtCore would look in the Homebrew prefix compiled into it.
mkdir -p "$app/Contents/Resources/translations"
cp "$(qmake -query QT_INSTALL_TRANSLATIONS)/qtbase_es.qm" "$app/Contents/Resources/translations/"
cat >"$app/Contents/Resources/qt.conf" <<'CONF'
[Paths]
Prefix = .
Plugins = PlugIns
Translations = Resources/translations
CONF

# --- Licences, components and source pointer -----------------------------------
licenses="$app/Contents/Resources/licenses"
mkdir -p "$licenses"
cp "$src/LICENSE" "$licenses/Ferny-GPL-2.0.txt"
cp "$src/NOTICE" "$licenses/NOTICE.txt"
cp "$src/packaging/macos/licenses/LGPL-3.0.txt" "$licenses/Qt-LGPL-3.0.txt"
cp "$src/packaging/macos/licenses/GPL-3.0.txt" "$licenses/GPL-3.0.txt"
cp "$src/third_party/scintilla/License.txt" "$licenses/Scintilla.txt"
cp "$src/third_party/lexilla/License.txt" "$licenses/Lexilla.txt"

remote="$(git remote get-url origin 2>/dev/null || echo "[TO COMPLETE]")"
qt_version="$(qmake -query QT_VERSION 2>/dev/null || echo "not recorded")"
{
  echo "Ferny $version"
  echo
  echo "Source: $remote"
  echo "Commit: $commit (ref $ref)"
  echo "Built from \`git archive\` of that commit; see BUILDING in the"
  echo "repository (docs/SETUP.md). The source of Qt is available at"
  echo "https://download.qt.io/official_releases/qt/ (version below)."
} >"$licenses/SOURCE.txt"
{
  echo "Ferny $version ($commit), macOS arm64"
  echo
  echo "Qt: $qt_version (Homebrew, LGPL-3.0, dynamically linked)"
  # Vendored versions as recorded in third_party/README.md.
  grep -E '^\| .(scintilla|lexilla)/. \|' "$src/third_party/README.md" |
    awk -F'|' '{gsub(/[` ]/,"",$2); sub(/\/$/,"",$2); sub(/^ +/,"",$3); split($3,v," "); print $2 ": " v[1] " (vendored, licence in this folder)"}'
  echo
  echo "Bundled frameworks and libraries:"
  (cd "$app/Contents" && find Frameworks PlugIns -maxdepth 2 \( -name '*.framework' -o -name '*.dylib' \) | sort | sed 's/^/  /')
  echo
  echo "Rust crates (Cargo.lock):"
  awk '/^name = /{n=$3} /^version = / && n != "" {gsub(/"/,"",n); v=$3; gsub(/"/,"",v); print "  " n " " v; n=""}' "$src/Cargo.lock"
} >"$licenses/COMPONENTS.txt"

# Absolute rpaths left by Homebrew builds (e.g. libdbus) would let a
# library outside the bundle be loaded: remove them.
while IFS= read -r -d '' bin; do
  if file -b "$bin" | grep -q 'Mach-O'; then
    otool -l "$bin" | awk '/cmd LC_RPATH$/ {want=1; next} want && $1 == "path" {print $2; want=0}' |
      { grep -v '^@' || true; } | while IFS= read -r rpath; do
      chmod u+w "$bin"
      install_name_tool -delete_rpath "$rpath" "$bin" 2>/dev/null
    done
  fi
done < <(find "$app" -type f -print0)

# --- SBOM (H4.3): crates, vendored code and Qt, from the same tree ---------------
timestamp="$(git show -s --format=%cI "$commit")"
(cd "$src" && cargo metadata --offline --locked --format-version 1 \
  --filter-platform aarch64-apple-darwin >"$work/metadata.json")
(cd "$src" && cargo run -q --offline --locked --release -p ferny-sbom -- \
  --metadata "$work/metadata.json" --lock "$src/Cargo.lock" \
  --third-party "$src/third_party/README.md" --qt "$qt_version" \
  --version "$version" --timestamp "$timestamp") >"$licenses/sbom.cdx.json"

# --- Signature (macdeployqt invalidated the linker's) ------------------------------
if [ -n "$identity" ]; then
  # Developer ID: hardened runtime and a secure timestamp, for notarisation.
  codesign --force --deep --options runtime --timestamp --sign "$identity" "$app"
else
  codesign --force --deep --sign - "$app"
fi

# --- Verification -----------------------------------------------------------------
codesign --verify --deep --strict "$app" || die "signature verification failed"
bad=0
while IFS= read -r -d '' bin; do
  if file -b "$bin" | grep -q 'Mach-O'; then
    archs="$(lipo -archs "$bin" 2>/dev/null || true)"
    case " $archs " in *" arm64 "*) ;; *) echo "not arm64: $bin ($archs)" >&2; bad=1 ;; esac
    # Every loaded library must be inside the bundle or part of the system,
    # and every rpath must be relative to the bundle.
    outside="$(otool -l "$bin" | awk '
      /cmd LC_(LOAD|LOAD_WEAK|REEXPORT|LAZY_LOAD)_DYLIB$/ {want="name"; next}
      /cmd LC_RPATH$/ {want="path"; next}
      want != "" && $1 == want {print want " " $2; want=""}' |
      grep -vE '^name (@rpath|@executable_path|@loader_path|/System/|/usr/lib/)' |
      grep -vE '^path @' || true)"
    if [ -n "$outside" ]; then
      echo "refers outside the bundle: $bin" >&2
      printf '%s\n' "$outside" >&2
      bad=1
    fi
  fi
done < <(find "$app" -type f -print0)
[ "$bad" -eq 0 ] || die "bundle verification failed"

# --- DMG -----------------------------------------------------------------------------
cp "$licenses/Ferny-GPL-2.0.txt" "$stage/LICENSE.txt"
ln -s /Applications "$stage/Applications"
# SOURCE.txt is license/source metadata for the app bundle only; never put it
# at the DMG root or expose it as an installer document.
[ ! -e "$stage/SOURCE.txt" ] || die "SOURCE.txt must not be included in the DMG"
hdiutil create -quiet -volname "Ferny $version" -srcfolder "$stage" -format UDZO -fs HFS+ "$work/$name.dmg"
if [ -n "$identity" ]; then
  codesign --timestamp --sign "$identity" "$work/$name.dmg"
fi

# Smoke test of the app inside the mounted image.
mkdir -p "$work/mnt"
# Physical path (/var is a link to /private/var), as Qt reports it.
mount_point="$(cd "$work/mnt" && pwd -P)"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount_point" "$work/$name.dmg"
if [ -e "$mount_point/SOURCE.txt" ]; then
  die "SOURCE.txt must not be included in the DMG"
fi
smoke="$(QT_QPA_PLATFORM=offscreen QT_DEBUG_PLUGINS=1 \
  "$mount_point/Ferny.app/Contents/MacOS/ferny" --smoke-test --language es 2>"$work/plugins.log" || true)"
case "$smoke" in
  *"smoke: ok icon=yes logo=yes"*"smoke: menus &Archivo"*) echo "package-macos: smoke test passed" ;;
  *) die "smoke test of the packaged app failed" ;;
esac
# Plugins must come from the bundle, never from a Qt installation.
if grep -E 'loaded library|Found metadata in lib' "$work/plugins.log" | grep -v "$mount_point" | grep -q .; then
  grep -E 'loaded library|Found metadata in lib' "$work/plugins.log" | grep -v "$mount_point" >&2
  die "the packaged app loaded Qt plugins from outside the bundle"
fi
grep 'loaded library' "$work/plugins.log" | grep -qF "\"$mount_point/Ferny.app/Contents/PlugIns/" ||
  die "no plugin loaded from the bundle"
hdiutil detach -quiet "$mount_point"
mount_point=""

if [ -n "$notary" ]; then
  xcrun notarytool submit "$work/$name.dmg" --keychain-profile "$notary" --wait
  xcrun stapler staple "$work/$name.dmg"
  spctl --assess --type open --context context:primary-signature "$work/$name.dmg" ||
    die "Gatekeeper rejects the notarised DMG"
fi

cp "$work/$name.dmg" "$dmg"
cp "$licenses/sbom.cdx.json" "$out/$name.cdx.json"
(cd "$out" && shasum -a 256 "$name.dmg" >"$name.dmg.sha256")
if [ -n "$minisign_key" ]; then
  minisign -S -s "$minisign_key" -m "$dmg.sha256" -t "Ferny $version ($short) $name.dmg"
fi
echo "package-macos: wrote $dmg"
cat "$dmg.sha256"
if [ -n "$identity" ]; then echo "package-macos: signed with a Developer ID"; else echo "package-macos: ad-hoc signature (no Developer ID)"; fi
[ -n "$notary" ] && echo "package-macos: notarised and stapled"
[ -n "$minisign_key" ] && echo "package-macos: checksum signed: $dmg.sha256.minisig"
true
