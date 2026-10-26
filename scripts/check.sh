#!/usr/bin/env bash
# Ferny local validation gate for macOS and Linux (docs/RELEASE-GATE.md).
#
# Runs every check that applies to the current state of the repository and
# prints a summary block to paste into milestone reports. A check whose tool
# or inputs do not exist yet is reported as SKIP (not verified), never as PASS.
#
# Usage: scripts/check.sh [--strict] [--help]
#   --strict  treat skipped checks as failures (use once the code exists)
#
# Exit codes: 0 all run checks passed (and nothing skipped with --strict),
#             1 at least one check failed, 2 usage error.
#
# Compatible with bash 3.2 (macOS default): no associative arrays, no mapfile.
set -uo pipefail

STRICT=0
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "check.sh: unknown option: $arg" >&2; exit 2 ;;
  esac
done

# Always validate the repository this script belongs to, whatever the cwd.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel 2>/dev/null)" || {
  echo "check.sh: the script must live inside the Ferny Git repository" >&2; exit 2; }
cd "$ROOT" || exit 2

RESULTS=()     # "STATUS|check|detail"
FAILED=0
SKIPPED=0

record() { RESULTS+=("$1|$2|$3"); }
pass()   { record PASS "$1" "${2:-}"; }
fail()   { record FAIL "$1" "${2:-}"; FAILED=$((FAILED + 1)); }
skip()   { record SKIP "$1" "${2:-not verified}"; SKIPPED=$((SKIPPED + 1)); }
have()   { command -v "$1" >/dev/null 2>&1; }
cpus()   { getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4; } # default parallelism

# Run a check: name, then the command. Output is shown only on failure.
# The elapsed time is printed so the gate's cost can be measured per step.
run() {
  local name="$1"; shift
  local out start=$SECONDS
  printf '  %-26s ' "$name"
  if out="$("$@" 2>&1)"; then
    echo "ok ($((SECONDS - start)) s)"; pass "$name"
  else
    echo "FAILED"; printf '%s\n' "$out" | tail -n 40 | sed 's/^/      /'
    fail "$name" "see output above"
  fi
}

has_files() { # has_files <git pathspec...>: any tracked or new file matches
  [ -n "$(git ls-files --cached --others --exclude-standard -- "$@" | head -n 1)" ]
}

echo "Ferny validation gate"
echo

# --- Repository hygiene -----------------------------------------------------
run "git diff --check" git diff --check HEAD

# --- Secrets ----------------------------------------------------------------
if have gitleaks; then
  run "gitleaks (history)" gitleaks git --redact --no-banner .
  run "gitleaks (staged)" gitleaks git --pre-commit --staged --redact --no-banner .
  run "gitleaks (unstaged)" gitleaks git --pre-commit --redact --no-banner .
else
  skip "gitleaks" "gitleaks not installed"
fi

