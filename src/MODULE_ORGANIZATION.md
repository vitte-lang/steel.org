// Module Organization Map
//
// This file provides a visual map of the Steel source tree and how modules
// are organized according to the architecture.

/*

SOURCE TREE ORGANIZATION
========================

src/
├── lib.rs                          # Module declarations and re-exports
├── main.vit                        # Vitte DSL CLI skeleton
├── bin/
│   └── steel.rs                   # Rust CLI entrypoint
│
├── PARSER (lexical & syntactic analysis)
│   ├── arscan.rs                   # Lexer/tokenizer
│   ├── read.rs                     # Block parser
│   └── loadapi.rs                  # Main API (parse, resolve, emit)
│
├── VALIDATOR (coherence & constraints)
│   ├── config.rs                   # Global config validation
│   ├── dependancies.rs             # Dependency graph, references
│   └── target_file.rs              # Target spec validation
│
├── RESOLVER (profile inheritance, variables, implicit rules)
│   ├── variable.rs                 # Variable interpolation & scope
│   ├── expand.rs                   # Macro expansion
│   ├── implicit.rs                 # Implicit rule resolution
│   └── default.rs                  # Default values
│
├── GENERATOR (Steelconfig.mcfg serialization)
│   ├── interface.rs                # Runtime abstraction (I/O)
│   └── output.rs                   # Serialization & exports
│
├── MODEL (core data structures)
│   ├── steelint.rs                # Workspace, Package, Profile, Target, Toolchain
│   ├── def_target_file.rs          # Target file definitions
│   └── rule.rs                     # Rule model
│
├── RUNTIME (OS, process, debugging)
│   ├── os.rs                       # OS abstractions
│   ├── posixos.rs                  # POSIX layer
│   ├── job.rs                      # Process management
│   └── debug.rs                    # Debugging & logging
│
├── CLI (command-line interface)
│   ├── commands.rs                 # Command implementations
│   └── (interface.rs already in GENERATOR)
│
├── UTILS (utilities)
│   ├── misc.rs                     # Miscellaneous
│   ├── hash.rs                     # Hash utilities
│   ├── strcache.rs                 # String cache (optimization)
│   ├── directory.rs                # Directory utilities
│   └── vpath.rs                    # Virtual path resolution
│
├── METADATA (version & warnings)
│   ├── version.rs                  # Version info
│   └── warning.rs                  # Warning utilities
│
├── BUILD (build orchestration)
│   └── build_muf.rs                # "build steel" command
│
└── PLATFORM (platform-specific)
    ├── vms_exit.rs
    ├── vms_export_symbol.rs
    ├── vms_progname.rs
    ├── vmsdir.rs
    ├── vmsfunctions.rs
    ├── vmsify.rs
    ├── vmsjobs.rs
    ├── remote_cstms.rs
    └── remote_stub.rs


PIPELINE VIEW
=============

SteelConfig (input)
    ↓
[PARSER: arscan + read]
    → Tokens & AST
    ↓
[VALIDATOR: config + dependancies + target_file]
    → Coherence validated
    ↓
[RESOLVER: default + variable + expand + implicit]
    → Configuration resolved
    ↓
[GENERATOR: output + interface]
    → Steelconfig.mcfg (frozen config artifact)
    ↓
Execution runner (consumes build artifact)


DEPENDENCY GRAPH (simplified)
=============================

Commands
  ├─→ interface (runtime)
  ├─→ loadapi (main API)
  │     ├─→ parser (read, arscan)
  │     ├─→ validator (config, dependancies, target_file)
  │     ├─→ resolver (variable, expand, implicit, default)
  │     └─→ generator (output, interface)
  └─→ model (steelint, def_target_file, rule)

Interface (runtime abstraction)
  ├─→ os (OS-specific)
  │     ├─→ posixos (POSIX layer)
  │     └─→ job (process mgmt)
  └─→ debug (logging)

Utils
  ├─→ misc
  ├─→ hash
  ├─→ strcache
  ├─→ directory
  └─→ vpath

Platform (VMS, remote execution)
  └─→ (mostly independent, conditionally compiled)


ADDING NEW FUNCTIONALITY
========================

New Command?
  1. Implement in commands.rs
  2. Add variant to CLI enum
  3. Router in src/bin/steel.rs

New Target Type?
  1. Extend model::Target in steelint.rs
  2. Add implicit rules in implicit.rs
  3. Validate in validator::target_file

New OS/Architecture?
  1. Implement OS trait in runtime::os.rs
  2. Platform-specific code in platform/
  3. Router in interface.rs by context

New Variable/Interpolation?
  1. Extend resolver::variable
  2. Add expansion logic in resolver::expand
  3. Test in validator::dependancies

*/
