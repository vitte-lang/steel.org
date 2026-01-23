# Steel Version 2.2026

## Unreleased
- Added a repo-wide `.editorconfig` so `steelconf`/`*.muf` always use 2-space indents.
- Added an editor setup guide and auto-setup on first run (opt-out with `STEEL_NO_EDITOR_SETUP`).
- Added `steel editor-setup` and `steel editor` commands.
- Added the `steecleditor` TUI (open/save/search, nano shortcuts, help, syntax highlight).
- Added terminal-editor syntax files (Vim/Neovim, micro, nano, Emacs) with `!muf 4` emphasis.
- Added VS Code extension link on the Downloads page.
- Reworked Downloads to use a single GitHub Releases link for all platforms.
- Expanded site examples with longer, copy-ready `steelconf` blocks per language.
- Humanized site copy (leads, cards, guides) across locales.
- Added `examples/README.md` with copy/run snippets by language.
- Updated man page (`doc/steel.1`) with editor commands.
- Simplified README + manifest wording (tech-simple tone).
- Fixed a macOS-only import warning in `src/os.rs`.
