#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
STEEL_BIN="${STEEL_BIN:-steel}"
BAKE_NAME="${1:-build_debug}"

mkdir -p "$ROOT_DIR/target/out"
"$STEEL_BIN" run --root "$ROOT_DIR" --file "$ROOT_DIR/examples/swift/steelconf" --bake "$BAKE_NAME"
