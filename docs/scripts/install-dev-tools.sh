#!/usr/bin/env bash
# Install Ferny development tools on macOS (Homebrew) or Debian 13 (apt).
#
# Usage: bash docs/scripts/install-dev-tools.sh [options]
#        (works from bash or zsh; the script re-runs itself with bash)
#
# Options:
#   --dry-run             print what would be done, change nothing
#   --with-fuzz           also install the nightly toolchain and cargo-fuzz
#   --with-release-tools  also install minisign and syft (release builds)
#   --update-shell-profile  add the PATH lines to ~/.zprofile (macOS) or
#                         ~/.profile (Debian) if they are missing
#   -h, --help            show this help
#
# Installs: git, gitleaks, rustup + stable toolchain (rustfmt, clippy),
# cargo-deny, cargo-audit, cmake, ninja, clang-format, shellcheck, semgrep,
# PowerShell (macOS only, to test scripts/check.ps1) and Qt (Homebrew `qt`
# on macOS, Debian's Qt 6.8 packages on Debian; versions in
# docs/DEPENDENCIES.md). The Rust version itself comes from
# rust-toolchain.toml and is installed by rustup on first use.
#
# Safety: uses the official package managers; any direct download is pinned
# and verified against the published SHA-256 before use. Already installed
# tools are skipped. Nothing is piped from the network into a shell.
#
# Environment overrides (pin versions):
#   CARGO_DENY_VERSION, CARGO_AUDIT_VERSION, CARGO_FUZZ_VERSION  (default: latest)
#   GITLEAKS_VERSION   (Debian fallback download, default 8.30.1)

# Re-run with bash when started from zsh or sh.
if [ -z "${BASH_VERSION:-}" ]; then exec bash "$0" "$@"; fi
set -uo pipefail

DRY_RUN=0; WITH_FUZZ=0; WITH_RELEASE=0; UPDATE_PROFILE=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --with-fuzz) WITH_FUZZ=1 ;;
    --with-release-tools) WITH_RELEASE=1 ;;
    --update-shell-profile) UPDATE_PROFILE=1 ;;
    -h|--help) sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "install-dev-tools: unknown option: $arg" >&2; exit 2 ;;
  esac
done

GITLEAKS_VERSION="${GITLEAKS_VERSION:-8.30.1}"
FAILURES=0

say()  { printf '\n==> %s\n' "$*"; }
note() { printf '    %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# Run a command, or print it in dry-run mode. Counts failures, never aborts,
# so one broken package does not hide the state of the others.
do_run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '    [dry-run] %s\n' "$*"
    return 0
  fi
  printf '    $ %s\n' "$*"
  if ! "$@"; then
    printf '    !! failed: %s\n' "$*" >&2
    FAILURES=$((FAILURES + 1))
    return 1
  fi
}

# --- Detect platform ------------------------------------------------------
OS="$(uname -s)"
PLATFORM=""
if [ "$OS" = "Darwin" ]; then
  PLATFORM="macos"
elif [ -r /etc/os-release ] && grep -qi '^ID=debian' /etc/os-release; then
  PLATFORM="debian"
  # shellcheck source=/dev/null
  . /etc/os-release
  case "${VERSION_ID:-}" in
    13) ;;
    *) note "warning: tested for Debian 13; found Debian ${VERSION_ID:-unknown}" ;;
  esac
else
  echo "install-dev-tools: supported systems are macOS and Debian 13." >&2
  exit 2
fi
say "Platform: $PLATFORM ($(uname -m))$([ "$DRY_RUN" -eq 1 ] && echo ', dry run')"

# --- macOS ------------------------------------------------------------------
install_macos() {
  if ! have brew; then
    echo "Homebrew is required: https://brew.sh (install it, then re-run)." >&2
    exit 1
  fi
  local formulae="git gitleaks rustup cmake ninja clang-format shellcheck semgrep qt"
  [ "$WITH_RELEASE" -eq 1 ] && formulae="$formulae minisign syft"

  say "Homebrew packages"
  local missing=""
  for f in $formulae; do
    if brew list --formula "$f" >/dev/null 2>&1; then note "ok  $f (already installed)"
    else missing="$missing $f"; fi
  done
  # shellcheck disable=SC2086
  [ -n "$missing" ] && do_run brew install $missing

  say "PowerShell (to test scripts/check.ps1 on macOS)"
  # PowerShell is a Homebrew formula (the former cask no longer exists).
  if have pwsh; then note "ok  pwsh (already installed)"
  else do_run brew install powershell; fi

  # rustup from Homebrew is keg-only: its proxies (cargo, rustc) live in
  # its own bin directory, and `cargo install` puts tools in ~/.cargo/bin.
  RUSTUP_BIN="$(brew --prefix rustup 2>/dev/null)/bin"
  export PATH="$RUSTUP_BIN:$HOME/.cargo/bin:$PATH"
  PROFILE_FILE="$HOME/.zprofile"
  PROFILE_LINE="export PATH=\"$RUSTUP_BIN:\$HOME/.cargo/bin:\$PATH\""
}

