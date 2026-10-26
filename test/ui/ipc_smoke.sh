#!/usr/bin/env bash
# Single-instance smoke test (macOS/Linux): a primary instance listens on a
# private endpoint; a second invocation forwards two files (one with a
# position) and exits; the primary reports what it opened.
#   test/ui/ipc_smoke.sh <ferny binary> <fixtures dir>
set -euo pipefail

ferny="$1"
fixtures="$2"
# Socket paths must be short: use a private directory under /tmp.
dir="$(mktemp -d /tmp/fy-ipc.XXXXXX)"
trap 'rm -rf "$dir"' EXIT
chmod 700 "$dir"
export QT_QPA_PLATFORM=offscreen

"$ferny" --smoke-test --smoke-ipc-wait --ipc-dir "$dir/ep" >"$dir/primary.out" 2>&1 &
primary=$!

for _ in $(seq 1 50); do
  grep -q "smoke: ipc listening" "$dir/primary.out" 2>/dev/null && break
  sleep 0.1
done
grep -q "smoke: ipc listening" "$dir/primary.out" || { cat "$dir/primary.out"; exit 1; }

# The socket must not be reachable by others.
if stat -f '%Lp' "$dir/ep" >/dev/null 2>&1; then
  perms="$(stat -f '%Lp' "$dir/ep")" # macOS/BSD stat
else
  perms="$(stat -c '%a' "$dir/ep")" # GNU stat
fi
[ "$perms" = "700" ] || { echo "endpoint directory mode $perms"; exit 1; }

"$ferny" --ipc-dir "$dir/ep" "$fixtures/hello.rs" "$fixtures/hello.cpp:4:5"
wait "$primary"
cat "$dir/primary.out"
grep -q "smoke: ipc tabs=2 cursor=4:5" "$dir/primary.out"
