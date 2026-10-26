#!/usr/bin/env bash
# Negative tests for scripts/package-macos.sh: it must refuse before
# building. Runs in a throwaway Git repository with synthetic data only.
# Usage: bash test/scripts/package_macos_test.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/scripts/package-macos.sh"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/ferny-package-test.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

# The pattern is a literal line of the script, so single quotes are intended.
# shellcheck disable=SC2016
if grep -q 'cp "\$licenses/SOURCE.txt" "\$stage/SOURCE.txt"' "$SCRIPT"; then
  echo "FAIL DMG must not include SOURCE.txt"
  exit 1
fi
if ! grep -q 'SOURCE.txt must not be included in the DMG' "$SCRIPT"; then
  echo "FAIL DMG must enforce SOURCE.txt exclusion"
  exit 1
fi
echo "ok   DMG excludes SOURCE.txt"

PASSED=0; FAILED=0
expect() { # expect <name> <expected exit code> <pattern> <command...>
  local name="$1" want="$2" pattern="$3"; shift 3
  local got
  "$@" >"$TMP/out.log" 2>&1; got=$?
  if [ "$got" -eq "$want" ] && grep -q -- "$pattern" "$TMP/out.log"; then
    echo "ok   $name"; PASSED=$((PASSED + 1))
  else
    echo "FAIL $name (exit $got, expected $want and '$pattern')"
    sed 's/^/     /' "$TMP/out.log" | tail -n 10
    FAILED=$((FAILED + 1))
  fi
}

repo="$TMP/repo"
mkdir -p "$repo/scripts"
cp "$SCRIPT" "$repo/scripts/package-macos.sh"
printf '[workspace.package]\nversion = "1.2.3"\n' >"$repo/Cargo.toml"
printf '[toolchain]\nchannel = "1.99.0"\n' >"$repo/rust-toolchain.toml"
git -C "$repo" init -q
git -C "$repo" config core.hooksPath /dev/null
git -C "$repo" add -A
git -C "$repo" -c user.name=test -c user.email=test@example.invalid commit -qm init
git -C "$repo" tag v9.9.9
git -C "$repo" tag v1.2.3

run() { (cd "$repo" && bash scripts/package-macos.sh "$@"); }

expect "unknown option" 2 "usage" run --bogus
expect "missing option value" 2 "usage" run --ref
expect "unknown ref" 1 "unknown ref" run --ref does-not-exist
expect "tag must match version" 1 "does not match version" run --ref v9.9.9

if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
  mkdir -p "$TMP/out"
  touch "$TMP/out/ferny-v1.2.3-macos-arm64.dmg"
  expect "existing artefact is kept" 1 "never overwritten" run --ref v1.2.3 --out "$TMP/out"
  rm "$TMP/out/ferny-v1.2.3-macos-arm64.dmg"
  touch "$TMP/out/ferny-v1.2.3-macos-arm64.dmg.sha256"
  expect "existing checksum is kept" 1 "never overwritten" run --ref v1.2.3 --out "$TMP/out"
  if [ -e "$TMP/out/ferny-v1.2.3-macos-arm64.dmg.sha256" ]; then
    echo "ok   checksum still there"; PASSED=$((PASSED + 1))
  else
    echo "FAIL checksum removed"; FAILED=$((FAILED + 1))
  fi
  touch "$TMP/out/ferny-v1.2.3-macos-arm64.cdx.json"
  rm "$TMP/out/ferny-v1.2.3-macos-arm64.dmg.sha256"
  expect "existing SBOM is kept" 1 "never overwritten" run --ref v1.2.3 --out "$TMP/out"
  rm "$TMP/out/ferny-v1.2.3-macos-arm64.cdx.json"
  # Release signing (H4.3) is checked before anything is built.
  expect "notarisation needs an identity" 1 "needs FERNY_SIGN_IDENTITY" \
    env FERNY_NOTARY_PROFILE=profile bash "$repo/scripts/package-macos.sh" --ref v1.2.3 --out "$TMP/out"
  expect "minisign key must exist" 1 "is not a file" \
    env FERNY_MINISIGN_KEY="$TMP/missing.key" bash "$repo/scripts/package-macos.sh" --ref v1.2.3 --out "$TMP/out"
  # A rustc other than the pinned one is refused before the cache is touched.
  mkdir -p "$TMP/fakebin"
  printf '#!/bin/sh\necho "rustc 1.0.0 (test)"\n' >"$TMP/fakebin/rustc"
  chmod +x "$TMP/fakebin/rustc"
  expect "pinned toolchain is required" 1 "pins 1.99.0" \
    env PATH="$TMP/fakebin:$PATH" bash "$repo/scripts/package-macos.sh" --ref v1.2.3 --out "$TMP/out"
else
  expect "other systems refused" 1 "only" run --ref v1.2.3 --out "$TMP/out"
fi

echo "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
