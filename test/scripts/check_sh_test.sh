#!/usr/bin/env bash
# Negative tests for scripts/check.sh: the gate must fail closed.
# Runs in a throwaway Git repository with synthetic data only.
# Usage: bash test/scripts/check_sh_test.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECK="$ROOT/scripts/check.sh"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/ferny-check-test.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

PASSED=0; FAILED=0
expect() { # expect <name> <expected exit code> <command...>
  local name="$1" want="$2"; shift 2
  local got
  "$@" >"$TMP/out.log" 2>&1; got=$?
  if [ "$got" -eq "$want" ]; then
    echo "ok   $name"; PASSED=$((PASSED + 1))
  else
    echo "FAIL $name (exit $got, expected $want)"; sed 's/^/     /' "$TMP/out.log" | tail -n 20
    FAILED=$((FAILED + 1))
  fi
}
expect_output() { # expect_output <name> <pattern>
  if grep -q -- "$2" "$TMP/out.log"; then
    echo "ok   $1"; PASSED=$((PASSED + 1))
  else
    echo "FAIL $1 (missing: $2)"; FAILED=$((FAILED + 1))
  fi
}

new_repo() {
  rm -rf "$TMP/repo"; mkdir -p "$TMP/repo/scripts"
  cp "$CHECK" "$TMP/repo/scripts/check.sh"
  git -C "$TMP/repo" init -q
  git -C "$TMP/repo" config core.hooksPath /dev/null
  echo "# test" > "$TMP/repo/README.md"
  git -C "$TMP/repo" add -A
  git -C "$TMP/repo" -c user.name=test -c user.email=test@example.invalid commit -qm init
}

# 1. Unknown option is a usage error.
new_repo
expect "unknown option exits 2" 2 bash "$TMP/repo/scripts/check.sh" --bogus

# 2. Clean repository passes in normal mode.
expect "clean repo passes" 0 bash "$TMP/repo/scripts/check.sh"
expect_output "skipped checks are reported as not verified" "skipped (not verified)"

# 3. Strict mode fails while checks are skipped.
expect "strict mode fails with skipped checks" 1 bash "$TMP/repo/scripts/check.sh" --strict

# 4. Trailing whitespace makes git diff --check fail.
new_repo
printf 'trailing space \n' >> "$TMP/repo/README.md"
expect "whitespace error fails the gate" 1 bash "$TMP/repo/scripts/check.sh"
expect_output "whitespace failure is named" "FAIL  git diff --check"

# 5. A modified vendored file fails the integrity check.
new_repo
mkdir -p "$TMP/repo/third_party/demo"
echo "original" > "$TMP/repo/third_party/demo/file.txt"
(cd "$TMP/repo/third_party" && shasum -a 256 demo/file.txt > demo.sha256)
git -C "$TMP/repo" add -A
git -C "$TMP/repo" -c user.name=test -c user.email=test@example.invalid commit -qm vendor
expect "intact vendored code passes" 0 bash "$TMP/repo/scripts/check.sh"
echo "tampered" > "$TMP/repo/third_party/demo/file.txt"
expect "tampered vendored file fails the gate" 1 bash "$TMP/repo/scripts/check.sh"
expect_output "integrity failure is named" "FAIL  third_party integrity"

# 6. A staged synthetic secret makes gitleaks fail (if installed).
if command -v gitleaks >/dev/null 2>&1; then
  new_repo
  token="ghp_$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 36)"
  printf 'github_token = "%s"\n' "$token" > "$TMP/repo/config.txt"
  git -C "$TMP/repo" add config.txt
  expect "staged secret fails the gate" 1 bash "$TMP/repo/scripts/check.sh"
  expect_output "staged secret failure is named" "FAIL  gitleaks (staged)"
  # 7. The same secret left unstaged must fail too.
  git -C "$TMP/repo" reset -q config.txt
  printf 'github_token = "%s"\n' "$token" >> "$TMP/repo/README.md"
  expect "unstaged secret fails the gate" 1 bash "$TMP/repo/scripts/check.sh"
  expect_output "unstaged secret failure is named" "FAIL  gitleaks (unstaged)"
  if grep -q "$token" "$TMP/out.log"; then
    echo "FAIL secret is redacted in output"; FAILED=$((FAILED + 1))
  else
    echo "ok   secret is redacted in output"; PASSED=$((PASSED + 1))
  fi
else
  echo "skip secret tests (gitleaks not installed — not verified)"
fi

# 8. If CMake configures but the build fails, CTest must be marked skipped,
# not run against a stale or missing UI executable.
if command -v cmake >/dev/null 2>&1 && command -v ctest >/dev/null 2>&1; then
  new_repo
  cat > "$TMP/repo/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.16)
project(FailingBuild LANGUAGES NONE)
enable_testing()
add_custom_target(always_fails ALL
  COMMAND "${CMAKE_COMMAND}" -E false
  VERBATIM)
EOF
  git -C "$TMP/repo" add CMakeLists.txt
  git -C "$TMP/repo" -c user.name=test -c user.email=test@example.invalid commit -qm cmake-fixture
  expect "failed CMake build blocks CTest and gate" 1 bash "$TMP/repo/scripts/check.sh"
  expect_output "CTest is skipped after build failure" "SKIP.*ctest"
  expect_output "build failure remains in summary" "FAIL  cmake build"
fi

echo
echo "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
