# steel_command (VS Code extension)

Syntax highlighting and snippets for Steel `steelconf` files.

## Install (local)

1) Open VS Code.
2) `Extensions: Install from VSIX...` if you package it, or
3) Use `Developer: Install Extension from Location...` and select this folder.

## Supported

- `steelconf` files (extensions: `.steelconf`, `.mff`, and filenames `steelconf`, `steelconfig.mff`)
- Snippets for workspace/profile/tool/bake/run/export blocks
- Basic syntax highlighting for blocks, directives, strings, numbers, comments, and run log sections
- Optional themes: "Steel Command", "Steel Command Dark"

## LSP (intelligent completion)

This extension now ships a Steelconf LSP for fast completions.

Build locally:

- `npm install`
- `npm run compile`

Then install the VSIX or run the extension in VS Code.
