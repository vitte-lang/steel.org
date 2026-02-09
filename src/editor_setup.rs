use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn ensure_editor_setup() -> io::Result<()> {
    if std::env::var_os("STEEL_NO_EDITOR_SETUP").is_some() {
        return Ok(());
    }

    let config_root = match config_root() {
        Some(root) => root,
        None => return Ok(()),
    };

    let steel_dir = config_root.join("steel");
    fs::create_dir_all(&steel_dir)?;
    let marker_path = steel_dir.join("editor-setup.done");
    if marker_path.exists() {
        return Ok(());
    }

    let mut skipped = Vec::new();

    if let Some(home) = home_dir() {
        setup_vim(&home)?;
        setup_neovim(&config_root)?;
        setup_micro(&config_root)?;
        setup_nano(&home)?;
        setup_emacs(&home)?;
    }

    if setup_helix(&config_root).is_err() {
        skipped.push("helix");
    }
    if setup_zed(&config_root).is_err() {
        skipped.push("zed");
    }
    if setup_sublime(&config_root).is_err() {
        skipped.push("sublime");
    }

    if !skipped.is_empty() {
        let todo = steel_dir.join("editor-setup.todo");
        let mut f = fs::OpenOptions::new().create(true).append(true).open(todo)?;
        writeln!(
            f,
            "Skipped editor setup due to existing config: {}",
            skipped.join(", ")
        )?;
    }

    fs::write(marker_path, "ok\n")?;
    Ok(())
}

