#!/usr/bin/env bash
# Build a Debian package from a committed ref or explicit local source tree,
# without installing it.
#
# Usage: scripts/package-deb.sh [--ref <tag-or-commit> | --source-tree <dir>] [--out <dir>]
# Defaults: --ref HEAD, --out bin. --ref exports committed source with git archive
# and requires a clean tracked working tree.
# --source-tree packages an explicit local tree and reports it as an uncommitted build.
# Existing output files are never replaced.
set -euo pipefail

usage() {
  echo "usage: scripts/package-deb.sh [--ref <tag-or-commit> | --source-tree <dir>] [--out <dir>]" >&2
  exit 2
}
die() {
  echo "package-deb: $*" >&2
  exit 1
}

ref="HEAD"
source_tree=""
ref_set=0
out="bin"
while [ $# -gt 0 ]; do
  case "$1" in
    --ref) [ $# -ge 2 ] || usage; ref="$2"; ref_set=1; shift 2 ;;
    --source-tree) [ $# -ge 2 ] || usage; source_tree="$2"; shift 2 ;;
    --out) [ $# -ge 2 ] || usage; out="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) usage ;;
  esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [ -n "$source_tree" ] && [ "$ref_set" -eq 1 ]; then
  die "choose either --ref or --source-tree"
fi
source_label=""
if [ -n "$source_tree" ]; then
  [ -d "$source_tree" ] || die "source tree does not exist: $source_tree"
  source_tree="$(cd "$source_tree" && pwd)"
  case "$source_tree/" in "$root/"*) die "source tree must be outside the repository (build output would contaminate source copy)" ;; esac
  [ -f "$source_tree/Cargo.toml" ] || die "source tree has no Cargo.toml"
  version="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' "$source_tree/Cargo.toml" | head -n 1)"
  [ -n "$version" ] || die "no workspace version in source tree Cargo.toml"
  source_label="source tree $source_tree (uncommitted source; not a release build)"
else
  commit="$(git rev-parse --verify --quiet "${ref}^{commit}")" || die "unknown ref: $ref"
  version="$(git show "$commit:Cargo.toml" | sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' | head -n 1)"
  [ -n "$version" ] || die "no workspace version in Cargo.toml at $ref"
  if git rev-parse --verify --quiet "refs/tags/$ref" >/dev/null; then
    [ "$ref" = "v$version" ] || die "tag $ref does not match version $version"
  fi
  source_label="$ref ($(git rev-parse --short "$commit"))"
fi
[ "$(uname -s)" = Linux ] || die "Linux only"
case "$(uname -m)" in
  x86_64) arch=amd64 ;;
  *) die "x86_64 only (Debian architecture amd64)" ;;
esac

name="Ferny-v${version}-linux-x86_64.deb"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
deb="$out/$name"
[ ! -e "$deb" ] || die "$deb exists; earlier artefacts are never overwritten"
if [ -z "$source_tree" ]; then
  [ -z "$(git status --porcelain --untracked-files=no)" ] ||
    die "working tree has tracked changes; build only from a clean committed ref"
fi

for tool in cmake cargo dpkg-deb sha256sum file; do
  command -v "$tool" >/dev/null 2>&1 || die "missing tool: $tool"
done
if command -v qmake >/dev/null 2>&1; then
  qmake_bin=qmake
elif command -v qmake6 >/dev/null 2>&1; then
  qmake_bin=qmake6
else
  die "missing Qt development tools (qmake or qmake6)"
fi
printf 'package-deb: Qt %s\n' "$($qmake_bin -query QT_VERSION)"
# Emit explicit runtime dependencies for the supported Debian 13 Qt 6.8 baseline.
depends='libc6, libgcc-s1, libstdc++6, libqt6core6t64, libqt6gui6, libqt6widgets6, libqt6network6, libqt6printsupport6, libqt6core5compat6'

work="$(mktemp -d "${TMPDIR:-/tmp}/ferny-deb.XXXXXX")"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT
mkdir -p "$work/src"
if [ -n "$source_tree" ]; then
  cp -R "$source_tree/." "$work/src/"
