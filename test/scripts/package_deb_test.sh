#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
script="$root/scripts/package-deb.sh"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/ferny-package-deb-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

if "$script" --unknown >"$tmp/out" 2>"$tmp/err"; then
  echo "expected invalid option to fail" >&2
  exit 1
fi
grep -q '^usage:' "$tmp/err"

if "$script" --ref definitely-not-a-real-ref --out "$tmp" >"$tmp/out" 2>"$tmp/err"; then
  echo "expected unknown ref to fail" >&2
  exit 1
fi
grep -q 'unknown ref' "$tmp/err"

if "$script" --ref HEAD --source-tree "$root" --out "$tmp" >"$tmp/out" 2>"$tmp/err"; then
  echo "expected mutually exclusive source options to fail" >&2
  exit 1
fi
grep -q 'choose either --ref or --source-tree' "$tmp/err"

echo "package_deb_test: PASS"