# --- Debian 13 --------------------------------------------------------------
apt_has() { apt-cache show "$1" >/dev/null 2>&1; }

install_gitleaks_release() {
  # Pinned release, verified against the published checksum file.
  local arch asset base tmp
  case "$(uname -m)" in
    x86_64) arch="x64" ;;
    aarch64) arch="arm64" ;;
    *) echo "    !! no gitleaks release for $(uname -m)" >&2; FAILURES=$((FAILURES + 1)); return 1 ;;
  esac
  asset="gitleaks_${GITLEAKS_VERSION}_linux_${arch}.tar.gz"
  base="https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}"
  if [ "$DRY_RUN" -eq 1 ]; then
    note "[dry-run] download $base/$asset, verify SHA-256, install to /usr/local/bin"
    return 0
  fi
  tmp="$(mktemp -d)"
  if curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$base/$asset" &&
     curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/checksums.txt" "$base/gitleaks_${GITLEAKS_VERSION}_checksums.txt" &&
     (cd "$tmp" && grep " $asset\$" checksums.txt | sha256sum -c -) &&
     tar -xzf "$tmp/$asset" -C "$tmp" gitleaks &&
     sudo install -m 0755 "$tmp/gitleaks" /usr/local/bin/gitleaks; then
    note "ok  gitleaks $GITLEAKS_VERSION (verified release)"
  else
    echo "    !! gitleaks download or checksum verification failed" >&2
    FAILURES=$((FAILURES + 1))
  fi
  rm -rf "$tmp"
}

install_debian() {
  if ! have sudo; then
    echo "sudo is required to install packages." >&2
    exit 1
  fi
  local pkgs="git ca-certificates curl build-essential pkg-config libssl-dev cmake ninja-build clang-format shellcheck pipx"
  # Qt 6.8 LTS from Debian (API floor of Ferny; docs/DEPENDENCIES.md).
  # qt6-5compat-dev: Scintilla's Qt layer uses QTextCodec (Core5Compat).
  # qt6-l10n-tools: lupdate/lrelease for the translations (LinguistTools).
  pkgs="$pkgs qt6-base-dev qt6-base-dev-tools qt6-tools-dev qt6-tools-dev-tools qt6-l10n-tools qt6-5compat-dev libgl-dev"
  [ "$WITH_RELEASE" -eq 1 ] && pkgs="$pkgs minisign"
  local want_rustup=0 want_gitleaks=0
  if apt_has rustup; then pkgs="$pkgs rustup"; else want_rustup=1; fi
  if have gitleaks; then :; elif apt_has gitleaks; then pkgs="$pkgs gitleaks"; else want_gitleaks=1; fi

  say "apt packages"
  do_run sudo apt-get update
  # shellcheck disable=SC2086
  do_run sudo apt-get install -y --no-install-recommends $pkgs

  if [ "$want_gitleaks" -eq 1 ]; then
    say "gitleaks (not in apt: pinned GitHub release)"
    install_gitleaks_release
  fi

  if [ "$want_rustup" -eq 1 ] && ! have rustup; then
    say "rustup (not in apt: official rustup-init, checksum verified)"
    local triple url tmp
    triple="$(uname -m)-unknown-linux-gnu"
    url="https://static.rust-lang.org/rustup/dist/$triple/rustup-init"
    if [ "$DRY_RUN" -eq 1 ]; then
      note "[dry-run] download $url, verify SHA-256, run rustup-init -y --no-modify-path"
    else
      tmp="$(mktemp -d)"
      if curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/rustup-init" "$url" &&
         curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/rustup-init.sha256" "$url.sha256" &&
         [ "$(cut -d' ' -f1 "$tmp/rustup-init.sha256")" = "$(sha256sum "$tmp/rustup-init" | cut -d' ' -f1)" ]; then
        chmod +x "$tmp/rustup-init" && do_run "$tmp/rustup-init" -y --no-modify-path --default-toolchain none
      else
        echo "    !! rustup-init download or checksum verification failed" >&2
        FAILURES=$((FAILURES + 1))
      fi
      rm -rf "$tmp"
    fi
  fi

  say "Semgrep (pipx)"
  if have semgrep; then note "ok  semgrep (already installed)"
  else do_run pipx install semgrep; fi

  if [ "$WITH_RELEASE" -eq 1 ] && ! have syft; then
    note "syft is not packaged in Debian; install it from https://github.com/anchore/syft/releases"
    note "(verify the checksum) when release builds start."
  fi

  export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
  PROFILE_FILE="$HOME/.profile"
  PROFILE_LINE="export PATH=\"\$HOME/.cargo/bin:\$HOME/.local/bin:\$PATH\""
}

