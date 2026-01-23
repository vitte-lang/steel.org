# Guide VS Code (debutant, complet)

Ce fichier explique Visual Studio Code (VS Code) comme si tu debutes. Lis-le
dans l'ordre, puis reviens aux sections qui t'interessent.

## 1) C'est quoi VS Code
- Un editeur de code gratuit, leger, et multi-plateforme.
- Tu peux y ouvrir un dossier de projet, editer les fichiers, lancer des
  commandes, et gerer Git.

## 2) Installer VS Code (tous OS)
### A) Methode simple (recommandee)
1. Va sur https://code.visualstudio.com/
2. Telecharge la version pour ton systeme.
3. Installe puis lance VS Code.

### B) Install via package manager (optionnel)
Linux (exemples):
- Ubuntu/Debian: `sudo apt install code`
- Fedora: `sudo dnf install code`
- Arch: `sudo pacman -S code`

macOS (Homebrew):
- `brew install --cask visual-studio-code`

Windows:
- Telecharge le `.exe` sur le site officiel et lance-le.

BSD (exemples):
- FreeBSD: `pkg install vscode`
- OpenBSD: `pkg_add vscode`

Note: les noms de paquets peuvent varier selon la version et le depot.

## 2bis) Detecter ton OS (si tu ne sais pas)
Ouvre un terminal et tape une des commandes ci-dessous:
- Linux/BSD/macOS: `uname -s`
- macOS (plus precise): `sw_vers`
- Windows (cmd): `ver`
- Windows (PowerShell): `$PSVersionTable.OS`

## 3) Ouvrir un projet
1. Clique sur "File" > "Open Folder..."
2. Choisis le dossier du projet (ici, `muffin`).
3. VS Code charge tous les fichiers du projet.

Astuce: si VS Code demande "Trust the authors?", tu peux choisir "Trust" si
le projet vient d'une source fiable (sinon, "Restricted Mode").

## 4) L'interface en 5 zones
1. **Barre laterale gauche**: explorateur de fichiers, recherche, Git, etc.
2. **Zone centrale**: les fichiers ouverts (onglets).
3. **Barre en bas**: statut, branche Git, erreurs, etc.
4. **Palette de commandes**: recherche rapide d'actions.
5. **Terminal integre**: pour taper des commandes sans quitter VS Code.

## 5) Ouvrir, creer, et sauvegarder
- Ouvrir un fichier: clique dans l'explorateur.
- Creer un fichier: clic droit dans l'explorateur > "New File".
- Sauvegarder: `Ctrl+S` (Windows/Linux) ou `Cmd+S` (Mac).

## 6) La palette de commandes (super utile)
Raccourci:
- Windows/Linux: `Ctrl+Shift+P`
- Mac: `Cmd+Shift+P`

Tu peux taper "Format Document", "Open Settings", "Git: Commit", etc.

## 7) Extensions (ajouter des super-pouvoirs)
Ouvre l'onglet "Extensions" (icone de cubes).
Tu peux chercher et installer:
- Un support de langage (ex: Rust, Swift, Python, etc.)
- Un theme (couleurs)
- Un formatteur (mise en forme du code)

Astuce: n'installe pas tout au hasard. Ajoute seulement ce dont tu as besoin.

## 8) Terminal integre
Menu: "Terminal" > "New Terminal"
Tu peux lancer:
- `ls` (liste des fichiers)
- `git status` (etat Git)
- des commandes de build/test propres au projet

## 9) Git de base (si tu utilises Git)
Dans la barre laterale, l'icone "Source Control" te montre:
- les fichiers modifies
- les differences
- un bouton pour committer

Flux simple:
1. Modifie des fichiers
2. Va dans "Source Control"
3. Ecris un message
4. Clique sur "Commit"

## 10) Rechercher dans le projet
Raccourci: `Ctrl+Shift+F` / `Cmd+Shift+F`
Tu peux chercher un mot dans tous les fichiers.

## 11) Parametres utiles (simples)
Ouvre "Settings" via la palette de commandes et cherche:
- "Auto Save": sauvegarde auto
- "Format On Save": formate le code a l'enregistrement
- "Tab Size": taille des tabulations

## 12) Depannage rapide
- Si le code n'est pas colore: installe l'extension du langage.
- Si rien ne marche: ferme et relance VS Code.
- Si un projet est lent: ferme les fichiers inutiles.

## 13) Raccourcis pratiques
- Ouvrir un fichier vite: `Ctrl+P` / `Cmd+P`
- Rechercher dans un fichier: `Ctrl+F` / `Cmd+F`
- Remplacer: `Ctrl+H` / `Cmd+H`
- Commenter une ligne: `Ctrl+/` / `Cmd+/`

---

Si tu veux un guide adapte au projet (ex: commandes precises, extensions
recommandees), dis-moi ce que tu fais avec ce depot et ton systeme.

---

# Syntaxe MUF (Steel) + exemples par langage

Cette section explique la syntaxe de base des fichiers `steelconf`/`*.muf`
et donne un exemple minimal par langage.

## A) Les bases de la syntaxe MUF
- En-tete obligatoire: `!muf 4`
- Blocs: `[tag nom?] ... ..`
- Directives dans un bloc: `.op arg1 arg2 ...`
- Commentaires: `;; ...` (ligne entiere ou fin de ligne)

Exemple minimal:
```text
!muf 4

[workspace]
  .set name "demo"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..
```