# --- Vendored code integrity -------------------------------------------------
verify_manifests() { # every file listed in third_party/*.sha256 is unchanged
  local m status=0
  cd third_party || return 1
  for m in *.sha256; do
    if have sha256sum; then sha256sum --quiet -c "$m" || status=1
    else shasum -a 256 --quiet -c "$m" || status=1; fi
  done
  cd "$ROOT" || return 1
  return $status
}
if ls third_party/*.sha256 >/dev/null 2>&1; then
  run "third_party integrity" verify_manifests
else
  skip "third_party integrity" "no vendored manifests yet"
fi

# --- Shell scripts ----------------------------------------------------------
# Vendored scripts in third_party/ are not ours and are never edited.
SHELL_FILES="$(git ls-files --cached --others --exclude-standard -- '*.sh' '.githooks/*' ':(exclude)third_party')"
if [ -z "$SHELL_FILES" ]; then
  skip "shellcheck" "no shell scripts"
elif have shellcheck; then
  # shellcheck disable=SC2086
  run "shellcheck" shellcheck $SHELL_FILES
else
  skip "shellcheck" "shellcheck not installed"
fi

# --- Rust -------------------------------------------------------------------
if [ ! -f Cargo.toml ]; then
  for c in "cargo fmt" "cargo clippy" "cargo test" "cargo deny" "cargo audit"; do
    skip "$c" "no Cargo workspace yet"
  done
elif ! have cargo; then
  for c in "cargo fmt" "cargo clippy" "cargo test" "cargo deny" "cargo audit"; do
    skip "$c" "cargo not installed"
  done
else
  run "cargo fmt" cargo fmt --all --check
  run "cargo clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
  run "cargo test" cargo test --workspace --locked
  if cargo deny --version >/dev/null 2>&1 && [ -f deny.toml ]; then
    run "cargo deny" cargo deny check
  else
    skip "cargo deny" "cargo-deny or deny.toml missing"
  fi
  if cargo audit --version >/dev/null 2>&1 && [ -f Cargo.lock ]; then
    run "cargo audit" cargo audit --deny warnings
  else
    skip "cargo audit" "cargo-audit or Cargo.lock missing"
  fi
fi

# --- C++ / Qt ---------------------------------------------------------------
if has_files 'ui/*.cpp' 'ui/*.h'; then
  if have clang-format; then
    # shellcheck disable=SC2046
    run "clang-format" sh -c 'git ls-files --cached --others --exclude-standard -- "ui/*.cpp" "ui/*.h" | xargs clang-format --dry-run --Werror'
  else
    skip "clang-format" "clang-format not installed"
  fi
else
  skip "clang-format" "no C++ sources yet"
fi

if [ -f CMakeLists.txt ]; then
  if have cmake; then
    BUILD_DIR="build/check"
    # Optional compiler cache: used only when ccache is installed. Its directory
    # is outside the repository. Hits need the same source path (CCACHE_BASEDIR).
    cache_args=()
    if have ccache; then
      export CCACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/ferny-check/ccache" CCACHE_BASEDIR="$ROOT"
      cache_args=(-DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache)
    fi
    if run "cmake configure" cmake -S . -B "$BUILD_DIR" "${cache_args[@]}" -DCMAKE_BUILD_TYPE=Debug -DFERNY_WARNINGS_AS_ERRORS=ON; then
      if run "cmake build" cmake --build "$BUILD_DIR" --parallel && { [ -x "$BUILD_DIR/ui/ferny" ] || [ -x "$BUILD_DIR/ui/Debug/ferny.exe" ]; }; then
        # -C Debug is required by multi-config generators (Visual Studio on Windows); single-config ones ignore it.
        # FIXTURES_SETUP/REQUIRED keep the order of dependent tests in parallel runs.
        run "ctest" ctest --test-dir "$BUILD_DIR" -C Debug -j "${FERNY_CTEST_JOBS:-$(cpus)}" --output-on-failure
      else
        skip "ctest" "build failed; UI tests were not run"
      fi
    else
      skip "cmake build" "configure failed"
      skip "ctest" "configure failed; UI tests were not run"
    fi
  else
    skip "cmake build" "cmake not installed"
  fi
else
  skip "cmake build" "no CMakeLists.txt yet"
  skip "ctest" "no CMakeLists.txt yet"
fi

# --- SAST ---------------------------------------------------------------------
if has_files '*.rs' '*.cpp' '*.h'; then
  if have semgrep; then
    # Registry rules are downloaded from semgrep.dev; metrics are disabled.
    run "semgrep" semgrep scan --metrics=off --error --quiet \
      --config p/rust --config p/c --exclude third_party --exclude target --exclude build
  else
    skip "semgrep" "semgrep not installed"
  fi
else
  skip "semgrep" "no Rust or C++ sources yet"
fi

# --- Fuzzing (short run) ------------------------------------------------------
if [ -d fuzz ]; then
  if cargo fuzz --version >/dev/null 2>&1; then
    # Targets are independent: build once, then run up to FERNY_FUZZ_JOBS of them
    # at a time for FERNY_FUZZ_SECONDS each (60 s per target by default).
    # cargo-fuzz needs nightly (sanitizers); the product builds on the pinned stable.
    FUZZ_SECONDS="${FERNY_FUZZ_SECONDS:-60}"
    FUZZ_JOBS="${FERNY_FUZZ_JOBS:-$(cpus)}"
    FUZZ_TARGETS="$(cargo fuzz list 2>/dev/null)"
    if [ -z "$FUZZ_TARGETS" ]; then
      skip "cargo fuzz" "no fuzz targets yet"
    elif run "fuzz build" cargo +nightly fuzz build; then
      FUZZ_OUT="$(mktemp -d "${TMPDIR:-/tmp}/ferny-fuzz.XXXXXX")"
      # The single-quoted script is meant to expand in the child shell, not here.
      # shellcheck disable=SC2016
      printf '%s\n' "$FUZZ_TARGETS" | xargs -P "$FUZZ_JOBS" -I{} sh -c \
        'if cargo +nightly fuzz run "$1" -- -max_total_time="$2" >"$3/$1.log" 2>&1; then echo ok >"$3/$1.status"; else echo failed >"$3/$1.status"; fi' \
        _ {} "$FUZZ_SECONDS" "$FUZZ_OUT"
      for target in $FUZZ_TARGETS; do
        printf '  %-26s ' "fuzz $target (${FUZZ_SECONDS} s)"
        if [ "$(cat "$FUZZ_OUT/$target.status" 2>/dev/null)" = ok ]; then
          echo "ok"; pass "fuzz $target"
        else
          echo "FAILED"; tail -n 40 "$FUZZ_OUT/$target.log" 2>/dev/null | sed 's/^/      /'
          fail "fuzz $target" "see output above"
        fi
      done
      rm -rf "$FUZZ_OUT"
    fi
  else
    skip "cargo fuzz" "cargo-fuzz not installed"
  fi
else
  skip "cargo fuzz" "no fuzz targets yet"
fi

# --- Summary --------------------------------------------------------------------
version_of() { # version_of <tool> <args...>: first line of the version, or "not installed"
  local tool="$1"; shift
  if have "$tool"; then "$tool" "$@" 2>/dev/null | head -n 1; else echo "not installed"; fi
}

COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo none)"
DIRTY=""; [ -n "$(git status --porcelain)" ] && DIRTY=" (uncommitted changes)"
OS_NAME="$(uname -s)"
if [ "$OS_NAME" = "Darwin" ]; then
  OS_DESC="macOS $(sw_vers -productVersion 2>/dev/null)"
elif [ -r /etc/os-release ]; then
  # shellcheck source=/dev/null
  OS_DESC="$(. /etc/os-release && echo "${PRETTY_NAME:-Linux}")"
else
  OS_DESC="$OS_NAME"
fi

echo
echo "----- BEGIN FERNY CHECK SUMMARY -----"
echo "Date:     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "System:   $OS_DESC, $(uname -m)"
echo "Branch:   $(git branch --show-current 2>/dev/null)"
echo "Commit:   $COMMIT$DIRTY"
echo "Mode:     $([ "$STRICT" -eq 1 ] && echo strict || echo normal)"
echo
echo "Tools:"
echo "  git        $(version_of git --version)"
echo "  gitleaks   $(version_of gitleaks version)"
echo "  cargo      $(version_of cargo --version)"
echo "  cmake      $(version_of cmake --version)"
echo "  semgrep    $(version_of semgrep --version)"
if have shellcheck; then
  echo "  shellcheck $(shellcheck --version | grep -E '^version')"
else
  echo "  shellcheck not installed"
fi
echo
echo "Results:"
for r in "${RESULTS[@]}"; do
  status="${r%%|*}"; rest="${r#*|}"; name="${rest%%|*}"; detail="${rest#*|}"
  if [ -n "$detail" ]; then
    printf '  %-4s  %-24s %s\n' "$status" "$name" "($detail)"
  else
    printf '  %-4s  %s\n' "$status" "$name"
  fi
done
echo
echo "Totals:   $(( ${#RESULTS[@]} - FAILED - SKIPPED )) passed, $FAILED failed, $SKIPPED skipped (not verified)"

VERDICT="PASS"
if [ "$FAILED" -gt 0 ]; then
  VERDICT="FAIL"
elif [ "$STRICT" -eq 1 ] && [ "$SKIPPED" -gt 0 ]; then
  VERDICT="FAIL (strict: skipped checks are not allowed)"
fi
echo "Verdict:  $VERDICT"
echo "----- END FERNY CHECK SUMMARY -----"

[ "$VERDICT" = "PASS" ]
