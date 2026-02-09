# Steelconf editor setup (terminal editors)

Steel installs these editor settings on first run. To skip it, set
`STEEL_NO_EDITOR_SETUP=1`. If a config file already exists (for example
`languages.toml` or `settings.json`), Steel will not overwrite it and will leave
an `editor-setup.todo` note under `~/.config/steel/`.

You can re-run the installer at any time:
```
steel editor-setup
```

This document shows the exact files written.

These snippets add a basic filetype and 2-space indentation for `steelconf` and
`*.muf` files. Tabs are converted to spaces.

## steecleditor config
Create `~/.config/steel/steecleditor.conf` (or `XDG_CONFIG_HOME/steel/steecleditor.conf`) to override defaults:
```
# Autosave interval in seconds (0 disables autosave)
autosave_interval=30

# Theme: dark or light
theme=dark
```
You can toggle theme in the editor with `Ctrl+M`.

## Vim / Neovim
Add to `~/.vimrc` or `~/.config/nvim/init.vim`:
```
augroup steelconf_ft
  autocmd!
  autocmd BufRead,BufNewFile steelconf,*.muf setfiletype steelconf
augroup END
```
Then add to `~/.vim/ftplugin/steelconf.vim` or `~/.config/nvim/ftplugin/steelconf.vim`:
```
setlocal tabstop=2
setlocal shiftwidth=2
setlocal expandtab
setlocal softtabstop=2
```

## Helix
Add to `~/.config/helix/languages.toml`:
```
[[language]]
name = "steelconf"
file-types = ["steelconf", "muf"]
indent = { tab-width = 2, unit = "  " }
```

## micro
Create `~/.config/micro/ftdetect/steelconf.yaml`:
```
filetype: steelconf
detect:
  filename: steelconf
  extension: muf
```
Then create `~/.config/micro/ftplugin/steelconf.lua`:
```
local config = import("micro/config")
config.MakeLocalOption("tabsize", 2)
config.MakeLocalOption("tabstospaces", true)
```

## nano
Add to `~/.nanorc` (global settings):
```
set tabsize 2
set tabstospaces
```

## Emacs
Add to your init file (`~/.emacs` or `~/.emacs.d/init.el`):
```
(add-to-list 'auto-mode-alist '("steelconf\\'" . fundamental-mode))
(add-to-list 'auto-mode-alist '("\\.muf\\'" . fundamental-mode))
(add-hook 'fundamental-mode-hook
          (lambda ()
            (when (or (string-equal (file-name-nondirectory (or (buffer-file-name) "")) "steelconf")
                      (and (buffer-file-name) (string-match "\\.muf\\'" (buffer-file-name))))
              (setq-local indent-tabs-mode nil)
              (setq-local tab-width 2)
              (setq-local standard-indent 2))))
```

## Zed
Add to `~/.config/zed/settings.json`:
```
{
  "languages": {
    "Plain Text": {
      "file_types": ["steelconf", "muf"],
      "tab_size": 2,
      "indent_width": 2,
      "soft_wrap": "none"
    }
  }
}
```

## Sublime Text
Create `~/.config/sublime-text/Packages/User/steelconf.sublime-settings`:
```
{
  "tab_size": 2,
  "translate_tabs_to_spaces": true
}
```
Create `~/.config/sublime-text/Packages/User/steelconf.sublime-syntax`:
```
%YAML 1.2
---
name: steelconf
file_extensions:
  - steelconf
  - muf
scope: text.steelconf
contexts:
  main: []
```

## JetBrains IDEs
1) Settings -> Editor -> File Types -> Text -> add `steelconf` and `*.muf`.
2) Settings -> Editor -> Code Style -> Plain Text -> set Tab size = 2, Indent = 2, Tabs = Spaces.