## B) Mots-cles !muf 4 (steelconf)
Liste des mots-cles utilises en prod (source: `src/run_muf.rs` + `steelconf`).
Ils sont classes par usage pour aider la lecture.

### 1) Blocs top-level
- `workspace` : metadonnees de workspace (`.set`).
- `profile <name>` : profil (ex: debug/release) avec des `.set`.
- `tool <name>` : outil executable (`.exec`).
- `bake <name>` : recette de build (sources, runs, output).
- `export` : liste de recettes exposees (avec `.ref`).

### 2) Sous-blocs
- `run <tool>` : etape d'execution dans un `bake`.

### 3) Directives (dans un bloc)
- `.set <key> <value>` : assigne un parametre (workspace/profile/run).
- `.exec <cmd>` : commande de l'outil (tool).
- `.make <id> <kind> <pattern>` : sources/inputs (bake). `kind` ex: `cglob`, `glob`, `file`, `list`.
- `.needs <bake>` : dependance de recette (bake).
- `.output <port> <path>` : sortie principale (bake).
- `.takes <id> as <flag>` : lie un input a un flag (run).
- `.emits <port> as <flag>` : lie une sortie a un flag (run).
- `.include <path>` : include header/chemin (run).
- `.define <key> [value]` : define (run).
- `.libdir <path>` : dossier de libs (run).
- `.lib <name>` : lib a linker (run).
- `.ref <bake>` : expose une recette (export).

### 4) Exemple court (recette minimale)
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

## C) Exemples par langage
Chaque exemple definit un `tool` (commande), un `bake` (recette), et un
`output` (fichier genere). Les chemins sont volontairement simples.

### 1) Swift
```text
!muf 4

[tool swiftc]
  .exec "swiftc"
..

[bake build_debug]
  .make swift_src cglob "Sources/**/*.swift"
  [run swiftc]
    .set "-g" 1
    .takes swift_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/swift_app_debug"
..
```

### 2) C (gcc/cc)
```text
!muf 4

[tool cc]
  .exec "cc"
..

[bake c_build]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-O2" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app_c"
..
```

### 3) Rust (cargo)
```text
!muf 4

[tool sh]
  .exec "sh"
..

[bake rust_build]
  .make rust_src cglob "src/**/*.rs"
  [run sh]
    .set "-c" "cargo build"
  ..
  .output exe "target/debug/steel"
..
```

### 4) Python
```text
!muf 4

[tool sh]
  .exec "sh"
..

[bake python_run]
  .make py_src cglob "src/**/*.py"
  [run sh]
    .set "-c" "python3 -u src/main.py"
  ..
  .output exe "target/out/python.run"
..
```

### 5) Java (javac + jar)
```text
!muf 4

[tool javac]
  .exec "javac"
..
[tool jar]
  .exec "jar"
..

[bake java_package]
  .make java_src cglob "src/**/*.java"
  [run javac]
    .set "-d" "target/classes"
    .takes java_src as "@args"
  ..
  [run jar]
    .set "-c" 1
    .set "-f" "target/out/app.jar"
    .set "-e" "com.example.App"
    .set "-C" "target/classes"
    .set "." 1
  ..
  .output jar "target/out/app.jar"
..
```

### 6) OCaml
```text
!muf 4

[tool ocamlc]
  .exec "ocamlc"
..

[bake ocaml_build]
  .make ml_src cglob "src/**/*.ml"
  [run ocamlc]
    .set "-g" 1
    .takes ml_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/ocaml_app.byte"
..
```

## D) steelconf.mff (format MFF v1)
Le fichier `steelconf.mff` est la version resolue/compilee de `steelconf`.
Header: `mff 1`. Le format est deterministe et line-oriented.
Les blocs se ferment avec `.end`.

### 1) Blocs principaux
- `host` : infos machine (os/arch/vendor/abi).
- `paths` : chemins racine (`root`, `dist`, `cache`, `store`).
- `stores` -> `store <name>` : cache resolu (`path`, `mode`, `fp`).
- `capsules` -> `capsule <name>` : policy resolue (env/fs/net/time, `fp`).
- `vars` -> `var <name>` : variables resolues (`type`, `value`, `fp`).
- `tools` -> `tool <name>` : outils resolus (`exec`, `expect_version`, `sandbox`, `capsule`, `fp`).
- `bakes` -> `bake <name>` : recettes resolues (ports, make, run, cache, fp).
- `wires` : connexions `wire from -> to`.
- `exports` : sorties publiques `export ref`.
- `plans` -> `plan <name>` : sequences `run exports` ou `run ref`.
- `switch` : mapping d'options CLI.

### 2) Clefs au top-level (selection)
- `profile = "..."` : profil selectionne.
- `target = "..."` : target selectionne (si defini).
- `plan = "..."` : plan selectionne (si defini).

### 3) Exemple minimal MFF
```text
mff 1

host
  os = "linux"
  arch = "x86_64"
.end

profile = "debug"

paths
  root = "/repo"
  dist = "/repo/dist"
.end

tools
  tool cc
    exec = "cc"
    sandbox = false
  .end
.end
```

## C) Ou sont les vrais exemples dans ce depot
- Swift: `examples/swift/steelconf`
- C (minimal): `steelconf_exemple`
- Multi-langage: `steelconf`

Si tu veux un exemple plus proche de ton projet (structure de dossiers,
options de build, formatage, tests, etc.), dis-moi le langage et l'objectif.
