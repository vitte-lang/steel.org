# Steelconf - Aide de syntaxe rapide

Version : MUF v4.1 (`!muf 4`)

## Blocs principaux

- `[workspace]`
- `[profile <name>]`
- `[tool <name>]`
- `[bake <name>]`
- `[run <tool>]`

## Directives courantes

- `.set <cle> <valeur>`
- `.exec <commande>`
- `.make <id> <kind> <pattern>`
- `.output <port> <path>`
- `.takes <port> as "@args"`
- `.emits <port> as "-o"`
- `.needs <bake>`
- `.ref <bake>` (export)

## Exemple minimal

```text
!muf 4

[workspace]
  .set name "demo"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool cc]
  .exec "cc"
..

[bake app]
  .make src cglob "src/**/*.c"
  [run cc]
    .takes src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```

Voir aussi : `doc/manifest.md`
