<a id="steel-manifest-documentation-complete"></a>
# Steel Manifest (documentation complete)

Ce document est la reference complete du projet. Il couvre la CLI, les formats, la syntaxe MUF, les regles de resolution, les logs et les conventions en usage dans le code.

<a id="1-vision-et-modele-mental"></a>
## 1. Vision et modele mental

Steel est la couche **de configuration declarative** du build. Il:
- lit et valide des fichiers MUF,
- resout les selections (profil/target) et les valeurs derivees,
- emet l artefact canonique **steelconfig.mff**,
- fournit un runner minimal pour executer des bakes declaratifs.

Le pipeline est en deux phases:
1) **Configuration** (steel / build steelconf) -> produit steelconfig.mff.
2) **Execution** (run) -> interprete steelconf pour executer des outils.

Cette separation garantit la reproductibilite: la configuration figee peut etre relue, versionnee et comparee.

<a id="2-concepts-clefs"></a>
## 2. Concepts clefs

- **Workspace**: conteneur global des settings (profil, paths, etc.).
- **Profile**: groupe de valeurs (opt, debug, etc.) appliquees au workspace.
- **Tool**: description declarative d un executable (ex: gcc) via `.exec`.
- **Bake**: unite de travail dans un DAG, definie par des sources, des runs et un output.
- **Run**: etape d execution d un tool, qui transforme des inputs en outputs.
- **MUF**: format declaratif (fichier texte) base sur blocs et directives.
- **MFF**: format resolu (fichier texte stable) emis par `steel` / `build steelconf`.

<a id="steellib-backends"></a>
## 2.1 SteelLib (backends)

Steel expose des backends via la crate SteelLib. Exemple d import OCaml:

```rust
use SteelLib::ocaml::{OcamlArgs, OcamlDriver, OcamlSpec};
```

Backends disponibles (liste rapide):
- C/C++: `gcc` (gcc/clang)
- OCaml: `ocaml` (ocamlc/ocamlopt)
- Python: `cpython` (CPython/PyPy/Nuitka)
- Haskell: `ghc-haskell` (GHC)

<a id="3-fichiers-et-artefacts"></a>
## 3. Fichiers et artefacts

<a id="3-1-entrees"></a>
### 3.1 Entrees

- `steelconf`: fichier de configuration cherche par `steel` / `build steelconf`.
- `steelconf`: fichier MUF attendu par `steel run` si `--file` n est pas fourni (alias: `steelconf`).
- Note migration: `Steelfile` a ete renomme en `steelconf`. Renommer vos fichiers existants si besoin.

<a id="3-2-sorties"></a>
### 3.2 Sorties

- `steelconfig.mff`: artefact resolu emis par `steel` / `build steelconf` (nom par defaut).
- `.steel-cache/`: cache de configuration (check, stats, etc.).
- `target/steel_run_<timestamp>.mff`: log d execution du runner (par defaut).

<a id="3-3-dossiers-conventionnels"></a>
### 3.3 Dossiers conventionnels

- `build/`: chemins de build resolus (phase configuration).
- `dist/`: sorties pour distribution.
- `target/`: sorties du runner et logs.

<a id="cli-complete"></a>
## 4. CLI complete (contrat actuel)

