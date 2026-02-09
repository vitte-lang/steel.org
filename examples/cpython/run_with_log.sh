#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
mkdir -p target/logs

steel run \
  --root ./ultra \
  --file steelconf \
  --log target/logs/run_debug.mff \
  --log-mode truncate \
  --all