# --- Rust toolchain and cargo tools (both platforms) -------------------------
install_rust_tools() {
  say "Rust stable toolchain"
  if [ "$DRY_RUN" -eq 0 ] && ! have rustup; then
    echo "    !! rustup not found after installation; check the output above" >&2
    FAILURES=$((FAILURES + 1)); return 1
  fi
  do_run rustup toolchain install stable --profile minimal --component rustfmt,clippy
  do_run rustup default stable

  say "Cargo tools"
  local spec name ver
  for spec in "cargo-deny:${CARGO_DENY_VERSION:-}" "cargo-audit:${CARGO_AUDIT_VERSION:-}"; do
    name="${spec%%:*}"; ver="${spec#*:}"
    if [ "$DRY_RUN" -eq 0 ] && have "$name" && [ -z "$ver" ]; then
      note "ok  $name (already installed)"; continue
    fi
    if [ -n "$ver" ]; then do_run cargo install --locked "$name" --version "$ver"
    else do_run cargo install --locked "$name"; fi
  done

  if [ "$WITH_FUZZ" -eq 1 ]; then
    say "Fuzzing: nightly toolchain and cargo-fuzz"
    do_run rustup toolchain install nightly --profile minimal
    if [ -n "${CARGO_FUZZ_VERSION:-}" ]; then do_run cargo install --locked cargo-fuzz --version "$CARGO_FUZZ_VERSION"
    else do_run cargo install --locked cargo-fuzz; fi
  fi
}

update_profile() {
  [ "$UPDATE_PROFILE" -eq 1 ] || return 0
  say "Shell profile ($PROFILE_FILE)"
  if [ -f "$PROFILE_FILE" ] && grep -qF "$PROFILE_LINE" "$PROFILE_FILE"; then
    note "ok  PATH line already present"
  elif [ "$DRY_RUN" -eq 1 ]; then
    note "[dry-run] append to $PROFILE_FILE: $PROFILE_LINE"
  else
    printf '\n# Added by Ferny docs/scripts/install-dev-tools.sh\n%s\n' "$PROFILE_LINE" >> "$PROFILE_FILE"
    note "added PATH line; open a new terminal to use it"
  fi
}

# --- Main ---------------------------------------------------------------------
case "$PLATFORM" in
  macos) install_macos ;;
  debian) install_debian ;;
esac
install_rust_tools
update_profile

say "Installed versions"
# Print the first output line that contains a version number.
show() {
  if have "$1"; then
    printf '    %-13s %s\n' "$1" "$("$@" 2>/dev/null | grep -m 1 -E '[0-9]+\.[0-9]+' || true)"
  else
    printf '    %-13s %s\n' "$1" "missing"
  fi
}
show git --version
show gitleaks version
show rustup --version
show cargo --version
show cargo-deny --version
show cargo-audit --version
show cmake --version
show ninja --version
show clang-format --version
show shellcheck --version
show semgrep --version
show qmake6 --version
[ "$PLATFORM" = "macos" ] && show qmake --version
[ "$PLATFORM" = "macos" ] && show pwsh --version
[ "$WITH_FUZZ" -eq 1 ] && show cargo-fuzz --version
[ "$WITH_RELEASE" -eq 1 ] && show minisign -v && show syft version

if [ "$UPDATE_PROFILE" -eq 0 ] && ! { [ -f "$PROFILE_FILE" ] && grep -qF "$PROFILE_LINE" "$PROFILE_FILE"; }; then
  say "PATH"
  note "Add this line to $PROFILE_FILE (or re-run with --update-shell-profile):"
  note "$PROFILE_LINE"
fi

if [ "$FAILURES" -gt 0 ]; then
  say "Finished with $FAILURES failure(s); see the messages above."
  exit 1
fi
say "Done. Verify with: scripts/check.sh"
