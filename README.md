# Steel

![Steel](https://img.shields.io/badge/Steel-config-orange)

Steel est la couche de configuration **declarative** du build. Il **lit**, **valide** et **resout** un workspace (packages, profils, toolchains, targets), puis **genere un artefact stable** (`steel.log` / `steelconfig.mff`). Vitte consomme cet artefact pour appliquer les regles de build et executer les etapes de compilation de facon deterministe.


## Points forts

- Configuration declarative, separee de l execution.
- Artefact canonique (`steel.log` / `steelconfig.mff`) pour audit et CI.
- Résolution déterministe et sorties faciles a outiller.
- Introspection integree (`print`, `graph`, `why`) pour diagnostiquer.
- Multi-OS/arch avec profils explicites.
- Mode dev et overrides via `-D KEY=VALUE` sans modifier le fichier.

## Configuration déclarative (exemple)

Un `steelconf` décrit explicitement le **workspace**, les **profils**, les **targets** et la **toolchain**. Pas de regles ad-hoc : la structure reste lisible, composable et stable.

```text
!muf 4

[workspace]
  .set name "demo"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set opt 0
  .set debug 1
..

[profile release]
  .set opt 2
  .set debug 0
..

[target x86_64-apple-darwin]
  .set os "macos"
  .set arch "x86_64"
..

[tool gcc]
  .exec "gcc"
..

[tool ar]
  .exec "ar"
..
```

## CLI (raccourci)

Commandes (details dans `doc/manifest.md`, liste rapide: `doc/manifest.md#liste-rapide-commandes`, flags: `doc/manifest.md#flags-frequents`):
- [`steel`](doc/manifest.md#cmd-build-steel) (equivalent de `steel build steelconf`)
- [`build steelconf`](doc/manifest.md#cmd-build-steel)
- [`run`](doc/manifest.md#cmd-run)
- [`doctor`](doc/manifest.md#cmd-doctor)
- [`cache`](doc/manifest.md#cmd-cache)
- [`graph`](doc/manifest.md#cmd-graph)
- [`fmt`](doc/manifest.md#cmd-fmt)
- [`version`/`help`](doc/manifest.md#cmd-aide-version)

### Flags frequents

- `steel` (comme `steel build steelconf`) n accepte aucun flag: tout est dans `steelconf`.

Exemples:
```text
steel run 
```

## Steel Editor (steecleditor)

Steel fournit un editeur terminal integre pour `steelconf` : **steecleditor**.

Lancer:
```text
steel editor
steel editor path/to/steelconf
```

Fonctions principales:
- Autocompletion steelconf (blocs/directives + snippets, type VSCode).
- Indentation intelligente, auto-close des crochets.
- Recherche, aller a une ligne, remplacement.
- Tabs multi-fichiers, recent files, session restore.
- Diff rapide vs disque et mini-map.
- Syntax highlight (steelconf + C/C++/Python/Java/etc).
- Autosave optionnel, mode lecture seule, themes.

Raccourcis utiles:
- `Ctrl+S` save, `Ctrl+O` open, `Ctrl+Q` quit
- `Ctrl+F` search, `F3` next match, `Ctrl+L` go to line
- `Ctrl+P` find file, `F2` recent files
- `Ctrl+R` steel run, `Ctrl+Shift+E` jump run error
- `Ctrl+Shift+G` glob preview, `Ctrl+Shift+I` insert snippet


## SteelLib: import OCaml

Le backend OCaml est exposé via SteelLib. Exemple d'import:

```rust
use SteelLib::ocaml::{OcamlArgs, OcamlDriver, OcamlSpec};
```

## Voir aussi

- `doc/manifest.md#liste-rapide-commandes`
- `doc/manifest.md#flags-frequents`
- `doc/manifest.md#cli-complete`

## Uniformisation totale (langages + machines)

Steel vise une **uniformisation totale** du build : meme modele, memes commandes et memes sorties logiques, quel que soit le langage (Vitte, C/C++, C#, Rust, …) et l’environnement (machines anciennes ou recentes, OS/arch heterogenes).

- **Langage-agnostique** : integration via des **tools declaratifs** (compile/link/archive/test/package), connectes dans un graphe typé.
- **Machine-agnostique** : execution pilotee par des **targets** (triples OS/arch) et des politiques stables (paths normalises, cache, sandbox).
- **Contrat unique** : la configuration est gelee dans `steel.log` et consommee ensuite de maniere deterministe.
- **Reproductibilite** : cache content-addressed + empreinte toolchain + policy capsule.
- **Observabilite** : diagnostics et introspection (`print`, `-why`, `-graph`) pour outiller CI, IDE et scripts.

L’objectif est de pouvoir orchestrer des projets **mono-langage** comme des projets **mixtes**, sur des environnements modernes comme plus anciens, sans divergence de modele ni de workflow.

## Pipeline recommandé

Le pipeline est scinde en deux phases : **Configuration** puis **Construction**.

   **Configuration** — `build steel`
   - Charge la config (workspace/packages/profils/targets/toolchains)
   - Valide la coherence (contraintes, chemins, compatibilites)
   - Resout les valeurs (defaults, heritages, overrides)
   - **Emet** `steel.log` (artefact canonique)


## Architecture



### Détection de reconstruction (incrémental)

Au cœur du pipeline, **Steel** calcule automatiquement **ce qui doit être reconstruit**.



#### Configuration gelée et artefacts

Lors de la phase de configuration, `steel` / `Steel` peut **générer un fichier `.mff`** (configuration gelée) destiné à une compilation globale. Cette configuration est ensuite utilisée pour produire des artefacts binaires Vitte :


```text
Steel build main.muff
```

Le build génère les sorties dans un répertoire d’artefacts **par dossier** (par convention `./.steel/` ou `./.muff/` selon la configuration), tant qu’aucune erreur n’est détectée pendant la compilation.

#### Projets multi-outils

Le buildfile reste générique : Steel est capable d’orchestrer des projets Vitte et des projets mixtes via des **tools** déclaratifs (par exemple pour C/C++/C#/Rust), tant que les toolchains et les étapes (compile/link/archive/package) sont décrites de manière explicite.

### Flux de traitement

```
┌─────────────────────────────────────────────────────────────┐
│                    PHASE CONFIGURATION                      │
│                      (build steel)                         │
└─────────────────────────────────────────────────────────────┘

1. CHARGEMENT
   ├── steelconf (workspace + packages + profils)
   ├── Toolchains (compilateurs, linkers, outils)
   └── Targets (spécifications de build)

2. VALIDATION
   ├── Syntaxe des fichiers
   ├── Références (packages, targets, profils existants)
   ├── Compatibilités (OS/arch, versions, dépendances)
   └── Contraintes (chemins, permissions, etc.)

3. RÉSOLUTION
   ├── Application des profils et héritage
   ├── Résolution des overrides
   ├── Interpolation des variables
   ├── Calcul des empreintes toolchain (fingerprints)
   └── Résolution des dépendances transitives

4. ÉMISSION
   └── steel.log (configuration gelée et normalisée)

┌─────────────────────────────────────────────────────────────┐
│                    PHASE CONSTRUCTION                       │
│                      (build vitte)                          │
└─────────────────────────────────────────────────────────────┘

Vitte lit steel.log et :
  ├── Construit le DAG des étapes
  ├── Résout les dépendances de fichiers
  ├── Exécute l'ordre topologique
  └── Gère l'incrémental et le cache
```

### Modèle de données

**Workspace** (conteneur global)
- nom, racine, profils, targets, packages

**Package** (unité compilable)
- nom, version, kind (bin/lib/test/doc), dépendances
- répertoires source, chemins d'inclusion, etc.

**Profile** (ensemble d'options)
- optimisation, debug, features, variables spécifiques
- inheritance (profils dérivés)

**Target** (objectif de build)
- nom, kind (binary/library/test), sources associées
- règles implicites (dépend du kind et du language)

**Toolchain** (ensemble d'outils)
- compilateur, linker, archiveur
- flags par défaut, environnement, version

### Responsabilités de chaque couche

**Steel** (Déclaratif)
- *Ce qu'on veut construire*, *comment* (profils), *pour qui* (targets)
- Séparation claire entre configuration et exécution
- Validation et cohérence

**Vitte** (Exécutif)
- *Comment y parvenir* : graphe de tâches, parallélisation, cache
- Isolation des détails d'exécution (compilateur, flags réels)
- Diagnostics de performance et débogage

**steel.log** (Contrat)
- Configuration **gelée** et **normalisée**
- Contient tout ce que Vitte doit savoir pour construire
- Invalidation automatique du cache en cas de changement

### Rôle de Vitte (orchestration de build)

Vitte orchestre la phase **construction** à partir de la configuration gelée :

- Définition des **targets** et des **étapes** (transformations `inputs → outputs`)
- Construction d’un **graphe de dépendances** (DAG)
- Exécution **déterministe** (ordre topologique) avec **incrémental** et **cache**
- Diagnostics outillables : « pourquoi ça rebuild ? », « qui dépend de quoi ? »

### Contrat `steelconf`

`steelconf` contient une configuration **normalisée** et **explicite** (plus d’implicite côté build). Exemples de champs attendus :

- version de schéma (`! muf4`),
- host/target (OS/arch/triple),
- profil sélectionné,
- chemins (root/build/dist/cache),
- toolchain + fingerprint (invalidation cache),
- inputs: liste explicite ou regroupement par glob,
- targets résolues et options associées,
- dépendances transitives résolues,
- variables d'environnement interpellées.

## Commandes

### Build

```text
steel run[<plan>] [flags] [-- <args>]
```

- `steel run steelconfig` : exécute le **plan par défaut** (ex: `default`).
- `steel run steelconfig` <plan>` : exécute un **plan nommé** (ex: `release`, `ci`, `package`).

#### Flags de build

- `-all` : construit tous les outputs **exportables** (ou tout le workspace selon policy).
- `-debug` : force le profil `debug`.
- `-release` : force le profil `release`.
- `-clean` : purge cache + artefacts (garde-fou conseillé : `--yes`).
- `-j <n>` : parallélisme du scheduler (ex: `-j 16`).
- `-watch` : mode dev (rebuild incrémental sur événements FS).
- `-why <artifact|ref>` : explique la chaîne de dépendances + la cause d’invalidation.
- `-graph[=text|dot|json]` : dump du DAG (format selon implémentation).
- `-D KEY=VALUE` : overrides de variables (répétable).

Exemples :

```text
steel build steelconf
steel fmt steelconf
steel doctor steelconf
steel graph 
steel ninja
steel cache
steel toolchain
steel editor 
steel editor-setup
steel help
steel version
```

### Validation / émission

- `check` : valide la configuration sans exécuter la construction.
  - options usuelles : `--profile <name>`, `--target <name>`, `--emit <path>`
- `resolve` : résout et **génère** `steel.log` (équivalent fonctionnel à `build steel` côté configuration).
  - options usuelles : `--emit <path>`, `--profile <name>`, `--target <name>`

Exemples :

```text
check
check --profile debug
resolve --emit ./steel.log
```

### Introspection

- `print <scope>` : affiche une vue résolue (format texte/JSON selon implémentation).
  - scopes typiques : `workspace`, `packages`, `targets`, `profiles`, `toolchains`, `vars`, `plans`, `exports`
- `graph [--format <text|dot|json>]` : export du graphe (sans build).
- `why <artifact|ref>` : diagnostic « pourquoi ça rebuild ? » (alias possible de `build steel -why`).

Exemples :

```text
print workspace
print targets
graph --format dot
why vittec_driver::vittec
```

### Décompilation (audit de build)

Steel expose une commande de **décompilation** orientée audit : elle permet de relire un artefact de configuration gelée **`.mff`** (ou un buildfile **`.muff`**) et de reconstituer une vue complète du projet : architecture, graphe, inputs/outputs, outils utilisés et paramètres.

- `decompile <project.mff>` : affiche l’architecture du build (DAG), la liste des fichiers, les ports, les toolchains et l’ensemble des valeurs résolues.
- `decompile <main.muff>` : affiche la configuration et les règles telles qu’elles seront gelées (vue normalisée), sans exécuter la construction.

Exemples :

```text
steel decompile projet.mff
steel decompile src/module/main.muff
steel decompile projet.mff --format json
steel decompile projet.mff --graph dot
```

Un fichier **`.mff`** enregistre la configuration **normalisée** et la **trace de compilation** (inputs, globs développés, règles, outils, arguments, empreintes). Il garantit une configuration uniforme sur toutes les machines.

Un buildfile **`.muff`** (ou un ensemble de `main.muff`) peut également être décompilé sur n’importe quelle machine pour retrouver :

- les listes d’inputs (fichiers, globs),
- les dépendances et l’ordre d’exécution,
- les bibliothèques/plugins référencés,
- les sorties attendues.

Limite : la reconstruction effective des binaires dépend de la **disponibilité** et de la **compatibilité** des toolchains (langages compilés, versions, targets). Steel fournit la description complète ; la machine doit disposer des compilateurs/outils compatibles pour reproduire les artefacts.

### Maintenance

- `fmt [<file>]` : formate un fichier Steel.
- `clean` : purge cache + artefacts (alias possible de `build steel -clean`).
- `cache <cmd>` : gestion du store/cache.
  - `cache stats` : statistiques.
  - `cache gc` : nettoyage.
- `doctor` : diagnostic environnement (outils, droits, chemins, sandbox).

Exemples :

```text
fmt
clean
cache stats
cache gc
doctor
```

### Divers

- `help [<cmd>]` : aide globale ou par sous-commande.
- `version` : version de Steel.

### Options globales (toutes commandes)

- `-v / --verbose` : sortie détaillée.
- `-q / --quiet` : sortie réduite.

### Exemples multi-OS (Linux / macOS / Windows / BSD / Solaris)

> Objectif : exécuter les commandes `build steel ...` de façon identique sur toutes les plateformes. Les étapes ci-dessous installent uniquement les utilitaires de base et supposent que `steel` et `vitte` sont disponibles dans le projet (ou dans le `PATH`).

#### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install -y git ca-certificates curl

# build
build steel
build steel -all
build steel -debug
```

#### Linux (Fedora/RHEL)

```bash
sudo dnf install -y git ca-certificates curl

build steel
build steel -release -j 16
```

#### Linux (Arch)

```bash
sudo pacman -S --needed git ca-certificates curl

build steel -all
```

#### macOS (Homebrew)

```bash
brew install git

build steel
build steel -watch
```

#### Windows (PowerShell)

```powershell
# Prérequis (au choix)
# winget install --id Git.Git -e
# winget install --id OpenJS.NodeJS.LTS -e   # si ton projet en a besoin

# Exécution
build steel
build steel -all
build steel -why app.exe
```

Notes Windows :
- Si `build` n’est pas une commande globale, exécuter depuis la racine du projet via `./steel` (si présent) ou ajouter le binaire au `PATH`.
- En CI Windows, privilégier `--color never` et `--json` pour l’outillage.

#### FreeBSD

```sh
sudo pkg install -y git ca_root_nss

build steel
build steel -graph=dot
```

#### OpenBSD

```sh
doas pkg_add git

build steel
```

#### NetBSD

```sh
# pkgin (si configuré)
# sudo pkgin install git ca-certificates

build steel
```

#### Solaris / illumos (OpenIndiana)

```sh
pfexec pkg install developer/versioning/git

build steel
build steel -release
```

Conseils cross-OS :
- Utiliser des chemins relatifs dans les buildfiles (`./dist`, `./.steel/cache`) pour limiter les divergences.
- Si un fichier `steel` est un script, vérifier le bit exécutable (`chmod +x steel`) et utiliser des fins de lignes LF.
- Pour l’outillage, préférer `steel print ... --json`, `steel graph --format ...`, `steel why ...`.

## Langage MUF (v4.1)

Le format MUF est versionné. La version actuelle est **v4.1**.

- Header : `!muf 4`
- Blocs : `[tag name?] ... ..`
- Directives : `.op arg1 arg2 ...`
- Commentaires : `;; ...`
- Grammaire EBNF : `assets/grammar/steel.ebnf`

## Format du fichier Steel (aperçu)

Exemple MUF v4.1 (blocs + `..`) :

```text
!muf 4

[workspace]
  .set name "vitte"
  .set root "."
..

[package hello]
  .set kind "bin"
  .set version "0.1.0"
  .set src_dir "src"
..

[profile debug]
  .set opt 0
  .set debug 1
..

[default]
  .set target "hello"
..
```

## Format `steel.log` (apercu)

```text
mff 1

[host]
  os "linux"
  arch "x86_64"
..

profile "debug"

[target]
  name "syntax_smoke"
  kind "test"
..

[paths]
  root "/path/to/repo"
  dist "dist"
..
```

## Variables d'environnement

- `MUFFIN_FILE` : chemin du fichier Steel.
- `MUFFIN_PROFILE` : profil par défaut.
- `MUFFIN_EMIT` : chemin par défaut de `steel.log`.
- `MUFFIN_OFFLINE` : active le mode offline.
- `VITTE_BUILD` : chemin explicite vers le driver `build`.
- `PATH` : s’assurer que `steel` et `vitte` sont accessibles (ou utiliser `./steel` depuis la racine).

## Fichiers

- `steel` ou `steelconf` : configuration principale.
- `*.vitte` : sources du projet.
- `steel.log` : configuration gelée et normalisée (artefact canonique) + trace outillable (graph, inputs, outils, empreintes), consommée par Vitte.
- `*.muf` : buildfiles (si le projet segmente la configuration par dossier/workspace).
- `*.muff` : configurations segmentées par répertoire (optionnel, générées ou maintenues selon le mode).
- `steel/main.muff` : point d’ancrage de configuration/build (nom configurable, optionnel).
- Artefacts de build (selon targets/plateformes) : `*.vo` (Vitte compilation), `*.va` (Vitte librairie statique), `*.o` / `*.obj` (C/C++ objets), `*.a` / `*.lib` (archives statiques), `*.so` / `*.dylib` / `*.dll` (librairies partagées), `*.exe` (exécutables Windows).

## Documentation

- [docs/quickstart.md](docs/quickstart.md) — Quickstart MUF v4.1
- [docs/cookbook.md](docs/cookbook.md) — Recettes (C, lib+app, multi-tool)
- [docs/migration.md](docs/migration.md) — Migration vers MUF v4.1
- [docs/faq-erreurs.md](docs/faq-erreurs.md) — FAQ erreurs (codes + fixes)
- [docs/observabilite.md](docs/observabilite.md) — Observabilite (doctor, cache, logs)
- [docs/troubleshooting.md](docs/troubleshooting.md) — Troubleshooting (guide long)
- [docs/reference/formats/index.md](docs/reference/formats/index.md) — Formats + versioning
- [ARCHITECTURE.md](ARCHITECTURE.md) — Détails internes, pipeline, modules
- [src/MODULE_ORGANIZATION.md](src/MODULE_ORGANIZATION.md) — Organisation des fichiers source
- Manpage: [doc/steel.1](doc/steel.1)

## Licence

Voir `COPYING`.

# Steel

![Steel](https://img.shields.io/badge/Steel-config-orange)

Steel est le **compilateur de configuration** du build Vitte.

- **Entrée** : un seul fichier à la racine du dépôt : **`steelconf`**.
- **Sortie** : un **binaire universel** de configuration **`steelconf.mub`** (Universal Binary Config), identique d’une machine à l’autre.
- **Consommateur** : **Vitte** lit `steelconf.mub` pour exécuter le build (DAG, cache, incrémental) de manière déterministe.

L’objectif : **un modèle unique**, **un workflow unique**, **un artefact unique**.

---

## Pourquoi un binaire universel ?

Le build a besoin d’une configuration **gelée** (plus d’implicites) :

- toutes les valeurs résolues (profils, targets, toolchains, variables),
- la liste normalisée des inputs (globs développés),
- l’empreinte toolchain/policy pour l’invalidation cache,
- un graphe exécutable (ports `inputs → outputs`).

`steelconf.mub` est cette barrière contractuelle entre :

- **déclaratif** (Steel) : ce qu’on veut construire,
- **exécutable** (Vitte) : comment on le construit.

---

## Pipeline

1) **Configurer (freeze)**

```text
build steel
```

- lit `steelconf`
- valide + résout
- émet `steelconf.mub`

2) **Construire (build)**

```text
build vitte
```

- lit `steelconf.mub`
- exécute le DAG (compile/link/test/package)
- gère incrémental + cache

---

## Commandes

### Configuration

```text
build steel [flags]
```

Flags usuels :

- `-debug` / `-release`
- `-j <n>` : parallélisme
- `-D KEY=VALUE` : override sans modifier `steelconf`
- `-why <ref>` : diagnostic d’invalidation
- `-graph[=text|dot|json]` : export graphe
- `-watch` : mode dev

### Introspection

```text
steel print <scope>
steel graph [--format <text|dot|json>]
steel why <ref>
```

---

## Format du fichier `steelconf` (aperçu)

Le fichier est orienté blocs, lisible, et conçu pour être **résolu** puis **gelé**.

```text
!muf 4

[workspace]
  .set name "vitte"
  .set root "."
..

[profile debug]
  .set opt 0
  .set debug 1
..

[profile release]
  .set opt 3
  .set debug 0
..

[toolchain host]
  .set cc "clang"
  .set ar "llvm-ar"
  .set ld "clang"
..

[target host]
  .set profile "release"
  ;; étapes déclaratives (exemple minimal)
  .set step_compile "tool=cc;inputs=src/**/*.c;outputs=build/**/*.o"
  .set step_link "tool=ld;inputs=build/**/*.o;outputs=out/bin/vittec"
..

[default]
  .set target "host"
..
```

---

## Fichiers

- **`steelconf`** : source de configuration (unique, à la racine).
- **`steelconf.mub`** : binaire universel de configuration gelée (artefact canonique).
- Artefacts Vitte (selon targets) : `*.vo`, `*.va`, exécutables, etc.

---

## Notes de compatibilité

- Le binaire `steelconf.mub` est conçu pour être **portable** (endianness/versions gérées par en-tête + schéma).
- Pour outiller CI/IDE, Steel peut exposer en plus des exports `--json`/`--dot`, mais **Vitte ne dépend que de `steelconf.mub`**.

---

## Licence

Voir `COPYING`.
# Steel

![Steel](https://img.shields.io/badge/Steel-config-orange)

Steel est la **configuration déclarative** du build Vitte.

- **Entrée** : un seul fichier à la racine du dépôt : **`steelconf`**.
- **Sortie** : un dossier **`target/`** créé à la racine (artefacts + configuration résolue).
- **Consommateur** : **Vitte** lit la configuration résolue depuis `target/` et exécute le build de manière déterministe.

---

## Principe

Le workflow est volontairement en 2 phases :

1) **Configurer (freeze)**

```text
build steel
```

- parse + valide `steelconf`
- résout profiles/targets/toolchains/variables
- **matérialise** une configuration stable dans `target/`

2) **Construire (build)**

```text
build vitte
```

- lit la config résolue depuis `target/`
- exécute le DAG (compile/link/test/package)
- gère incrémental + cache

---

## Layout généré dans `target/`

Par défaut, Steel crée (ou met à jour) une arborescence standard :

```text
target/
  steel/
    config.mub
    graph.json
    fingerprints.json
  build/
    <triple>/
      <profile>/
        obj/
        gen/
  out/
    <triple>/
      <profile>/
        bin/
        lib/
  cache/
    cas/
    meta/
```

Notes :

- `target/steel/config.mub` : **binaire universel** de configuration résolue (portable, versionné).
- `target/steel/graph.json` : export outillable du DAG (CI/IDE).
- `target/steel/fingerprints.json` : empreintes toolchain/policy pour l’invalidation.

---

## Commandes

### Configuration

```text
build steel [flags]
```

Flags usuels :

- `-debug` / `-release`
- `-j <n>` : parallélisme
- `-D KEY=VALUE` : override sans modifier `steelconf`
- `-why <ref>` : diagnostic d’invalidation
- `-graph[=text|dot|json]` : export graphe
- `-watch` : mode dev

### Introspection

```text
steel print <scope>
steel graph [--format <text|dot|json>]
steel why <ref>
```

---

## Format minimal de `steelconf` (aperçu)

```text
!muf 4

[workspace]
  .set name "vitte"
  .set root "."
  .set target_dir "target"
..

[profile debug]
  .set opt 0
  .set debug 1
..

[profile release]
  .set opt 3
  .set debug 0
..

[toolchain host]
  .set cc "clang"
  .set ar "llvm-ar"
  .set ld "clang"
..

[target host]
  .set profile "release"
  .set step_compile "tool=cc;inputs=src/**/*.c;outputs=target/build/${TRIPLE}/${PROFILE}/obj/**/*.o"
  .set step_link "tool=ld;inputs=target/build/${TRIPLE}/${PROFILE}/obj/**/*.o;outputs=target/out/${TRIPLE}/${PROFILE}/bin/vittec"
..

[default]
  .set target "host"
..
```

---

## Fichiers

- **`steelconf`** : source de configuration (unique).
- **`target/`** : **racine canonique** de toutes les sorties (config résolue + build + cache + outputs).

---

## Licence

Voir `COPYING`.
# Steel

![Steel](https://img.shields.io/badge/Steel-config-orange)

Steel est la couche de configuration **déclarative** du build Vitte.

- **Entrée** : un seul fichier à la racine du dépôt : **`steelconf`**.
- **Sortie** : un dossier **`target/`** créé à la racine (config résolue + build + cache + outputs).
- **Contrat universel** : `target/steel/config.mub` (**binaire universel** de configuration résolue).
- **Consommateur** : **Vitte** lit `target/steel/config.mub` et exécute le build de manière déterministe.

---

## Pipeline

1) **Configurer (freeze)**

```text
build steel
```

- parse + valide `steelconf`
- résout profiles/targets/toolchains/variables
- matérialise la configuration stable dans `target/steel/config.mub`

2) **Construire (build)**

```text
build vitte
```

- lit `target/steel/config.mub`
- exécute le DAG (compile/link/test/package)
- gère incrémental + cache

---

## Layout par défaut dans `target/`

```text
target/
  steel/
    config.mub
    graph.json
    fingerprints.json
  build/
    <triple>/
      <profile>/
        obj/
        gen/
  out/
    <triple>/
      <profile>/
        bin/
        lib/
  cache/
    cas/
    meta/
```

Notes :

- `config.mub` : binaire universel (portable + versionné).
- `graph.json` : export outillable du DAG (CI/IDE).
- `fingerprints.json` : empreintes toolchain/policy pour l’invalidation.

---

## Commandes

### Configuration

```text
build steel [flags]
```

Flags usuels :

- `-debug` / `-release`
- `-j <n>` : parallélisme
- `-D KEY=VALUE` : override sans modifier `steelconf`
- `-why <ref>` : diagnostic d’invalidation
- `-graph[=text|dot|json]` : export graphe
- `-watch` : mode dev

### Introspection

```text
steel print <scope>
steel graph [--format <text|dot|json>]
steel why <ref>
```

---

## Format minimal de `steelconf` (aperçu)

```text
!muf 4

[workspace]
  .set name "vitte"
  .set root "."
  .set target_dir "target"
..

[profile debug]
  .set opt 0
  .set debug 1
..

[profile release]
  .set opt 3
  .set debug 0
..

[toolchain host]
  .set cc "clang"
  .set ar "llvm-ar"
  .set ld "clang"
..

[target host]
  .set profile "release"
  .set step_compile "tool=cc;inputs=src/**/*.c;outputs=target/build/${TRIPLE}/${PROFILE}/obj/**/*.o"
  .set step_link "tool=ld;inputs=target/build/${TRIPLE}/${PROFILE}/obj/**/*.o;outputs=target/out/${TRIPLE}/${PROFILE}/bin/vittec"
..

[default]
  .set target "host"
..
```

---

## Fichiers

- **`steelconf`** : source de configuration (unique).
- **`target/`** : racine canonique de toutes les sorties.
- **`target/steel/config.mub`** : contrat universel consommé par Vitte.

---

## Licence

Voir `COPYING`.
