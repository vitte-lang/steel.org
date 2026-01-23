# Architecture interne - Steel

## Vue d'ensemble

Steel est organisé en **modules fonctionnels** selon le pipeline de configuration :

```
INPUT (SteelConfig, toolchains, targets)
  ↓
[PARSER] → tokens/AST
  ↓
[VALIDATOR] → coherence check
  ↓
[RESOLVER] → resolved configuration
  ↓
[GENERATOR] → Steelconfig.mcfg
  ↓
OUTPUT (consommé par le runner)
```

## Modules par fonction

### Parser (`parser::*`)
- **arscan.rs** — Lexer/tokenizer, analyse lexicale
- **read.rs** — Parser bloc-orienté (.end terminator)
- **loadapi.rs** — Main public API (parse, resolve, emit)

### Validator (`validator::*`)
- **config.rs** — Cohérence globale de la configuration
- **dependancies.rs** — Graphe de dépendances, résolution de références
- **target_file.rs** — Validation des spécifications de targets

### Resolver (`resolver::*`)
- **variable.rs** — Interpolation et scope des variables
- **expand.rs** — Expansion des macros et variables
- **implicit.rs** — Résolution des règles implicites
- **default.rs** — Application des valeurs par défaut

### Generator (`generator::*`)
- **interface.rs** — Abstraction runtime (I/O, filesystem)
- **output.rs** — Sérialisation en Steelconfig.mcfg, export graphes

### Model (`model::*`)
- **steelint.rs** — Internal data structures (Workspace, Package, Profile, Target, Toolchain)
- **def_target_file.rs** — Target file definitions
- **rule.rs** — Rule model and implicit rules

### Runtime (`runtime::*`)
- **os.rs** — Abstractions OS (cross-platform)
- **posixos.rs** — Compliance layer POSIX
- **job.rs** — Job/process management
- **debug.rs** — Debug utilities, logging

### CLI (`cli::*`)
- **commands.rs** — Implémentations des commandes (help, fmt, check, resolve, print, graph)
- **interface.rs** — Interface CLI

### Utils (`utils::*`)
- **misc.rs** — Utilitaires divers
- **hash.rs** — Utilitaires hash
- **strcache.rs** — Cache de chaînes (optimisation)
- **directory.rs** — Utilitaires répertoires
- **vpath.rs** — Résolution chemins virtuels

### Metadata (`metadata::*`)
- **version.rs** — Information version
- **warning.rs** — Utilitaires warning

### Platform (`platform::*`)
- **vms_*** — Implémentations spécifiques VMS
- **remote_*** — Execution remoto/stub
- **posixos.rs** — Layer POSIX

## Flux de données

### Phase 1 : Configuration (build steel)

```
1. LOAD
   SteelConfig → (arscan) → tokens
                 ↓
   tokens → (read) → AST (Workspace, Packages, Profiles, Targets)

2. VALIDATE
   AST → (config) → ✓ global coherence
         ↓
         (dependancies) → ✓ references, graph
         ↓
         (target_file) → ✓ target specs

3. RESOLVE
   AST → (default) → with defaults
         ↓
         (variable) → resolved vars
         ↓
         (expand) → interpolated
         ↓
         (implicit) → resolved rules
         ↓
         ConfigResolved

4. GENERATE
   ConfigResolved → (output) → Steelconfig.mcfg
                               + exports (DOT, JSON, etc.)
```

### Phase 2 : Construction (execution)

Le runner lit Steelconfig.mcfg et :
- Construit le DAG des règles
- Résout les dépendances de fichiers
- Exécute l'ordre topologique
- Gère l'incrémental et le cache

## Points clés

### Séparation Déclaratif/Exécutif

- **Steel** (déclaratif) : *Ce qu'on veut construire*
  - Config normalisée et validée
  - Pas de détails d'exécution (pas de flags compilateur réels, pas d'ordre d'exécution)

- **Runner** (exécutif) : *Comment y parvenir*
  - DAG, parallélisation, cache
  - Détails d'exécution isolés

### Steelconfig.mcfg comme contrat

- Configuration **gelée** (immutable après génération)
- **Normalisée** (plus d'ambiguïté)
- **Explicite** (pas d'implicite côté exécution)
- **Fingerprint** pour invalidation cache

### Determinisme

- Résolution stable et reproductible
- Sorting explicite des listes
- Variables d'environnement gelées
- Pas d'accès au filesystem durant résolution

## Dépendances externes

- `clap` — Argument parsing
- `serde/serde_json/toml` — Serialization
- `regex` — Pattern matching
- `walkdir` — Filesystem traversal
- `anyhow/thiserror` — Error handling

## Conventions de code

### Nommage des modules

- Utiliser `snake_case` pour les modules
- Un fichier par module
- Grouper les modules connexes via re-export dans `lib.rs`

### Erreurs

- Utiliser `anyhow::Result<T>` pour résultats publics
- Custom errors via `thiserror` pour la logique interne
- Messages d'erreur informatifs (contexte, suggestion)

### Tests

- Tests unitaires à côté du code (mod tests)
- Tests d'intégration dans `tests/`
- Snapshots pour output complexes (insta)

## Points d'extension

### Ajouter une nouvelle commande

1. Implémenter dans `commands.rs`
2. Ajouter enum variant dans CLI
3. Router dans `src/bin/steel.rs`

### Ajouter un nouveau type de target

1. Étendre `model::Target`
2. Ajouter règles implicites dans `implicit.rs`
3. Valider dans `validator::*`

### Supporter un nouvel OS/arch

1. Implémenter trait OS dans `runtime::os.rs`
2. Ajouter implémentation spécifique dans `platform::*`
3. Router dans `interface.rs` selon contexte