<a id="liste-rapide-commandes"></a>
Liste rapide (toutes les commandes):
- [`steel`](#42-build-steel-configuration) (alias: `build steelconf`) — ex: `steel`
- [`build steelconf`](#42-build-steel-configuration) (alias: `resolve`, `check`, `print`) — ex: `steel build steelconf`
- [`run`](#43-run-execution) — ex: `steel run --file MinConfig.muf --all`
- [`doctor`](#44-doctor) — ex: `steel doctor --json`
- [`cache`](#45-cache) — ex: `steel cache status`
- [`graph`](#46-graph-stub) — ex: `steel graph --dot`
- [`fmt`](#47-fmt-stub) — ex: `steel fmt --file steelconf --check`
- [`version`](#41-aide-et-version) — ex: `steel --version`
- [`help`](#41-aide-et-version) — ex: `steel help`

<a id="flags-frequents"></a>
### Flags frequents (profil/target/emit/log)

Ces flags apparaissent dans plusieurs commandes; ils sont censes rester stables.

<a id="profile-name"></a>
#### `--profile <name>`

- Selection du profil de configuration (ex: `debug`, `release`).
- Defaut: `debug` (ou `MUFFIN_PROFILE` si defini).
- Utilise par: `run`.

Exemples:
```
steel run --profile debug --all
```

<a id="log-path-et-log-mode"></a>
#### `--log <path>` (et `--log-mode`)

- Chemin de log pour l execution `run`.
- Defaut: `target/steel_run_<timestamp>.mff`.
- `--log-mode` accepte `append` ou `truncate`.
- Utilise par: `run`.

Exemples:
```
steel run --log target/run.mff --log-mode truncate --all
```

<a id="cmd-aide-version"></a>
### 4.1 Aide et version

- `steel help` / `steel -h` / `steel --help`
- `steel version` / `steel -V` / `steel --version`

<a id="cmd-build-steel"></a>
### 4.2 build steelconf (configuration)

Commande principale (alias `steel build steelconf`):
```
steel
```

Semantique:
- parse / valide / resolve la configuration,
- emet `steelconfig.mff`,
- aucun parametre: tout est lu depuis `steelconf`.

Aliases:
- `steel resolve` : alias de `build steelconf`.
- `steel check`   : emit dans `.steel-cache/check/` puis supprime (best effort).
- `steel print`   : emit + print.

<a id="cmd-run"></a>
### 4.3 run (execution)

```
steel run [--root <path>] [--file <path>] [--profile <name>]
           [--toolchain <path>] [--bake <name>] [--all]
           [--print] [--no-cache]
           [--log <path>] [--log-mode <append|truncate>] [-v]
```

Semantique:
- lit un fichier MUF (par defaut `steelconf`),
- selectionne les bakes a executer,
- construit les commandes et execute les tools.

<a id="cmd-doctor"></a>
### 4.4 doctor

```
steel doctor [--root <path>] [--json] [-v]
```

- diagnostics de presence du fichier de config et des tools.

<a id="cmd-cache"></a>
### 4.5 cache

```
steel cache <status|clear> [--root <path>] [--json] [-v]
```

- `status`: taille/nb fichiers de `.steel-cache`.
- `clear`: suppression du cache.

<a id="cmd-graph"></a>
### 4.6 graph (stub)

```
steel graph [--root <path>] [--text|--dot] [-v]
```

- comportement stub: sortie deterministe et placeholder.

<a id="cmd-fmt"></a>
### 4.7 fmt (stub)

```
steel fmt [--file <path>] [--check] [-v]
```

- comportement stub: placeholder.

<a id="4-8-codes-de-sortie"></a>
### 4.8 Codes de sortie

- `0`: succes.
- `1`: erreur d execution / configuration.
- `2`: erreur d usage ou de configuration (runner).
- `3`: erreur d execution d outil.
- `4`: erreur I/O.

<a id="5-resolution-et-discovery-build-steel"></a>
## 5. Resolution et discovery (build steelconf)

<a id="5-1-racine-du-workspace"></a>
### 5.1 Racine du workspace

- par defaut: repertoire courant.
- aucun argument: `steel` utilise le repertoire courant.

<a id="5-2-recherche-de-steelfile"></a>
### 5.2 Recherche de steelconf

Ordre:
1) `steelconf` dans la racine.
2) scan DFS deterministe sous la racine, avec tri lexicographique.

Regles de scan (fixes):
- profondeur max: 16.
- dossiers ignores: `.git`, `.hg`, `.svn`, `target`, `node_modules`, `dist`, `build`, `.steel`, `.steel-cache`.
- fichiers/dirs caches ignores.
- symlinks non suivis.

<a id="5-3-profil-et-target"></a>
### 5.3 Profil et target

- definis dans `steelconf` (workspace + blocs profile/target).

<a id="5-4-toolchain-et-fingerprint"></a>
### 5.4 Toolchain et fingerprint

- prise des variables: `CC`, `CXX`, `AR`, `LD`, `RUSTC`.
- overrides par backend (toolchain):
  - `toolchain.python` -> `PYTHON`
  - `toolchain.ocaml` -> `OCAMLPATH`
  - `toolchain.ghc` -> `GHC_PACKAGE_PATH`
- overrides explicites via env:
  - `MUFFIN_TOOLCHAIN_PYTHON`, `MUFFIN_TOOLCHAIN_OCAML`, `MUFFIN_TOOLCHAIN_GHC`
- fingerprint deterministe (FNV-1a 64-bit) sur:
  - bytes du steelconf,
  - profile + target,
  - outils et versions.
- `MUFFIN_FINGERPRINT_TIME=1` ajoute un sel temporel (non deterministe).

<a id="6-format-steelconfig-mff-mff-v1"></a>
## 6. Format steelconfig.mff (mff v1)

<a id="6-1-header"></a>
### 6.1 Header

```
mff 1
```

<a id="6-2-sections"></a>
### 6.2 Sections

```
project
  root "/path/to/root"
  steelfile "/path/to/steelconf"
.end

select
  profile "debug"
  target "x86_64-unknown-linux-gnu"
.end

paths
  build "/path/to/root/build"
  dist "/path/to/root/dist"
  cache "/path/to/root/.steel-cache"
.end

toolchain
  cc "gcc"
  cxx "g++"
  ar "ar"
  ld "ld"
  rustc "rustc"
  python "python3"
  ocaml "/opt/ocaml/lib"
  ghc "/opt/ghc/pkgdb"

  versions
    cc "gcc (GCC) 13.2.0"
  .end
.end

vars
  set "steel.profile" "debug"
  set "steel.target" "x86_64-unknown-linux-gnu"
  set "steel.offline" "false"
  set "steel.root" "/path/to/root"
  set "steel.file" "/path/to/steelconf"
.end

fingerprint
  value "fnv1a64:..."
.end
```

Notes:
- structure deterministe (ordre stable via BTreeMap).
- `.end` ferme les sections.

<a id="6-3-schemas-machine-readables-ide-ci"></a>
### 6.3 Schemas machine-readables (IDE/CI)

- `steel.graph.json/1` (export DAG) : `schemas/steel.graph.json.schema.json`
- `steel.decompile.report` (mff report JSON) : `schemas/steel.decompile.report.schema.json`
- `steel.fingerprints.json/1` (sidecar fingerprints) : `schemas/steel.fingerprints.json.schema.json`
- Grammaire MUF : `assets/grammar/steel.ebnf`

<a id="7-format-muf-v4-1"></a>
## 7. Format MUF (v4.1)

<a id="7-1-structure-generale"></a>
### 7.1 Structure generale

- shebang optionnel (`#!`), BOM optionnel.
- en-tete obligatoire: `!muf 4`.
- blocs `[tag name?] ... ..`.
- directives `.op arg1 arg2 ...`.
- commentaires `;; ...`.

<a id="7-2-lexemes"></a>
### 7.2 Lexemes

- Name: `[A-Za-z_][A-Za-z0-9_]*`
- String: `"..."` avec echappements `\n \r \t \0 \xNN \uNNNN`.
- Number: `+42`, `-7`, `3.14`, `1.2e3`.
- Ref: `~name/name/...`.

<a id="7-3-ebnf-minimal"></a>
### 7.3 EBNF minimal

```ebnf
Block     = WS0 , BlockHead , WS0 , NL , { BlockItem } , WS0 , BlockClose , WS0 , NL ;
BlockHead = "[" , WS0 , Tag , [ WS1 , Name ] , WS0 , "]" ;
BlockClose = ".." ;
Directive = WS0 , "." , Op , { WS1 , Atom } , WS0 , NL ;
Atom = Ref | String | Number | Name ;
```

<a id="8-regles-muf-interpretees-par-le-runner"></a>
## 8. Regles MUF interpretees par le runner

Le runner actuel supporte un sous-ensemble volontairement limite.

<a id="8-1-bloc-workspace"></a>
### 8.1 Bloc [workspace]

- `.set <key> <value>`
- stocke des variables globales (ex: `profile`, `target_dir`).

<a id="8-2-bloc-profile-name"></a>
### 8.2 Bloc [profile <name>]

- `.set <key> <value>`
- variables appliquees au moment de l execution.

<a id="8-3-bloc-tool-name"></a>
### 8.3 Bloc [tool <name>]

- `.exec <path-or-name>`
- definit l executable (ex: `gcc`).

<a id="8-4-bloc-bake-name"></a>
### 8.4 Bloc [bake <name>]

- `.make <id> <kind> <pattern>`
  - `kind` supporte: `glob`, `cglob`, `file` (alias: `list`).
  - raccourci: `.make <id> <path>` == `.make <id> file <path>`.
  - pour lister plusieurs fichiers, repeter `.make`.
- `.needs <bake>`: dependance explicite.
- `.output <port> <path>`: output final.
- sous-bloc `[run <tool>]` obligatoire.

<a id="8-5-bloc-run-tool"></a>
### 8.5 Bloc [run <tool>]

Directives supportees:
- `.takes <port> as "@args"`
  - seul `@args` est supporte.
  - injecte la liste des sources comme arguments bruts.
- `.emits <port> as "-o"`
  - associe un port a un flag de sortie.
- `.set <flag> <value>`
  - si value = `1|true|yes` -> ajoute le flag seul.
  - si value = `0|false|no` -> ignore.
  - sinon -> ajoute `flag` puis `value`.
- `.include <path>` -> `-I <path>`
- `.define <K> [<V>]` -> `-DK` ou `-DK=V`
- `.libdir <path>` -> `-L <path>`
- `.lib <name>` -> `-l <name>`

<a id="8-6-variables-et-interpolation"></a>
### 8.6 Variables et interpolation

- syntaxe: `${key}`
- scope: variables du workspace + profile.

Exemple:
```
.set "-O${opt}" 1
.set "-g" "${debug}"
```

<a id="9-regles-de-selection-et-execution"></a>
## 9. Regles de selection et execution

<a id="9-1-selection-des-bakes"></a>
### 9.1 Selection des bakes

Priorite:
1) `--all` -> tous les bakes dans l ordre du fichier.
2) `--bake <name>` (repete).
3) bake `app` si present.
4) premier bake dans le fichier.

<a id="9-2-dag-et-dependances"></a>
### 9.2 DAG et dependances

- `.needs` cree des dependances entre bakes.
- l ordre est un tri topologique.
- un cycle declenche une erreur.

<a id="9-3-incremental-et-cache"></a>
### 9.3 Incremental et cache

- par defaut: skip si l output est plus recent que tous les inputs.
- `--no-cache` force l execution.

<a id="10-logs-d-execution-runner"></a>
## 10. Logs d execution (runner)

<a id="10-1-emplacement"></a>
### 10.1 Emplacement

- par defaut: `target/steel_run_<timestamp>.mff`.
- `--log <path>` surcharge l emplacement.
- `--log-mode truncate` recree le fichier avant execution.

<a id="10-2-format"></a>
### 10.2 Format

```
[log meta]
format "steel-runlog-1"
tool "steel"
version "steel <version>"
ts_iso "<RFC3339>"
..

[bake log "<name>"]
output "<path>"
sources_count <n>
source "<path>"   ;; repete
[run log]
ts <epoch>
ts_iso "<RFC3339>"
duration_ms <n>
cmd "gcc ..."
cwd "/path/to/root"
status <code>
ok true|false
stdout_bytes <n>
stderr_bytes <n>
stdout "..."   ;; optionnel
stderr "..."   ;; optionnel
..
runs <n>
duration_ms <n>
..
[run summary]
ts_iso "<RFC3339>"
..
```

<a id="11-globs-et-determinisme"></a>
## 11. Globs et determinisme

- patterns `glob` / `cglob` sont resolus avec un moteur de glob interne.
- les chemins sont converts en paths relatifs au root.
- l ordre est stable (lexicographique).

<a id="12-exemples-complets"></a>
## 12. Exemples complets

<a id="12-1-exemple-minimal-c"></a>
### 12.1 Exemple minimal (C)

```
!muf 4

[workspace]
  .set name "steel-example-c"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set opt 0
  .set debug 1
  .set ndebug 0
..

[tool gcc]
  .exec "gcc"
..

[bake app]
  .make c_src cglob "**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O${opt}" 1
    .set "-g" "${debug}"
    .set "-DNDEBUG" "${ndebug}"
    .set "-Wall" 1
    .set "-Wextra" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```

<a id="12-2-cli"></a>
### 12.2 CLI

```
# configuration
steel
steel print

# execution
steel run --root . --file steelconf --bake app
steel run --root . --all --print
```

<a id="12-3-exemples-steelconfig-muff-c-c-et-ocaml"></a>
### 12.3 Exemples steelconf (C/C++/OCaml)

#### C (binaire simple)

```
!muf 4

[workspace]
  .set name "demo-c"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool gcc]
  .exec "gcc"
..

[bake app_c]
  .make c_src cglob "src/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_c"
..
```

#### C (lib statique + link)

```
!muf 4

[workspace]
  .set name "demo-c-lib"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gcc]
  .exec "gcc"
..

[tool ar]
  .exec "ar"
..

[bake lib_c]
  .make c_src cglob "lib/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O2" 1
    .set "-c" 1
    .emits obj as "-o"
  ..
  .output obj "target/obj/lib_c.o"
..

[bake app_c]
  .make main file "src/main.c"
  [run gcc]
    .takes main as "@args"
    .takes "target/obj/lib_c.o" as "@args"
    .set "-O2" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_c"
..
```

#### C++ (binaire simple)

```
!muf 4

[workspace]
  .set name "demo-cpp"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool gpp]
  .exec "g++"
..

[bake app_cpp]
  .make cpp_src cglob "src/**/*.cpp"
  [run gpp]
    .takes cpp_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_cpp"
..
```

#### C++ (include + libs)

```
!muf 4

[workspace]
  .set name "demo-cpp-inc"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gpp]
  .exec "g++"
..

[bake app_cpp]
  .make cpp_src cglob "src/**/*.cpp"
  [run gpp]
    .takes cpp_src as "@args"
    .set "-std=c++20" 1
    .set "-O2" 1
    .set "-Iinclude" 1
    .set "-lpthread" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_cpp"
..
```

#### OCaml (native ocamlopt)

```
!muf 4

[workspace]
  .set name "demo-ocaml"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool ocamlopt]
  .exec "ocamlopt"
..

[bake app_ml]
  .make ml_src cglob "src/**/*.ml"
  [run ocamlopt]
    .takes ml_src as "@args"
    .set "-O2" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_ml"
..
```

#### OCaml (bytecode ocamlc)

```
!muf 4

[workspace]
  .set name "demo-ocaml-byte"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool ocamlc]
  .exec "ocamlc"
..

[bake app_ml_byte]
  .make ml_src cglob "src/**/*.ml"
  [run ocamlc]
    .takes ml_src as "@args"
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_ml.byte"
..
```

<a id="12-4-exemples-avances-multi-bakes-tests-libs-dynamiques"></a>
### 12.4 Exemples avances (multi-bakes, tests, libs dynamiques)

#### Multi-bakes (lib + app C)

```
!muf 4

[workspace]
  .set name "demo-multi-bakes"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gcc]
  .exec "gcc"
..

[bake lib_core]
  .make c_src cglob "lib/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O2" 1
    .set "-c" 1
    .emits obj as "-o"
  ..
  .output obj "target/obj/lib_core.o"
..

[bake app]
  .make main file "src/main.c"
  [run gcc]
    .takes main as "@args"
    .takes "target/obj/lib_core.o" as "@args"
    .set "-O2" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app"
..
```

#### Tests C (bake dedie)

```
!muf 4

[workspace]
  .set name "demo-tests-c"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool gcc]
  .exec "gcc"
..

[bake tests_c]
  .make test_src cglob "tests/**/*.c"
  [run gcc]
    .takes test_src as "@args"
    .set "-std=c17" 1
    .set "-g" 1
    .set "-O0" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/tests_c"
..
```

#### Lib dynamique C (shared)

```
!muf 4

[workspace]
  .set name "demo-shared-c"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gcc]
  .exec "gcc"
..

[bake lib_shared]
  .make c_src cglob "src/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-fPIC" 1
    .set "-shared" 1
    .set "-O2" 1
    .emits so as "-o"
  ..
  .output so "target/lib/libdemo.so"
..
```

#### Lib dynamique C (shared) — Windows

```
!muf 4

[workspace]
  .set name "demo-shared-c-win"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gcc]
  .exec "gcc"
..

[bake lib_shared_win]
  .make c_src cglob "src/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-shared" 1
    .set "-O2" 1
    .emits dll as "-o"
  ..
  .output dll "target\\lib\\demo.dll"
..
```

#### Lib dynamique C (shared) — macOS

```
!muf 4

[workspace]
  .set name "demo-shared-c-macos"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool clang]
  .exec "clang"
..

[bake lib_shared_macos]
  .make c_src cglob "src/**/*.c"
  [run clang]
    .takes c_src as "@args"
    .set "-shared" 1
    .set "-O2" 1
    .emits dylib as "-o"
  ..
  .output dylib "target/lib/libdemo.dylib"
..
```

#### Lib dynamique C++ (shared)

```
!muf 4

[workspace]
  .set name "demo-shared-cpp"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gpp]
  .exec "g++"
..

[bake lib_shared_cpp]
  .make cpp_src cglob "src/**/*.cpp"
  [run gpp]
    .takes cpp_src as "@args"
    .set "-fPIC" 1
    .set "-shared" 1
    .set "-std=c++20" 1
    .set "-O2" 1
    .emits so as "-o"
  ..
  .output so "target/lib/libdemo_cpp.so"
..
```

#### Lib dynamique C++ (shared) — Windows

```
!muf 4

[workspace]
  .set name "demo-shared-cpp-win"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool gpp]
  .exec "g++"
..

[bake lib_shared_cpp_win]
  .make cpp_src cglob "src/**/*.cpp"
  [run gpp]
    .takes cpp_src as "@args"
    .set "-shared" 1
    .set "-std=c++20" 1
    .set "-O2" 1
    .emits dll as "-o"
  ..
  .output dll "target\\lib\\demo_cpp.dll"
..
```

#### Lib dynamique C++ (shared) — macOS

```
!muf 4

[workspace]
  .set name "demo-shared-cpp-macos"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool clangpp]
  .exec "clang++"
..

[bake lib_shared_cpp_macos]
  .make cpp_src cglob "src/**/*.cpp"
  [run clangpp]
    .takes cpp_src as "@args"
    .set "-shared" 1
    .set "-std=c++20" 1
    .set "-O2" 1
    .emits dylib as "-o"
  ..
  .output dylib "target/lib/libdemo_cpp.dylib"
..
```

#### OCaml (lib + app)

```
!muf 4

[workspace]
  .set name "demo-ocaml-lib"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[tool ocamlopt]
  .exec "ocamlopt"
..

[bake lib_ml]
  .make ml_src cglob "lib/**/*.ml"
  [run ocamlopt]
    .takes ml_src as "@args"
    .set "-a" 1
    .emits lib as "-o"
  ..
  .output lib "target/lib/libml.cmxa"
..

[bake app_ml]
  .make ml_src cglob "src/**/*.ml"
  [run ocamlopt]
    .takes ml_src as "@args"
    .takes "target/lib/libml.cmxa" as "@args"
    .set "-O2" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/app_ml"
..
```

#### C++ (tests + runner simple)

```
!muf 4

[workspace]
  .set name "demo-cpp-tests"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool gpp]
  .exec "g++"
..

[tool sh]
  .exec "sh"
..

[bake tests_cpp]
  .make test_src cglob "tests/**/*.cpp"
  [run gpp]
    .takes test_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/tests_cpp"
..

[bake run_tests]
  [run sh]
    .set "-c" 1
    .set "./target/bin/tests_cpp" 1
  ..
..
```

#### C++ (tests + runner PowerShell, Windows)

```
!muf 4

[workspace]
  .set name "demo-cpp-tests-win"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[tool gpp]
  .exec "g++"
..

[tool pwsh]
  .exec "pwsh"
..

[bake tests_cpp]
  .make test_src cglob "tests/**/*.cpp"
  [run gpp]
    .takes test_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target\\bin\\tests_cpp.exe"
..

[bake run_tests]
  [run pwsh]
    .set "-NoProfile" 1
    .set "-ExecutionPolicy" 1
    .set "Bypass" 1
    .set "-Command" 1
    .set ".\\target\\bin\\tests_cpp.exe" 1
  ..
..
```

#### C++ (tests conditionnes + PowerShell, Windows)

```
!muf 4

[workspace]
  .set name "demo-cpp-tests-win-profile"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set run_tests 1
..

[profile release]
  .set run_tests 0
..

[tool gpp]
  .exec "g++"
..

[tool pwsh]
  .exec "pwsh"
..

[bake tests_cpp]
  .make test_src cglob "tests/**/*.cpp"
  [run gpp]
    .takes test_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target\\bin\\tests_cpp.exe"
..

[bake run_tests]
  [run pwsh]
    .set "-NoProfile" 1
    .set "-ExecutionPolicy" 1
    .set "Bypass" 1
    .set "-Command" 1
    .set "if (${run_tests} -eq 1) { .\\target\\bin\\tests_cpp.exe }" 1
  ..
..
```

#### C++ (tests conditionnes + PowerShell, Windows, skip)

```
!muf 4

[workspace]
  .set name "demo-cpp-tests-win-profile-skip"
  .set root "."
  .set target_dir "target"
  .set profile "release"
..

[profile debug]
  .set run_tests 1
..

[profile release]
  .set run_tests 0
..

[tool gpp]
  .exec "g++"
..

[tool pwsh]
  .exec "pwsh"
..

[bake tests_cpp]
  .make test_src cglob "tests/**/*.cpp"
  [run gpp]
    .takes test_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target\\bin\\tests_cpp.exe"
..

[bake run_tests]
  [run pwsh]
    .set "-NoProfile" 1
    .set "-ExecutionPolicy" 1
    .set "Bypass" 1
    .set "-Command" 1
    .set "if (${run_tests} -eq 1) { .\\target\\bin\\tests_cpp.exe } else { Write-Host \"skip tests (run_tests=0)\"; exit 5 }" 1
  ..
..
```

#### C++ (tests conditionnes par profile)

```
!muf 4

[workspace]
  .set name "demo-cpp-tests-profile"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set run_tests 1
..

[profile release]
  .set run_tests 0
..

[tool gpp]
  .exec "g++"
..

[tool sh]
  .exec "sh"
..

[bake tests_cpp]
  .make test_src cglob "tests/**/*.cpp"
  [run gpp]
    .takes test_src as "@args"
    .set "-std=c++20" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/tests_cpp"
..

[bake run_tests]
  [run sh]
    .set "-c" 1
    .set "test ${run_tests} -eq 1 && ./target/bin/tests_cpp" 1
  ..
..
```

#### C (tests conditionnes par profile)

```
!muf 4

[workspace]
  .set name "demo-c-tests-profile"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set run_tests 1
..

[profile release]
  .set run_tests 0
..

[tool gcc]
  .exec "gcc"
..

[tool sh]
  .exec "sh"
..

[bake tests_c]
  .make test_src cglob "tests/**/*.c"
  [run gcc]
    .takes test_src as "@args"
    .set "-std=c17" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/bin/tests_c"
..

[bake run_tests]
  [run sh]
    .set "-c" 1
    .set "test ${run_tests} -eq 1 && ./target/bin/tests_c" 1
  ..
..
```

Petit exemple de test C minimal (pour `tests/**/*.c`):

```
#include <stdio.h>
#include <assert.h>

int main(void) {
    assert(1 + 1 == 2);
    puts("ok");
    return 0;
}
```

Petit exemple de test C++ minimal (pour `tests/**/*.cpp`):

```
#include <iostream>
#include <cassert>

int main() {
    assert(1 + 1 == 2);
    std::cout << "ok\n";
    return 0;
}
```

#### C (tests conditionnes + PowerShell, Windows)

```
!muf 4

[workspace]
  .set name "demo-c-tests-win-profile"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set run_tests 1
..

[profile release]
  .set run_tests 0
..

[tool gcc]
  .exec "gcc"
..

[tool pwsh]
  .exec "pwsh"
..

[bake tests_c]
  .make test_src cglob "tests/**/*.c"
  [run gcc]
    .takes test_src as "@args"
    .set "-std=c17" 1
    .set "-O0" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target\\bin\\tests_c.exe"
..

[bake run_tests]
  [run pwsh]
    .set "-NoProfile" 1
    .set "-ExecutionPolicy" 1
    .set "Bypass" 1
    .set "-Command" 1
    .set "if (${run_tests} -eq 1) { .\\target\\bin\\tests_c.exe }" 1
  ..
..
```
<a id="12-5-table-exemples-objectif-toolchain"></a>
### 12.5 Table recap (exemple -> objectif -> toolchain)

| Exemple | Objectif | Toolchain |
| --- | --- | --- |
| `12.1` | C minimal, binaire simple | `gcc` |
| `12.3` C (binaire) | Binaire C avec sources multiples | `gcc` |
| `12.3` C (lib + link) | Compilation en deux etapes | `gcc`, `ar` |
| `12.3` C++ (binaire) | Binaire C++ standard | `g++` |
| `12.3` C++ (include + libs) | Include + lien lib | `g++` |
| `12.3` OCaml (native) | Binaire natif OCaml | `ocamlopt` |
| `12.3` OCaml (bytecode) | Binaire bytecode OCaml | `ocamlc` |
| `12.4` Multi-bakes | Lib + app, bakes separes | `gcc` |
| `12.4` Tests C | Bake de tests | `gcc` |
| `12.4` Lib dynamique C | `.so` partagee | `gcc` |
| `12.4` Lib dynamique C++ | `.so` partagee | `g++` |
| `12.4` OCaml (lib + app) | Lib OCaml + binaire | `ocamlopt` |
| `12.4` Lib dynamique C (Windows) | `.dll` Windows | `gcc` |
| `12.4` Lib dynamique C (macOS) | `.dylib` macOS | `clang` |
| `12.4` Lib dynamique C++ (Windows) | `.dll` Windows | `g++` |
| `12.4` Lib dynamique C++ (macOS) | `.dylib` macOS | `clang++` |
| `12.4` C++ tests + runner | Binaire tests + run | `g++`, `sh` |
| `12.4` C++ tests + runner (Windows) | Binaire tests + run | `g++`, `pwsh` |
| `12.4` C++ tests par profile | Tests conditionnes | `g++`, `sh` |
| `12.4` C tests par profile | Tests conditionnes | `gcc`, `sh` |
| `12.4` C++ tests (Windows, conditionnel) | Tests conditionnes | `g++`, `pwsh` |
| `12.4` C tests (Windows, conditionnel) | Tests conditionnes | `gcc`, `pwsh` |
| `12.4` C++ tests (Windows, conditionnel, skip) | Tests conditionnes + skip | `g++`, `pwsh` |

<a id="13-diagnostics-et-erreurs-courantes"></a>
## 13. Diagnostics et erreurs courantes

- `error[U001]`: commande inconnue ou manquante.
- `error[E001]`: echec interne de configuration.
- `error[C001]`: erreur de configuration du runner.
- `error[P001]`: erreur de parsing MUF.
- `error[X001]`: tool a echoue.
- `error[IO01]`: erreur I/O.

Conseils:
- verifier l existence de `steelconf`.
- utiliser `workspace.profile`.
- activer `-v` pour des diagnostics.

<a id="14-roadmap-cibles-plausibles"></a>
## 14. Roadmap (cibles plausibles)

- Implementer `steel graph` avec export `text|dot|json` depuis le DAG resolu.
- Implementer `steel fmt` (normalisation des blocs/directives).
- Remplacer la resolution minimale de `build steelconf` par un parser MUF complet.
- Etendre le runner a d autres blocs (targets, toolchains, exports, plans).
- Exposer un format `.mff` de run log stable et versionne.

<a id="milestones-proposition"></a>
### Milestones (proposition)

M1 - CLI et outputs:
- graph text/dot/json
- fmt --check + format deterministe

M2 - Configuration complete:
- parse MUF complet dans `build steelconf`
- emission `steelconfig.mff` basee sur le modele resolu

M3 - Runner et outillage:
- support targets/toolchains/exports/plans
- run log versionne stable

<a id="15-limitations-et-etat-actuel"></a>
## 15. Limitations et etat actuel

- `graph` et `fmt` sont des stubs (placeholder deterministe).
- le resolver de `build steelconf` est minimal (pas de parse MUF complet).
- le runner supporte un sous-ensemble de MUF (workspace/profile/tool/bake/run).

<a id="16-glossaire-rapide"></a>
## 16. Glossaire rapide

- **MUF**: format declaratif source.
- **MFF**: format resolu, stable, emet par `build steelconf`.
- **Bake**: noeud du graphe d execution.
- **Run**: etape d execution d un tool.
- **Toolchain**: ensemble des executables (cc/ld/ar/rustc).

<a id="17-tutoriel-complet-pas-a-pas"></a>
## 17. Tutoriel complet (pas a pas)

Cette section est un guide long format qui suit un projet fictif de A a Z.
Elle reutilise les concepts de la reference et les illustre avec un chemin
concret: ecrire un steelconf, generer un MFF, puis executer.

<a id="17-1-objectif-et-plan-du-tuto"></a>
### 17.1 Objectif et plan du tuto

Nous allons construire un petit projet C avec une lib statique et un binaire:
- `src/lib/` contient `lib.c` + `lib.h`
- `src/app/` contient `main.c`
- build en `target/out/`

Plan:
1) creer un workspace minimal
2) declarer les tools
3) declarer les bakes
4) declarer les profiles
5) executer avec `steel run`
6) utiliser la configuration resolue `.mff`