else
  git archive --format=tar "$commit" | tar -x -C "$work/src"
fi

echo "package-deb: $source_label, version $version, amd64"
export CARGO_NET_OFFLINE=true
generator=()
if command -v ninja >/dev/null 2>&1; then generator=(-G Ninja); fi
cmake -S "$work/src" -B "$work/build" "${generator[@]}" \
  -DCMAKE_BUILD_TYPE=Release -DFERNY_WARNINGS_AS_ERRORS=OFF \
  -DCMAKE_CXX_FLAGS=-Wno-error=shadow
cmake --build "$work/build" --parallel --target ferny

stage="$work/root"
install -d "$stage/usr/bin" "$stage/usr/share/applications" \
  "$stage/usr/share/icons/hicolor/256x256/apps" "$stage/usr/share/doc/ferny" \
  "$stage/DEBIAN"
install -m 0755 "$work/build/ui/ferny" "$stage/usr/bin/ferny"
install -m 0644 "$work/src/imagenes/ferny-icon-app.png" \
  "$stage/usr/share/icons/hicolor/256x256/apps/ferny.png"
cat >"$stage/usr/share/applications/ferny.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Ferny
GenericName=Programmer's Editor
Comment=Geany-inspired programmer's editor
Exec=ferny %F
Icon=ferny
Terminal=false
Categories=Development;IDE;TextEditor;
MimeType=text/plain;text/x-c;text/x-c++;text/x-python;text/x-rust;text/markdown;
StartupNotify=true
DESKTOP
install -m 0644 "$work/src/LICENSE" "$stage/usr/share/doc/ferny/copyright"
cat >"$stage/DEBIAN/control" <<CONTROL
Package: ferny
Version: $version
Section: devel
Priority: optional
Architecture: $arch
Maintainer: FErArg <contact@ferarg.com>
Homepage: https://www.ferarg.com
Depends: $depends
Description: Geany-inspired programmer's editor
 Ferny is a desktop programmer's editor built with Rust, Qt 6 and Scintilla,
 based on the Geany project.
CONTROL
chmod 0755 "$stage/DEBIAN"
chmod 0644 "$stage/DEBIAN/control"

"$stage/usr/bin/ferny" --version >"$work/version.txt" 2>&1 ||
  die "built executable version check failed"
grep -F "Ferny $version" "$work/version.txt" >/dev/null ||
  die "executable version does not match package version $version"
QT_QPA_PLATFORM=offscreen "$stage/usr/bin/ferny" --smoke-test >"$work/smoke.txt" 2>&1 ||
  die "packaged executable smoke test failed"
grep -F 'smoke: ok' "$work/smoke.txt" >/dev/null || die "smoke test did not report success"

dpkg-deb --root-owner-group --build "$stage" "$work/$name" >/dev/null
actual_name="$(dpkg-deb -f "$work/$name" Package)"
actual_version="$(dpkg-deb -f "$work/$name" Version)"
actual_arch="$(dpkg-deb -f "$work/$name" Architecture)"
[ "$actual_name" = ferny ] || die "wrong package name: $actual_name"
[ "$actual_version" = "$version" ] || die "wrong package version: $actual_version"
[ "$actual_arch" = "$arch" ] || die "wrong package architecture: $actual_arch"
contents="$(dpkg-deb -c "$work/$name")"
printf '%s\n' "$contents" | grep -F 'usr/bin/ferny' >/dev/null || die "package lacks executable"
printf '%s\n' "$contents" | grep -F 'usr/share/applications/ferny.desktop' >/dev/null || die "package lacks desktop entry"
dpkg-deb --extract "$work/$name" "$work/verify"
"$work/verify/usr/bin/ferny" --version >"$work/packaged-version.txt" 2>&1 ||
  die "extracted package executable version check failed"
grep -F "Ferny $version" "$work/packaged-version.txt" >/dev/null ||
  die "packaged executable reports an unexpected version"

cp "$work/$name" "$deb"
echo "package-deb: wrote $deb"
sha256sum "$deb"
