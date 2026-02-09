# Lexique MUF 4 (steelconf)

Mini lexique des mots-cles en usage dans les fichiers `steelconf`/`*.muf`.
Base: implementation dans `src/run_muf.rs` + exemple `steelconf`.

## Structure
- En-tete: `!muf 4`
- Bloc: `[tag nom?] ... ..`
- Directive: `.op arg1 arg2 ...`
- Commentaire: `;; ...` (ligne entiere ou fin de ligne)

## Blocs top-level
- `workspace` : metadonnees du workspace (valeurs via `.set`).
- `profile <name>` : profil (ex: debug/release) via `.set`.
- `tool <name>` : declare un outil (commande via `.exec`).
- `bake <name>` : recette de build (sources, run, output).
- `export` : expose des recettes (via `.ref`).

## Sous-blocs
- `run <tool>` : etape d'execution dans un `bake`.

## Directives
- `.set <key> <value>` : parametre generic.
- `.exec <cmd>` : commande de l'outil.
- `.make <id> <kind> <pattern>` : sources/inputs (kind: `cglob`, `glob`, `file`, `list`).
- `.needs <bake>` : dependance de recette.
- `.output <port> <path>` : sortie principale.
- `.takes <id> as <flag>` : lie un input a un flag.
- `.emits <port> as <flag>` : lie une sortie a un flag.
- `.include <path>` : include header/chemin.
- `.define <key> [value]` : macro/define.
- `.libdir <path>` : dossier de libs.
- `.lib <name>` : lib a linker.
- `.ref <bake>` : expose une recette (bloc `export`).

## Exemple minimal
```text
!muf 4

[tool cc]
  .exec "cc"
..

[bake app]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-O2" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```