<a id="17-2-arborescence-initiale"></a>
### 17.2 Arborescence initiale

```
myproj/
  steelconf
  src/
    lib/
      lib.c
      lib.h
    app/
      main.c
```

Contenu minimal pour `lib.c`:
```
int add(int a, int b) { return a + b; }
```

Contenu minimal pour `main.c`:
```
#include "../lib/lib.h"
int main(void) { return add(1, 2); }
```

<a id="17-3-base-muf"></a>
### 17.3 Base MUF

On commence par le header et un workspace:
```
!muf 4

[workspace]
  .set name "myproj"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..
```

Ce bloc fixe des variables globales utilisables dans les autres blocs.

<a id="17-4-declarer-un-tool"></a>
### 17.4 Declarer un tool

```
[tool cc]
  .exec "gcc"
..
```

Le nom `cc` est logique, il n est pas lie a la commande systeme.

<a id="17-5-premier-bake"></a>
### 17.5 Premier bake

```
[bake lib]
  .make c_src cglob "src/lib/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O0" 1
    .emits obj as "-o"
  ..
  .output obj "target/out/lib.o"
..
```

<a id="17-6-bake-app-qui-depend-de-lib"></a>
### 17.6 Bake app qui depend de lib

```
[bake app]
  .make c_src cglob "src/app/*.c"
  .needs lib
  [run cc]
    .takes c_src as "@args"
    .include "src/lib"
    .set "-std=c17" 1
    .set "-O0" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```

