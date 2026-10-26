#!/usr/bin/env bash
# Install Ferny development and macOS DMG packaging tools.
# Usage: bash scripts/development/install-macos.sh [--dry-run] [--with-release-tools]
set -euo pipefail

dry_run=0
with_release=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    --with-release-tools) with_release=1 ;;
    -h|--help)
      sed -n '2,4p' "$0"
      exit 0
      ;;
    *) echo "install-macos: unknown option: $arg" >&2; exit 2 ;;
  esac
done

run() {
  if [ "$dry_run" -eq 1 ]; then
    printf '[dry-run]'; printf ' %q' "$@"; printf '\n'
  else
    "$@"
  fi
}

[ "$(uname -s)" = Darwin ] || { echo 'macOS required' >&2; exit 2; }
[ "$(uname -m)" = arm64 ] || { echo 'macOS arm64 required' >&2; exit 2; }
command -v brew >/dev/null 2>&1 || { echo 'Homebrew required: https://brew.sh' >&2; exit 1; }

formulae=(git gitleaks rustup cmake ninja clang-format shellcheck semgrep qt powershell)
[ "$with_release" -eq 1 ] && formulae+=(minisign syft)
missing=()
for formula in "${formulae[@]}"; do
  brew list --formula "$formula" >/dev/null 2>&1 || missing+=("$formula")
done
[ "${#missing[@]}" -eq 0 ] || run brew install "${missing[@]}"

rustup_bin="$(brew --prefix rustup)/bin"
export PATH="$rustup_bin:$HOME/.cargo/bin:$PATH"
run rustup toolchain install stable --profile minimal --component rustfmt,clippy
run rustup default stable
run cargo install --locked cargo-deny
run cargo install --locked cargo-audit
run cargo fetch --locked

if [ "$dry_run" -eq 0 ]; then
  command -v qmake >/dev/null 2>&1 || { echo 'qmake missing after Qt installation' >&2; exit 1; }
  command -v macdeployqt >/dev/null 2>&1 || { echo 'macdeployqt missing after Qt installation' >&2; exit 1; }
fi

cat <<'EOF'
Ready for development and DMG packaging:
  scripts/dev.sh
  scripts/package-macos.sh --ref HEAD --out bin
Expected artifact: bin/ferny-v<VERSION>-macos-arm64.dmg
EOF
