# Steel machine-readable schemas

This directory hosts JSON schemas for stable, machine-readable outputs
intended for IDE/CI tooling.

Currently covered:
- `steel.graph.json/1` -> `schemas/steel.graph.json.schema.json`
- `steel.decompile.report` -> `schemas/steel.decompile.report.schema.json`
- `steel.fingerprints.json/1` -> `schemas/steel.fingerprints.json.schema.json`

Notes:
- The MUF language syntax is specified in `assets/grammar/steel.ebnf`.
- The `.mff` text format is specified in `doc/manifest.md` (section "Format steel.log").