<a id="17-7-execution"></a>
### 17.7 Execution

```
steel run --root . --file steelconf --bake app
```

Le runner va:
- prendre le bake `app`
- executer d abord `lib`
- puis executer `app`

<a id="17-8-premiere-configuration-resolue"></a>
### 17.8 Premiere configuration resolue

```
steel
```

Cela produit `steelconfig.mff`. C est une photo stable de votre config.

<a id="18-tutoriel-profiles-et-variantes"></a>
## 18. Tutoriel: profiles et variantes

<a id="18-1-ajouter-un-profile-release"></a>
### 18.1 Ajouter un profile release

```
[profile release]
  .set opt 2
  .set debug 0
  .set ndebug 1
..
```

Et adapter le run:
```
.set "-O${opt}" 1
.set "-g" "${debug}"
.set "-DNDEBUG" "${ndebug}"
```

<a id="18-2-selection-du-profile"></a>
### 18.2 Selection du profile

```
steel run --profile release --bake app
```

La selection `--profile` surcharge `workspace.profile`.

<a id="18-3-profiles-multiples"></a>
### 18.3 Profiles multiples

On peut declarer autant de profiles que necessaire:
```
[profile sanitize]
  .set opt 1
  .set debug 1
  .set asan 1
..
```

Dans le run:
```
.set "-fsanitize=address" "${asan}"
```

<a id="19-tutoriel-targets-et-portabilite"></a>
## 19. Tutoriel: targets et portabilite

<a id="19-1-variables-de-target"></a>
### 19.1 Variables de target

```
[workspace]
  .set target "x86_64-unknown-linux-gnu"
..
```

Vous pouvez utiliser `${target}` dans les chemins ou flags.

<a id="19-2-separer-output-par-target"></a>
### 19.2 Separer output par target

```
.set target_dir "target/${target}"
```

Cela evite les collisions entre builds.