fn config_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join(".config"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata));
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn write_if_missing(path: &Path, content: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn append_if_missing(path: &Path, marker: &str, content: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.contains(marker) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    if !existing.ends_with('\n') && !existing.is_empty() {
        writeln!(f)?;
    }
    writeln!(f, "{marker}")?;
    writeln!(f, "{content}")?;
    Ok(())
}

fn setup_vim(home: &Path) -> io::Result<()> {
    let ftdetect = home.join(".vim/ftdetect/steelconf.vim");
    let ftplugin = home.join(".vim/ftplugin/steelconf.vim");
    let syntax = home.join(".vim/syntax/steelconf.vim");
    write_if_missing(
        &ftdetect,
        "augroup steelconf_ft\n  autocmd!\n  autocmd BufRead,BufNewFile steelconf,*.muf setfiletype steelconf\naugroup END\n",
    )?;
    write_if_missing(
        &ftplugin,
        "setlocal tabstop=2\nsetlocal shiftwidth=2\nsetlocal expandtab\nsetlocal softtabstop=2\ninoremap <buffer> [ []<Left>\n",
    )?;
    write_if_missing(
        &syntax,
        "if exists(\"b:current_syntax\")\n  finish\nendif\n\nsyntax match steelconfComment /^\\s*;;.*$/\nsyntax match steelconfHeader /^\\s*!muf\\s\\+4\\s*$/\nsyntax match steelconfHeaderBad /^\\s*!muf\\s\\+\\d\\+\\s*$/\nsyntax match steelconfBlock /^\\s*\\[.*\\]\\s*$/\nsyntax match steelconfDirective /^\\s*\\.[a-zA-Z_][a-zA-Z0-9_-]*/\nsyntax match steelconfNumber /\\v[-+]?\\d+(\\.\\d+)?/\nsyntax match steelconfBool /\\v\\<(true|false)\\>/\nsyntax match steelconfWorkspace /\\v\\<workspace\\>/\nsyntax match steelconfProfile /\\v\\<profile\\>/\nsyntax match steelconfTool /\\v\\<tool\\>/\nsyntax match steelconfBake /\\v\\<bake\\>/\nsyntax match steelconfRun /\\v\\<run\\>/\nsyntax match steelconfExport /\\v\\<(export|exports)\\>/\nsyntax match steelconfStore /\\v\\<store\\>/\nsyntax match steelconfCapsule /\\v\\<capsule\\>/\nsyntax match steelconfPlan /\\v\\<plan\\>/\nsyntax match steelconfSwitch /\\v\\<switch\\>/\nsyntax region steelconfString start=/\"/ end=/\"/\n\nhighlight default link steelconfComment Comment\nhighlight default link steelconfHeader Type\nhighlight default link steelconfHeaderBad Error\nhighlight default link steelconfBlock Keyword\nhighlight default link steelconfDirective Statement\nhighlight default link steelconfString String\nhighlight default link steelconfNumber Number\nhighlight default link steelconfBool Boolean\nhighlight default link steelconfWorkspace Keyword\nhighlight default link steelconfProfile Type\nhighlight default link steelconfTool Statement\nhighlight default link steelconfBake Function\nhighlight default link steelconfRun WarningMsg\nhighlight default link steelconfExport Constant\nhighlight default link steelconfStore Identifier\nhighlight default link steelconfCapsule PreProc\nhighlight default link steelconfPlan String\nhighlight default link steelconfSwitch Special\n\nlet b:current_syntax = \"steelconf\"\n",
    )?;
    Ok(())
}

fn setup_neovim(config_root: &Path) -> io::Result<()> {
    let base = config_root.join("nvim");
    let ftdetect = base.join("ftdetect/steelconf.vim");
    let ftplugin = base.join("ftplugin/steelconf.vim");
    let syntax = base.join("syntax/steelconf.vim");
    write_if_missing(
        &ftdetect,
        "augroup steelconf_ft\n  autocmd!\n  autocmd BufRead,BufNewFile steelconf,*.muf setfiletype steelconf\naugroup END\n",
    )?;
    write_if_missing(
        &ftplugin,
        "setlocal tabstop=2\nsetlocal shiftwidth=2\nsetlocal expandtab\nsetlocal softtabstop=2\ninoremap <buffer> [ []<Left>\n",
    )?;
    write_if_missing(
        &syntax,
        "if exists(\"b:current_syntax\")\n  finish\nendif\n\nsyntax match steelconfComment /^\\s*;;.*$/\nsyntax match steelconfHeader /^\\s*!muf\\s\\+4\\s*$/\nsyntax match steelconfHeaderBad /^\\s*!muf\\s\\+\\d\\+\\s*$/\nsyntax match steelconfBlock /^\\s*\\[.*\\]\\s*$/\nsyntax match steelconfDirective /^\\s*\\.[a-zA-Z_][a-zA-Z0-9_-]*/\nsyntax match steelconfNumber /\\v[-+]?\\d+(\\.\\d+)?/\nsyntax match steelconfBool /\\v\\<(true|false)\\>/\nsyntax match steelconfWorkspace /\\v\\<workspace\\>/\nsyntax match steelconfProfile /\\v\\<profile\\>/\nsyntax match steelconfTool /\\v\\<tool\\>/\nsyntax match steelconfBake /\\v\\<bake\\>/\nsyntax match steelconfRun /\\v\\<run\\>/\nsyntax match steelconfExport /\\v\\<(export|exports)\\>/\nsyntax match steelconfStore /\\v\\<store\\>/\nsyntax match steelconfCapsule /\\v\\<capsule\\>/\nsyntax match steelconfPlan /\\v\\<plan\\>/\nsyntax match steelconfSwitch /\\v\\<switch\\>/\nsyntax region steelconfString start=/\"/ end=/\"/\n\nhighlight default link steelconfComment Comment\nhighlight default link steelconfHeader Type\nhighlight default link steelconfHeaderBad Error\nhighlight default link steelconfBlock Keyword\nhighlight default link steelconfDirective Statement\nhighlight default link steelconfString String\nhighlight default link steelconfNumber Number\nhighlight default link steelconfBool Boolean\nhighlight default link steelconfWorkspace Keyword\nhighlight default link steelconfProfile Type\nhighlight default link steelconfTool Statement\nhighlight default link steelconfBake Function\nhighlight default link steelconfRun WarningMsg\nhighlight default link steelconfExport Constant\nhighlight default link steelconfStore Identifier\nhighlight default link steelconfCapsule PreProc\nhighlight default link steelconfPlan String\nhighlight default link steelconfSwitch Special\n\nlet b:current_syntax = \"steelconf\"\n",
    )?;
    Ok(())
}

fn setup_helix(config_root: &Path) -> io::Result<()> {
    let path = config_root.join("helix/languages.toml");
    let query = config_root.join("helix/runtime/queries/steelconf/highlights.scm");
    let block = "\n[[language]]\nname = \"steelconf\"\nfile-types = [\"steelconf\", \"muf\"]\nindent = { tab-width = 2, unit = \"  \" }\n";
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing.contains("name = \"steelconf\"") {
        write_if_missing(&query, helix_highlights())?;
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    if !existing.ends_with('\n') && !existing.is_empty() {
        writeln!(f)?;
    }
    write!(f, "{block}")?;
    write_if_missing(&query, helix_highlights())?;
    Ok(())
}

fn setup_micro(config_root: &Path) -> io::Result<()> {
    let ftdetect = config_root.join("micro/ftdetect/steelconf.yaml");
    let ftplugin = config_root.join("micro/ftplugin/steelconf.lua");
    let syntax = config_root.join("micro/syntax/steelconf.yaml");
    write_if_missing(
        &ftdetect,
        "filetype: steelconf\ndetect:\n  filename: steelconf\n  extension: muf\n",
    )?;
    write_if_missing(
        &ftplugin,
        "local config = import(\"micro/config\")\nconfig.MakeLocalOption(\"tabsize\", 2)\nconfig.MakeLocalOption(\"tabstospaces\", true)\nconfig.MakeLocalOption(\"autoclose\", true)\n",
    )?;
    write_if_missing(
        &syntax,
        "filetype: steelconf\nrules:\n  - \";;.*$\": \"comment\"\n  - \"!muf[[:space:]]+4\": \"type\"\n  - \"!muf[[:space:]]+[0-9]+\": \"error\"\n  - \"\\\\[[^\\\\]]+\\\\]\": \"keyword\"\n  - \"\\\\.[a-zA-Z_][a-zA-Z0-9_-]*\": \"statement\"\n  - \"\\\\b(true|false)\\\\b\": \"constant.bool\"\n  - \"-?[0-9]+(\\\\.[0-9]+)?\": \"constant.number\"\n  - \"\\\\b(workspace|profile|tool|bake|run|export|exports|store|capsule|plan|switch)\\\\b\": \"keyword\"\n  - \"\\\".*?\\\"\": \"string\"\n",
    )?;
    Ok(())
}

fn setup_nano(home: &Path) -> io::Result<()> {
    let nanorc = home.join(".nanorc");
    let syntax = home.join(".config/steel/steelconf.nanorc");
    append_if_missing(
        &nanorc,
        "# steelconf (steel)",
        "set tabsize 2\nset tabstospaces\ninclude \"~/.config/steel/steelconf.nanorc\"",
    )
    .and_then(|_| {
        write_if_missing(
            &syntax,
            "syntax \"steelconf\" \"steelconf\" \"\\\\.muf$\"\ncolor brightblack \"^\\\\s*;;.*$\"\ncolor brightcyan \"^\\\\s*!muf\\\\s+4\\\\s*$\"\ncolor brightred \"^\\\\s*!muf\\\\s+[0-9]+\\\\s*$\"\ncolor cyan \"\\\\[[^\\\\]]+\\\\]\"\ncolor yellow \"^\\\\s*\\\\.[a-zA-Z_][a-zA-Z0-9_-]*\"\ncolor magenta \"\\\\b(true|false)\\\\b\"\ncolor brightyellow \"-?[0-9]+(\\\\.[0-9]+)?\"\ncolor brightblue \"\\\\b(workspace|profile|tool|bake|run|export|exports|store|capsule|plan|switch)\\\\b\"\ncolor green \"\\\"[^\"]*\\\"\"\n",
        )
    })
}

fn setup_emacs(home: &Path) -> io::Result<()> {
    let emacs = home.join(".emacs");
    let init_el = home.join(".emacs.d/init.el");
    let target = if emacs.exists() || !init_el.exists() {
        emacs
    } else {
        init_el
    };
    append_if_missing(
        &target,
        ";; steelconf (steel)",
        "(add-to-list 'auto-mode-alist '(\"steelconf\\\\'\" . fundamental-mode))\n(add-to-list 'auto-mode-alist '(\"\\\\.muf\\\\'\" . fundamental-mode))\n(add-hook 'fundamental-mode-hook\n          (lambda ()\n            (when (or (string-equal (file-name-nondirectory (or (buffer-file-name) \"\")) \"steelconf\")\n                      (and (buffer-file-name) (string-match \"\\\\.muf\\\\'\" (buffer-file-name))))\n              (setq-local indent-tabs-mode nil)\n              (setq-local tab-width 2)\n              (setq-local standard-indent 2)\n              (when (fboundp 'electric-pair-local-mode) (electric-pair-local-mode 1))\n              (setq-local font-lock-defaults\n                          '((\n                             (\"^\\\\s-*;;.*$\" . font-lock-comment-face)\n                             (\"^\\\\s-*!muf\\\\s-+4\\\\s-*$\" . font-lock-type-face)\n                             (\"^\\\\s-*!muf\\\\s-+[0-9]+\\\\s-*$\" . font-lock-warning-face)\n                             (\"^\\\\s-*\\\\[[^\\\\]]+\\\\]\" . font-lock-keyword-face)\n                             (\"^\\\\s-*\\\\.[a-zA-Z_][a-zA-Z0-9_-]*\" . font-lock-builtin-face)\n                             (\"\\\\b(true|false)\\\\b\" . font-lock-constant-face)\n                             (\"-?[0-9]+(\\\\.[0-9]+)?\" . font-lock-constant-face)\n                             (\"\\\\b(workspace|profile|tool|bake|run|export|exports|store|capsule|plan|switch)\\\\b\" . font-lock-keyword-face)\n                             (\"\\\"[^\"]*\\\"\" . font-lock-string-face)\n                           ))))))",
    )
}

fn setup_zed(config_root: &Path) -> io::Result<()> {
    let path = config_root.join("zed/settings.json");
    if path.exists() {
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if existing.contains("steelconf") {
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "settings.json exists",
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = "{\n  \"languages\": {\n    \"Plain Text\": {\n      \"file_types\": [\"steelconf\", \"muf\"],\n      \"tab_size\": 2,\n      \"indent_width\": 2,\n      \"soft_wrap\": \"none\"\n    }\n  }\n}\n";
    fs::write(path, content)?;
    Ok(())
}

fn setup_sublime(config_root: &Path) -> io::Result<()> {
    let base = config_root.join("sublime-text/Packages/User");
    let settings = base.join("steelconf.sublime-settings");
    let syntax = base.join("steelconf.sublime-syntax");
    write_if_missing(
        &settings,
        "{\n  \"tab_size\": 2,\n  \"translate_tabs_to_spaces\": true\n}\n",
    )?;
    write_if_missing(
        &syntax,
        "%YAML 1.2\n---\nname: steelconf\nfile_extensions:\n  - steelconf\n  - muf\nscope: text.steelconf\ncontexts:\n  main: []\n",
    )?;
    Ok(())
}

fn helix_highlights() -> &'static str {
    "; Placeholder: helix highlighting requires a tree-sitter grammar.\n"
}
