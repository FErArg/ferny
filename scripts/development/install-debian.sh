#!/usr/bin/env bash
# Install Ferny development and Debian DEB packaging tools.
# Usage: bash scripts/development/install-debian.sh [--dry-run] [--with-release-tools]
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
    *) echo "install-debian: unknown option: $arg" >&2; exit 2 ;;
  esac
done

run() {
  if [ "$dry_run" -eq 1 ]; then
    printf '[dry-run]'; printf ' %q' "$@"; printf '\n'
  else
    "$@"
  fi
}

[ "$(uname -s)" = Linux ] || { echo 'Linux required' >&2; exit 2; }
[ -r /etc/os-release ] || { echo '/etc/os-release missing' >&2; exit 2; }
# shellcheck source=/dev/null
. /etc/os-release
[ "${ID:-}" = debian ] || { echo 'Debian required' >&2; exit 2; }
if [ "${VERSION_ID:-}" != 13 ]; then
  echo "warning: tested on Debian 13; found ${VERSION_ID:-unknown}" >&2
fi

packages=(git ca-certificates curl build-essential pkg-config libssl-dev cmake ninja-build clang-format shellcheck pipx \
  qt6-base-dev qt6-base-dev-tools qt6-tools-dev qt6-tools-dev-tools qt6-l10n-tools qt6-5compat-dev libgl-dev)
[ "$with_release" -eq 1 ] && packages+=(minisign)
run sudo apt-get update
run sudo apt-get install -y --no-install-recommends "${packages[@]}"

if ! command -v rustup >/dev/null 2>&1; then
  echo 'rustup missing; install from https://rustup.rs, then rerun this script.' >&2
  exit 1
fi
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
run rustup toolchain install stable --profile minimal --component rustfmt,clippy
run rustup default stable
run cargo install --locked cargo-deny
run cargo install --locked cargo-audit
run cargo fetch --locked

if [ "$dry_run" -eq 0 ]; then
  command -v qmake6 >/dev/null 2>&1 || { echo 'qmake6 missing after Qt installation' >&2; exit 1; }
fi

cat <<'EOF'
Ready for development and DEB packaging:
  scripts/dev.sh
  scripts/package-deb.sh --out bin
Expected artifact: bin/ferny-v<VERSION>-linux-<arch>.deb
EOF