<a id="19-3-exemple-windows"></a>
### 19.3 Exemple Windows

```
[profile windows]
  .set exe_ext ".exe"
..
```

Puis:
```
.output exe "target/out/app${exe_ext}"
```

<a id="20-tutoriel-outils-toolchain-et-versions"></a>
## 20. Tutoriel: outils, toolchain et versions

<a id="20-1-plusieurs-tools"></a>
### 20.1 Plusieurs tools

```
[tool cc]
  .exec "gcc"
..
[tool ar]
  .exec "ar"
..
```

<a id="20-2-toolchain-parametrable"></a>
### 20.2 Toolchain parametrable

Dans la vraie vie, on veut choisir `clang` ou `gcc`.
Une strategie simple:
```
[workspace]
  .set cc "gcc"
..
[tool cc]
  .exec "${cc}"
..
```

<a id="20-3-version-et-fingerprint"></a>
### 20.3 Version et fingerprint

Le build `steel` calcule un fingerprint stable:
- contenu du steelconf
- profil/target
- versions des tools

Le fingerprint peut etre injecte dans vos sorties ou logs.

<a id="21-tutoriel-bakes-avances"></a>
## 21. Tutoriel: bakes avances

<a id="21-1-plusieurs-outputs"></a>
### 21.1 Plusieurs outputs

Un bake peut emettre plusieurs ports:
```
[bake compile]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .emits obj as "-o"
  ..
  .output obj "target/out/app.o"
  .output dep "target/out/app.d"
..
```

<a id="21-2-plusieurs-runs"></a>
### 21.2 Plusieurs runs

Un bake peut avoir plusieurs runs, mais ici le runner supporte un seul bloc `run`.
Dans ce cas, on decoupe en deux bakes:
- `compile` -> `obj`
- `link` -> `exe`

<a id="21-3-parametrer-les-patterns"></a>
### 21.3 Parametrer les patterns

On peut utiliser des variables dans les glob:
```
.make c_src cglob "src/${module}/*.c"
```

<a id="22-tutoriel-wiring-et-ports"></a>
## 22. Tutoriel: wiring et ports

<a id="22-1-passer-un-output-comme-input"></a>
### 22.1 Passer un output comme input

Dans une version complete, on relierait un port `obj` vers `link`.
Le runner actuel ne resolv pas un wiring complet, mais la logique est:
```
.needs compile
```

<a id="22-2-convention-de-ports"></a>
### 22.2 Convention de ports

Conventions conseillees:
- `src` pour inputs
- `obj` pour objets
- `exe` pour binaire final
- `lib` pour archives

<a id="22-3-ports-nommes"></a>
### 22.3 Ports nommes

Les ports doivent etre stables et predictibles, car ils sont referencables.

<a id="23-tutoriel-execution-et-logs"></a>
## 23. Tutoriel: execution et logs

<a id="23-1-logs-par-defaut"></a>
### 23.1 Logs par defaut

Le run produit un fichier `.mff` de log dans `target/`.
Ce log sert a diagnostiquer les commandes et la duree.

<a id="23-2-forcer-un-log-fixe"></a>
### 23.2 Forcer un log fixe

```
steel run --log target/runlog.mff --log-mode truncate
```

<a id="23-3-lecture-rapide-du-log"></a>
### 23.3 Lecture rapide du log

Reperer:
- `cmd`
- `status`
- `stdout` / `stderr`

<a id="24-tutoriel-cache-et-incremental"></a>
## 24. Tutoriel: cache et incremental

<a id="24-1-principe"></a>
### 24.1 Principe

Le runner compare:
- timestamps des inputs
- timestamps des outputs

Si output est plus recent, il skip.

<a id="24-2-desactiver-le-cache"></a>
### 24.2 Desactiver le cache

```
steel run --no-cache
```

<a id="24-3-regles-pour-un-build-fiable"></a>
### 24.3 Regles pour un build fiable

- toujours ecrire outputs dans un dossier stable
- eviter les flags non deterministes
- stabiliser les listes de fichiers via glob deterministes

<a id="25-tutoriel-graph-et-introspection"></a>
## 25. Tutoriel: graph et introspection

<a id="25-1-graph-text"></a>
### 25.1 Graph text

```
steel graph --text
```

Dans le futur, cette commande montrera les bakes et dependances.

<a id="25-2-graph-dot"></a>
### 25.2 Graph dot

```
steel graph --dot > graph.dot
```

<a id="25-3-json-de-graphe"></a>
### 25.3 JSON de graphe

Le schema cible est `schemas/steel.graph.json.schema.json`.
Utiliser ce format pour IDE/CI.

<a id="26-tutoriel-diagnostics-et-qualite"></a>
## 26. Tutoriel: diagnostics et qualite

<a id="26-1-types-de-diagnostics"></a>
### 26.1 Types de diagnostics

- erreurs bloquantes
- warnings informatifs
- notes pour contexte

<a id="26-2-exemple-d-erreur-de-syntaxe"></a>
### 26.2 Exemple d erreur de syntaxe

```
error[P001]: invalid token
  at steelconf:12:3
```

<a id="26-3-conseils-pour-debug"></a>
### 26.3 Conseils pour debug

- utiliser `steel print`
- isoler un bake via `--bake`
- activer `-v`

<a id="27-tutoriel-integration-ci-ide"></a>
## 27. Tutoriel: integration CI/IDE

<a id="27-1-ci"></a>
### 27.1 CI

Dans un pipeline:
1) `steel print > steelconfig.mff`
2) archiver `steelconfig.mff`
3) executer `steel run`

<a id="27-2-ide"></a>
### 27.2 IDE

L IDE peut:
- lire `steelconfig.mff`
- afficher le DAG via `graph.json`
- naviguer dans les sources

<a id="27-3-json-schemas"></a>
### 27.3 JSON schemas

Utiliser les schemas pour valider la sortie machine-readable.

<a id="28-tutoriel-structure-d-un-projet-moyen"></a>
## 28. Tutoriel: structure d un projet moyen

Exemple typique:
```
root/
  steelconf
  src/
    lib/
    app/
  include/
  tests/
  target/
  dist/
```

Conseils:
- `src/` pour les sources
- `include/` pour les headers
- `target/` pour les artefacts
- `dist/` pour les artefacts finaux

<a id="29-faq-rapide"></a>
## 29. FAQ rapide

<a id="29-1-pourquoi-separer-muf-et-mff"></a>
### 29.1 Pourquoi separer MUF et MFF

MUF est la source editable.
MFF est un artefact stable et deterministe.

<a id="29-2-comment-changer-de-tool"></a>
### 29.2 Comment changer de tool

Modifier la valeur de `.exec` ou utiliser une variable de workspace.

<a id="29-3-comment-deboguer-un-tool"></a>
### 29.3 Comment deboguer un tool

Utiliser `--log` et inspecter la commande.

<a id="30-annexes"></a>
## 30. Annexes

<a id="30-1-variables-d-environnement-supportees"></a>
### 30.1 Variables d environnement supportees

- `MUFFIN_FINGERPRINT_TIME` (sel temporel pour debug, non deterministe).
- `MUFFIN_PROFILE`, `MUFFIN_TARGET`, `MUFFIN_EMIT` sont ignores pour `steel`.

<a id="30-2-rappel-des-chemins-par-defaut"></a>
### 30.2 Rappel des chemins par defaut

- `steelconf` a la racine
- `steelconfig.mff` en sortie
- `target/` pour logs runner

<a id="30-3-exemple-de-steelfile-complet"></a>
### 30.3 Exemple de steelconf complet

```
!muf 4

[workspace]
  .set name "myproj"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set opt 0
  .set debug 1
  .set ndebug 0
..

[profile release]
  .set opt 3
  .set debug 0
  .set ndebug 1
..

[tool cc]
  .exec "gcc"
..

[bake lib]
  .make c_src cglob "src/lib/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O${opt}" 1
    .set "-g" "${debug}"
    .set "-DNDEBUG" "${ndebug}"
    .emits obj as "-o"
  ..
  .output obj "target/out/lib.o"
..

[bake app]
  .make c_src cglob "src/app/*.c"
  .needs lib
  [run cc]
    .takes c_src as "@args"
    .include "src/lib"
    .set "-std=c17" 1
    .set "-O${opt}" 1
    .set "-g" "${debug}"
    .set "-DNDEBUG" "${ndebug}"
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```

<a id="30-4-checklist-de-release"></a>
### 30.4 Checklist de release

- build en `release`
- executer tests
- verifier outputs dans `dist/`
- archiver `steelconfig.mff`

<a id="31-tutoriel-structure-multi-modules"></a>
## 31. Tutoriel: structure multi-modules

Objectif: organiser un projet avec plusieurs libs et un binaire final.

<a id="31-1-arborescence-type"></a>
### 31.1 Arborescence type

```
root/
  steelconf
  src/
    core/
      core.c
      core.h
    util/
      util.c
      util.h
    app/
      main.c
```

<a id="31-2-bakes-de-compilation"></a>
### 31.2 Bakes de compilation

```
[bake core]
  .make c_src cglob "src/core/*.c"
  [run cc]
    .takes c_src as "@args"
    .include "src/core"
    .emits obj as "-o"
  ..
  .output obj "target/out/core.o"
..

[bake util]
  .make c_src cglob "src/util/*.c"
  [run cc]
    .takes c_src as "@args"
    .include "src/util"
    .emits obj as "-o"
  ..
  .output obj "target/out/util.o"
..
```

