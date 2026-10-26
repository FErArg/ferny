#!/usr/bin/env bash
# Build Ferny (Debug) and run it — the "dev" loop for macOS and Linux.
#
#   scripts/dev.sh                 # build and open an empty window
#   scripts/dev.sh path/to/file    # build and open a file
#
# Arguments are passed to the app unchanged. Build tree: build/dev.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
build="$root/build/dev"

if [ ! -f "$build/CMakeCache.txt" ]; then
  generator=()
  if command -v ninja >/dev/null 2>&1; then generator=(-G Ninja); fi
  cmake -S "$root" -B "$build" "${generator[@]}" -DCMAKE_BUILD_TYPE=Debug
fi
cmake --build "$build" --parallel
exec "$build/ui/ferny" "$@"