<a id="31-3-bake-final"></a>
### 31.3 Bake final

```
[bake app]
  .make c_src cglob "src/app/*.c"
  .needs core
  .needs util
  [run cc]
    .takes c_src as "@args"
    .include "src/core"
    .include "src/util"
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
```

<a id="32-tutoriel-artefacts-et-conventions-de-sortie"></a>
## 32. Tutoriel: artefacts et conventions de sortie

<a id="32-1-convention-target-out"></a>
### 32.1 Convention target/out

Utiliser `target/out` pour les sorties immediates, puis `dist/` pour publier.

<a id="32-2-copier-vers-dist"></a>
### 32.2 Copier vers dist

Steel ne fait pas la copie automatiquement. On peut ajouter un bake:
```
[bake dist]
  .needs app
  [run sh]
    .set "cmd" "cp target/out/app dist/app"
  ..
  .output exe "dist/app"
..
```

Ce pattern est symbolique: il depend du runner et des tools autorises.

<a id="32-3-nommage-deterministe"></a>
### 32.3 Nommage deterministe

Choisir des noms stables pour:
- bakes
- ports
- outputs

<a id="33-tutoriel-schemas-et-json"></a>
## 33. Tutoriel: schemas et JSON

<a id="33-1-pourquoi-un-export-json"></a>
### 33.1 Pourquoi un export JSON

Le JSON est pratique pour:
- les IDE
- la CI
- les integrations externes

<a id="33-2-graph-json"></a>
### 33.2 Graph JSON

Le schema cible est `steel.graph.json/1`.
Le fichier est stable et deterministe.

<a id="33-3-validation"></a>
### 33.3 Validation

Valider un JSON via un validateur standard et le schema fourni.

<a id="34-tutoriel-mff-comme-contrat"></a>
## 34. Tutoriel: MFF comme contrat

<a id="34-1-mff-versionne"></a>
### 34.1 MFF versionne

Le header `mff 1` fixe la version du format.

<a id="34-2-utiliser-mff-en-ci"></a>
### 34.2 Utiliser MFF en CI

```
steel print > steelconfig.mff
```

Puis archiver l artefact. Cela permet:
- comparer les configs entre branches
- diagnostiquer des regressions

<a id="34-3-diff-mff"></a>
### 34.3 Diff MFF

Comme c est deterministe, un diff git est lisible et utile.

<a id="35-tutoriel-erreurs-et-recuperation"></a>
## 35. Tutoriel: erreurs et recuperation

<a id="35-1-erreurs-de-parse"></a>
### 35.1 Erreurs de parse

Si MUF est invalide, la phase `build steelconf` renvoie des diagnostics.

<a id="35-2-erreurs-de-run"></a>
### 35.2 Erreurs de run

Si une commande echoue, le code retour est non zero.

<a id="35-3-strategie-de-debug"></a>
### 35.3 Strategie de debug

- lancer un seul bake
- activer `--print`
- inspecter les logs

<a id="36-tutoriel-performance-et-scalabilite"></a>
## 36. Tutoriel: performance et scalabilite

<a id="36-1-limiter-les-globs"></a>
### 36.1 Limiter les globs

Eviter les patterns trop larges:
```
src/**/*
```

Preferer:
```
src/**/*.c
```

<a id="36-2-decouper-en-bakes"></a>
### 36.2 Decouper en bakes

Plusieurs bakes permettent d isoler les recompilations.

<a id="36-3-cache"></a>
### 36.3 Cache

Le cache est simple mais efficace pour des projets moyens.

<a id="37-tutoriel-tests-et-verifications"></a>
## 37. Tutoriel: tests et verifications

<a id="37-1-bake-de-tests"></a>
### 37.1 Bake de tests

```
[bake test]
  .needs app
  [run sh]
    .set "cmd" "target/out/app --selftest"
  ..
  .output exe "target/out/test.ok"
..
```

<a id="37-2-tests-par-profile"></a>
### 37.2 Tests par profile

```
steel run --profile debug --bake test
steel run --profile release --bake test
```

<a id="37-3-tests-en-ci"></a>
### 37.3 Tests en CI

Automatiser la sequence:
- build
- run tests
- archive artifacts

<a id="38-tutoriel-conventions-d-equipe"></a>
## 38. Tutoriel: conventions d equipe

<a id="38-1-style-steelfile"></a>
### 38.1 Style steelconf

Recommandations:
- un bloc par ligne de responsabilite
- noms explicites
- sections ordonnees

<a id="38-2-lint-humain"></a>
### 38.2 Lint humain

Avant `steel fmt`, etablir une convention d equipe.

<a id="38-3-revisions"></a>
### 38.3 Revisions

Valider les changements MUF comme du code:
- review
- diff
- tests

<a id="39-tutoriel-extensions-et-limites"></a>
## 39. Tutoriel: extensions et limites

<a id="39-1-runner-minimal"></a>
### 39.1 Runner minimal

Le runner actuel ne supporte pas toutes les directives MUF possibles.

<a id="39-2-extensibilite"></a>
### 39.2 Extensibilite

Les blocs et directives ont ete concus pour etre etendus sans casser la compatibilite.

<a id="39-3-cas-non-couverts"></a>
### 39.3 Cas non couverts

- outils complexes multi-etapes
- environnements distants
- cache distribue

<a id="40-annexes-supplementaires"></a>
## 40. Annexes supplementaires

<a id="40-1-checklists-par-phase"></a>
### 40.1 Checklists par phase

Configuration:
- steelconf valide
- profiles definis
- tools resolus

Execution:
- bakes ordonnes
- outputs stables
- logs ok

<a id="40-2-memo-muf"></a>
### 40.2 Memo MUF

- `!muf 4` header
- `[tag name]` blocs
- `.op arg` directives
- `..` fin de bloc

<a id="40-3-contacts-et-contribution"></a>
### 40.3 Contacts et contribution

Voir `CONTRIBUTING.md` pour les regles de contribution.

<a id="41-tutoriel-avance-build-c-complet-compile-link"></a>
## 41. Tutoriel avance: build C complet (compile + link)

Objectif: montrer un pipeline en deux etapes avec outputs intermediaires.

<a id="41-1-separation-compile-link"></a>
### 41.1 Separation compile/link

```
[bake compile]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .emits obj as "-o"
  ..
  .output obj "target/out/app.o"
..

[bake link]
  .needs compile
  [run cc]
    .set "cmd" "gcc target/out/app.o -o target/out/app"
  ..
  .output exe "target/out/app"
..
```

<a id="41-2-utiliser-ar-pour-une-lib-statique"></a>
### 41.2 Utiliser ar pour une lib statique

```
[bake ar]
  .needs compile
  [run ar]
    .set "cmd" "ar rcs target/out/lib.a target/out/app.o"
  ..
  .output lib "target/out/lib.a"
..
```

<a id="41-3-conseils"></a>
### 41.3 Conseils

- separer compilation et linkage facilite l incremental
- garder des outputs stables

<a id="42-tutoriel-interoperabilite-avec-d-autres-langages"></a>
## 42. Tutoriel: interoperabilite avec d autres langages

<a id="42-1-c-ocaml-conceptuel"></a>
### 42.1 C + OCaml (conceptuel)

Steel peut orchestrer plusieurs tools:
- `ocamlc`
- `gcc`

<a id="42-2-exemple-de-bakes-paralleles"></a>
### 42.2 Exemple de bakes paralleles

```
[bake ocaml]
  .make ml_src cglob "src/ocaml/*.ml"
  [run ocamlc]
    .takes ml_src as "@args"
    .emits obj as "-o"
  ..
  .output obj "target/out/ocaml.o"
..

[bake c]
  .make c_src cglob "src/c/*.c"
  [run cc]
    .takes c_src as "@args"
    .emits obj as "-o"
  ..
  .output obj "target/out/c.o"
..
```

<a id="42-3-binaire-final"></a>
### 42.3 Binaire final

```
[bake link]
  .needs ocaml
  .needs c
  [run cc]
    .set "cmd" "gcc target/out/ocaml.o target/out/c.o -o target/out/app"
  ..
  .output exe "target/out/app"
..
```

<a id="43-tutoriel-conventions-de-nommage"></a>
## 43. Tutoriel: conventions de nommage

<a id="43-1-noms-de-bakes"></a>
### 43.1 Noms de bakes

Regles conseillees:
- courts
- explicites
- pas d espace

Exemples:
- `compile`
- `link`
- `test`
- `dist`

<a id="43-2-noms-de-ports"></a>
### 43.2 Noms de ports

Ports usuels:
- `src`
- `obj`
- `lib`
- `exe`

<a id="43-3-noms-de-tools"></a>
### 43.3 Noms de tools

Preferer `cc`, `cxx`, `ar`, `ld` pour l intention.

<a id="44-tutoriel-variables-et-templates"></a>
## 44. Tutoriel: variables et templates

<a id="44-1-variables-globales"></a>
### 44.1 Variables globales

```
[workspace]
  .set cc "gcc"
  .set cflags "-Wall -Wextra"
..
```

<a id="44-2-utilisation"></a>
### 44.2 Utilisation

```
  .set "${cflags}" 1
```

<a id="44-3-garder-la-lisibilite"></a>
### 44.3 Garder la lisibilite

Eviter les chaines trop longues.
Preferer plusieurs `.set` simples.

<a id="45-tutoriel-macros-d-equipe-pattern"></a>
## 45. Tutoriel: macros d equipe (pattern)

<a id="45-1-facteur-commun"></a>
### 45.1 Facteur commun

Exemple: un bake standard C.
```
[bake c_compile]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-std=c17" 1
    .set "-O${opt}" 1
    .set "-g" "${debug}"
    .emits obj as "-o"
  ..
  .output obj "target/out/app.o"
..
```

<a id="45-2-copier-le-pattern"></a>
### 45.2 Copier le pattern

Le but est de garder une structure stable et reconnaissable.

<a id="46-tutoriel-fichiers-generes"></a>
## 46. Tutoriel: fichiers generes

<a id="46-1-outputs-temporaires"></a>
### 46.1 Outputs temporaires

Les outputs intermediaires doivent etre dans `target/out`.

<a id="46-2-nettoyage"></a>
### 46.2 Nettoyage

`steel cache clear` supprime les caches, pas les outputs.
Utiliser un script externe pour nettoyer `target/`.

<a id="46-3-regle-d-or"></a>
### 46.3 Regle d or

Ne pas melanger sources et outputs.

<a id="47-tutoriel-cli-approfondie"></a>
## 47. Tutoriel: CLI approfondie

<a id="47-1-steel-build"></a>
### 47.1 Steel build

```
steel
```

<a id="47-2-steel-print"></a>
### 47.2 Steel print

```
steel print
```

<a id="47-3-steel-check"></a>
### 47.3 Steel check

```
steel check
```

<a id="48-tutoriel-audit-de-config"></a>
## 48. Tutoriel: audit de config

<a id="48-1-verifier-la-coherence"></a>
### 48.1 Verifier la coherence

- outputs dans `target/`
- pas de chemins absolus
- bakes explicites

<a id="48-2-comparer-deux-mff"></a>
### 48.2 Comparer deux MFF

```
diff steelconfig.mff other/steelconfig.mff
```

<a id="48-3-metriques-simples"></a>
### 48.3 Metriques simples

Nombre de bakes, nombre de sources, taille du log.

<a id="49-tutoriel-migration-d-un-build-legacy"></a>
## 49. Tutoriel: migration d un build legacy

<a id="49-1-etapes"></a>
### 49.1 Etapes

1) lister les commandes actuelles
2) en faire des bakes
3) stabiliser les paths

<a id="49-2-exemple-makefile-steel"></a>
### 49.2 Exemple Makefile -> Steel

```
gcc -c src/main.c -o target/out/main.o
gcc target/out/main.o -o target/out/app
```

Devient:
```
[bake compile]
  .make c_src cglob "src/main.c"
  [run cc]
    .takes c_src as "@args"
    .emits obj as "-o"
  ..
  .output obj "target/out/main.o"
..

[bake link]
  .needs compile
  [run cc]
    .set "cmd" "gcc target/out/main.o -o target/out/app"
  ..
  .output exe "target/out/app"
..
```

<a id="50-conclusion-du-guide"></a>
## 50. Conclusion du guide

Steel vise:
- un format declaratif stable
- une separation claire entre configuration et execution
- des artefacts deterministes

Ce guide peut etre etendu en fonction de l evolution du runner et des schemas.

<a id="51-tutoriel-documentation-projet-interne"></a>
## 51. Tutoriel: documentation projet interne

<a id="51-1-documenter-le-build"></a>
### 51.1 Documenter le build

Ajouter un court README:
- comment lancer `steel`
- comment lancer `steel run`
- quels outputs attendre

<a id="51-2-exemples-reproduisibles"></a>
### 51.2 Exemples reproduisibles

Fournir un exemple minimal qui compile sans configuration externe.

<a id="51-3-mettre-a-jour"></a>
### 51.3 Mettre a jour

Synchroniser la doc avec les changements du steelconf.

<a id="52-tutoriel-projets-multi-binaries"></a>
## 52. Tutoriel: projets multi-binaries

<a id="52-1-deux-binaires"></a>
### 52.1 Deux binaires

```
[bake app_one]
  .make c_src cglob "src/app1/*.c"
  [run cc]
    .takes c_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/app_one"
..

[bake app_two]
  .make c_src cglob "src/app2/*.c"
  [run cc]
    .takes c_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/app_two"
..
```

<a id="52-2-execution-ciblee"></a>
### 52.2 Execution ciblee

```
steel run --bake app_one
steel run --bake app_two
```

<a id="52-3-execution-globale"></a>
### 52.3 Execution globale

```
steel run --all
```

<a id="53-tutoriel-organisation-par-dossiers"></a>
## 53. Tutoriel: organisation par dossiers

<a id="53-1-convention-de-root"></a>
### 53.1 Convention de root

Le root doit contenir:
- steelconf
- sources
- dossier target

<a id="53-2-sous-projets"></a>
### 53.2 Sous-projets

Si plusieurs projets cohabitent, preferer un steelconf par projet.

<a id="53-3-eviter-les-chemins-absolus"></a>
### 53.3 Eviter les chemins absolus

Les chemins absolus cassent la portabilite.

<a id="54-tutoriel-traitement-des-assets"></a>
## 54. Tutoriel: traitement des assets

<a id="54-1-copier-des-fichiers"></a>
### 54.1 Copier des fichiers

```
[bake assets]
  .make data cglob "assets/**/*"
  [run sh]
    .set "cmd" "cp -R assets target/out/assets"
  ..
  .output dir "target/out/assets"
..
```

<a id="54-2-assets-par-profile"></a>
### 54.2 Assets par profile

Utiliser une variable:
```
.set assets_dir "assets/${profile}"
```

<a id="54-3-conseils"></a>
### 54.3 Conseils

- garder un dossier `assets/` stable
- eviter les copies inutiles

<a id="55-tutoriel-gestion-des-includes"></a>
## 55. Tutoriel: gestion des includes

<a id="55-1-include-unique"></a>
### 55.1 Include unique

```
.include "include"
```

<a id="55-2-plusieurs-includes"></a>
### 55.2 Plusieurs includes

```
.include "src/lib"
.include "src/util"
```

<a id="55-3-convention"></a>
### 55.3 Convention

Preferer un dossier `include/` commun.

<a id="56-tutoriel-debug-et-traces"></a>
## 56. Tutoriel: debug et traces

<a id="56-1-activer-plus-de-verbosite"></a>
### 56.1 Activer plus de verbosite

Utiliser `-v` sur la CLI.

<a id="56-2-tracer-une-commande"></a>
### 56.2 Tracer une commande

Inspecter le log:
- `cmd`
- `status`

<a id="56-3-reproduire-hors-steel"></a>
### 56.3 Reproduire hors Steel

Copier la commande et l executer manuellement pour isoler l erreur.

<a id="57-tutoriel-compatibilite-os"></a>
## 57. Tutoriel: compatibilite OS

<a id="57-1-windows"></a>
### 57.1 Windows

Penser au suffixe `.exe` et aux paths.

<a id="57-2-macos"></a>
### 57.2 macOS

Attention aux differences de toolchain.

<a id="57-3-linux"></a>
### 57.3 Linux

Le comportement par defaut est teste sur Linux.

<a id="58-tutoriel-conventions-de-logs"></a>
## 58. Tutoriel: conventions de logs

<a id="58-1-stockage"></a>
### 58.1 Stockage

Utiliser `target/logs/` si besoin de plusieurs logs.

<a id="58-2-nommage"></a>
### 58.2 Nommage

```
target/logs/run_debug.mff
target/logs/run_release.mff
```

<a id="58-3-archivage"></a>
### 58.3 Archivage

Archiver les logs avec les artefacts en CI.

<a id="59-tutoriel-verifications-pre-release"></a>
## 59. Tutoriel: verifications pre-release

<a id="59-1-checklist"></a>
### 59.1 Checklist

- build en release
- tests executes
- outputs valides
- version taggee

<a id="59-2-nettoyage"></a>
### 59.2 Nettoyage

Supprimer `target/` avant une release propre.

<a id="59-3-archive-finale"></a>
### 59.3 Archive finale

Archiver `dist/` et `steelconfig.mff`.

<a id="60-annexes-additionnelles"></a>
## 60. Annexes additionnelles

<a id="60-1-exemple-de-structure-finalisee"></a>
### 60.1 Exemple de structure finalisee

```
root/
  steelconf
  src/
  include/
  assets/
  target/
  dist/
```

<a id="60-2-commandes-essentielles"></a>
### 60.2 Commandes essentielles

- `steel`
- `steel run`
- `steel print`
- `steel check`

<a id="60-3-rappel"></a>
### 60.3 Rappel

Le format MUF reste la source, MFF est l artefact resolu.

<a id="61-tutoriel-patterns-de-refactor"></a>
## 61. Tutoriel: patterns de refactor

<a id="61-1-extraire-un-bake-commun"></a>
### 61.1 Extraire un bake commun

Quand plusieurs bakes se ressemblent, creer un bake modele et le copier.

<a id="61-2-renommer-sans-casser"></a>
### 61.2 Renommer sans casser

Renommer les bakes avec prudence:
- mettre a jour `--bake`
- mettre a jour les `.needs`

<a id="61-3-simplifier-les-ports"></a>
### 61.3 Simplifier les ports

Garder des ports courts et explicites.

<a id="62-tutoriel-gestion-des-sources"></a>
## 62. Tutoriel: gestion des sources

<a id="62-1-sources-par-dossier"></a>
### 62.1 Sources par dossier

Decouper les globs par dossier pour garder un controle fin.

<a id="62-2-exclure-des-fichiers"></a>
### 62.2 Exclure des fichiers

Eviter les fichiers generes dans les globs sources.

<a id="62-3-stabilite"></a>
### 62.3 Stabilite

Les listes de fichiers doivent rester stables entre runs.

<a id="63-tutoriel-conventions-de-paths"></a>
## 63. Tutoriel: conventions de paths

<a id="63-1-paths-relatifs"></a>
### 63.1 Paths relatifs

Toujours utiliser des paths relatifs au root.

<a id="63-2-normalisation"></a>
### 63.2 Normalisation

Steel normalise les chemins avec `/`.

<a id="63-3-depots-multiples"></a>
### 63.3 Depots multiples

Si un sous-projet est un submodule, preferer un steelconf separé.

<a id="64-tutoriel-edition-collaborative"></a>
## 64. Tutoriel: edition collaborative

<a id="64-1-revue-de-steelfile"></a>
### 64.1 Revue de steelconf

Considerer le steelconf comme du code critique.

<a id="64-2-historique"></a>
### 64.2 Historique

Les diffs sur MUF sont lisibles et utiles.

<a id="64-3-validation-ci"></a>
### 64.3 Validation CI

Executer `steel check` en CI.

<a id="65-tutoriel-logs-et-audit-long-terme"></a>
## 65. Tutoriel: logs et audit long terme

<a id="65-1-logs-persistants"></a>
### 65.1 Logs persistants

Archiver les logs dans un dossier dedie.

<a id="65-2-comparaison"></a>
### 65.2 Comparaison

Comparer les logs pour detecter des regressions de commandes.

<a id="65-3-tracabilite"></a>
### 65.3 Traçabilite

Associer un log a une version de steelconf.

<a id="66-tutoriel-build-reproductible"></a>
## 66. Tutoriel: build reproductible

<a id="66-1-sources-propres"></a>
### 66.1 Sources propres

Nettoyer `target/` avant un build de reference.

<a id="66-2-versions-fixes"></a>
### 66.2 Versions fixes

Utiliser des toolchains versionnees.

<a id="66-3-mff-comme-preuve"></a>
### 66.3 MFF comme preuve

Archiver `steelconfig.mff` avec l artefact final.

<a id="67-tutoriel-integrer-des-scripts"></a>
## 67. Tutoriel: integrer des scripts

<a id="67-1-utiliser-un-tool-sh"></a>
### 67.1 Utiliser un tool `sh`

```
[tool sh]
  .exec "sh"
..
```

<a id="67-2-bake-script"></a>
### 67.2 Bake script

```
[bake script]
  [run sh]
    .set "cmd" "echo hello > target/out/hello.txt"
  ..
  .output file "target/out/hello.txt"
..
```

<a id="67-3-prudence"></a>
### 67.3 Prudence

Limiter les scripts pour conserver la reproductibilite.

<a id="68-tutoriel-build-multi-arch"></a>
## 68. Tutoriel: build multi-arch

<a id="68-1-deux-targets"></a>
### 68.1 Deux targets

```
steel
steel
```

<a id="68-2-outputs-separes"></a>
### 68.2 Outputs separes

Utiliser `target/${target}` pour separer les artefacts.

<a id="68-3-verifier-les-diffs"></a>
### 68.3 Verifier les diffs

Comparer les `steelconfig.mff` par target.

<a id="69-tutoriel-maintenir-le-guide"></a>
## 69. Tutoriel: maintenir le guide

<a id="69-1-version-du-format"></a>
### 69.1 Version du format

Mettre a jour les sections MUF/MFF si le format evolue.

<a id="69-2-roadmap"></a>
### 69.2 Roadmap

Ajouter des sections quand de nouvelles commandes arrivent.

<a id="69-3-exemples"></a>
### 69.3 Exemples

Garder des exemples courts et valides.

<a id="70-annexes-finales"></a>
## 70. Annexes finales

<a id="70-1-recap-complet"></a>
### 70.1 Recap complet

- MUF: source declarative
- MFF: config resolue
- runner: execution

<a id="70-2-repertoire-docs"></a>
### 70.2 Repertoire docs

Consulter:
- `doc/manifest.md`
- `schemas/README.md`

<a id="70-3-fin"></a>
### 70.3 Fin

Fin du guide et de la reference.

<a id="71-tutoriel-discipline-de-versioning"></a>
## 71. Tutoriel: discipline de versioning

<a id="71-1-versionner-le-steelfile"></a>
### 71.1 Versionner le steelconf

Toujours committer le steelconf avec le code source.

<a id="71-2-versionner-le-mff"></a>
### 71.2 Versionner le MFF

Archiver `steelconfig.mff` pour les builds importants.

<a id="71-3-tagger-les-releases"></a>
### 71.3 Tagger les releases

Associer un tag git a un MFF archivé.

<a id="72-tutoriel-bonnes-pratiques-de-maintenance"></a>
## 72. Tutoriel: bonnes pratiques de maintenance

<a id="72-1-nettoyage-periodique"></a>
### 72.1 Nettoyage periodique

Supprimer les anciens logs et outputs.

<a id="72-2-audit-des-globs"></a>
### 72.2 Audit des globs

Revoir les patterns pour eviter les inclusions accidentelles.

<a id="72-3-revue-des-profiles"></a>
### 72.3 Revue des profiles

Supprimer les profiles obsoletes.

<a id="73-tutoriel-ergonomie-pour-les-nouveaux-arrivants"></a>
## 73. Tutoriel: ergonomie pour les nouveaux arrivants

<a id="73-1-readme-minimal"></a>
### 73.1 README minimal

Documenter:
- commandes principales
- prerequis

<a id="73-2-scripts-helper"></a>
### 73.2 Scripts helper

Ajouter un script `./build.sh` si besoin de simplifier.

<a id="73-3-exemples"></a>
### 73.3 Exemples

Fournir un exemple MUF minimal dans le repo.

<a id="74-tutoriel-verification-croisee"></a>
## 74. Tutoriel: verification croisee

<a id="74-1-comparer-config-et-run"></a>
### 74.1 Comparer config et run

Verifier que le run correspond a la configuration resolue.

<a id="74-2-verifier-les-chemins"></a>
### 74.2 Verifier les chemins

Eviter les divergences entre `target/` et `dist/`.

<a id="74-3-consistance-des-tools"></a>
### 74.3 Consistance des tools

Utiliser les memes versions de tools en local et CI.

<a id="75-tutoriel-build-offline"></a>
## 75. Tutoriel: build offline

<a id="75-1-mode-offline"></a>
### 75.1 Mode offline

Utiliser `--offline` pour eviter les acces reseau.

<a id="75-2-tools-locaux"></a>
### 75.2 Tools locaux

Verifier que tout est present localement.

<a id="75-3-logs"></a>
### 75.3 Logs

Archiver les logs pour audit.

<a id="76-tutoriel-path-mapping"></a>
## 76. Tutoriel: path mapping

<a id="76-1-root-stable"></a>
### 76.1 Root stable

Garder un root stable facilite les diffs.

<a id="76-2-paths-relatifs"></a>
### 76.2 Paths relatifs

Toujours preferer des paths relatifs au root.

<a id="76-3-normalisation"></a>
### 76.3 Normalisation

Eviter les separateurs OS dans les fichiers MUF.

<a id="77-tutoriel-gestion-des-flags"></a>
## 77. Tutoriel: gestion des flags

<a id="77-1-flags-par-profile"></a>
### 77.1 Flags par profile

Conserver les flags dans les profiles:
```
[profile debug]
  .set cflags "-O0 -g"
..
```

<a id="77-2-utilisation"></a>
### 77.2 Utilisation

```
.set "${cflags}" 1
```

<a id="77-3-lisibilite"></a>
### 77.3 Lisibilite

Eviter des lignes de flags trop longues.

<a id="78-tutoriel-modularite"></a>
## 78. Tutoriel: modularite

<a id="78-1-decoupage-logique"></a>
### 78.1 Decoupage logique

Un bake par responsabilite.

<a id="78-2-reutilisation"></a>
### 78.2 Reutilisation

Copier les patterns pour garder une structure uniforme.

<a id="78-3-evolution"></a>
### 78.3 Evolution

Ajouter des bakes sans casser les existants.

<a id="79-tutoriel-compatibilite-future"></a>
## 79. Tutoriel: compatibilite future

<a id="79-1-garder-le-header"></a>
### 79.1 Garder le header

Toujours `!muf 4` en tete.

<a id="79-2-anticiper-les-changements"></a>
### 79.2 Anticiper les changements

Lire la roadmap et adapter les exemples.

<a id="79-3-compatibilite-backward"></a>
### 79.3 Compatibilite backward

Eviter les changements cassants dans les conventions internes.

<a id="80-annexes-supplementaires-fin"></a>
## 80. Annexes supplementaires (fin)

<a id="80-1-commandes-resumees"></a>
### 80.1 Commandes resumees

- build: `steel`
- run: `steel run --bake app`
- print: `steel print`

<a id="80-2-artefacts"></a>
### 80.2 Artefacts

- `steelconfig.mff`
- logs runner
- outputs `target/out`

<a id="80-3-dernier-rappel"></a>
### 80.3 Dernier rappel

Steel privilegie la stabilite et la lisibilite des configurations.
