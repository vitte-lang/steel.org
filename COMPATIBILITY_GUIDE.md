# Guide Pratique : Compatibilité Cross-Platform

## Démarrage rapide

### Compiler pour la plateforme courante

```bash
# Build standard (moderne, optimisé)
cargo build --release

# Build legacy (vieux OS, plus compatible)
cargo build --release --features legacy

# Build full (toutes les optimisations)
cargo build --release --features full
```

### Tester la compatibilité

```bash
# Tests OS compatibility
cargo test --test os_compatibility --all-features

# Tests avec mode fallback
MUFFIN_FALLBACK=1 cargo test

# Tests legacy
cargo test --features legacy
```

## Intégration dans le code

### Détecter les capacités du système

```rust
use steel::os::{get_current_os, OsTier};

fn main() {
    let os = get_current_os();
    
    println!("OS: {}", os.name());
    println!("Tier: {:?}", os.tier());
    println!("Parallel jobs: {}", os.supports_parallel_jobs());
    
    match os.tier() {
        OsTier::Legacy => println!("Running on legacy OS"),
        OsTier::Compatible => println!("Compatible OS"),
        OsTier::Modern => println!("Modern OS"),
        OsTier::Current => println!("Latest OS"),
    }
}
```

### Adapter le comportement au système

```rust
use steel::os::get_current_os;

fn build_with_optimal_parallelism() {
    let os = get_current_os();
    
    let job_count = if os.supports_parallel_jobs() {
        os.cpu_count()
    } else {
        1  // Fallback to sequential on legacy systems
    };
    
    println!("Using {} parallel jobs", job_count);
}
```

### Utiliser les chemins correctement

```rust
use steel::os::get_current_os;
use std::path::PathBuf;

fn construct_path(parts: &[&str]) -> PathBuf {
    let os = get_current_os();
    let separator = os.path_separator();
    
    // On Windows: C:\Users\Name\steel
    // On Unix:    /home/name/steel
    let path_str = parts.join(&separator.to_string());
    PathBuf::from(path_str)
}
```

### Gestion sécurisée des fichiers

```rust
use steel::os::get_current_os;
use std::fs;
use std::path::Path;

fn safe_link(src: &Path, dst: &Path) -> anyhow::Result<()> {
    let os = get_current_os();
    
    if os.symlink_support() {
        #[cfg(unix)]
        fs::soft_link(src, dst)?;
        
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(src, dst)?;
    } else if os.hardlink_support() {
        fs::hard_link(src, dst)?;
    } else {
        // Fallback: copy
        fs::copy(src, dst)?;
    }
    
    Ok(())
}
```

### Gestion des variables d'environnement

```rust
use steel::os::get_current_os;

fn setup_build_env() {
    let os = get_current_os();
    
    // Get configuration
    let cc = os.get_env("CC").unwrap_or_else(|| "gcc".to_string());
    let cflags = os.get_env("CFLAGS").unwrap_or_else(|| "-O2".to_string());
    
    // Adapt to system capabilities
    if os.supports_parallel_jobs() {
        os.set_env("MAKEFLAGS", &format!("-j{}", os.cpu_count()))
            .expect("Should set MAKEFLAGS");
    }
}
```

## Détection avancée

### Vérifier les capacités spécifiques

```rust
use steel::os::get_current_os;

fn configure_features() {
    let os = get_current_os();
    
    // Cache: prefer parallel on modern systems
    if os.supports_parallel_jobs() {
        // Enable parallel caching
    }
    
    // Symlinks: use on modern systems
    if os.symlink_support() {
        // Use symlinks for build artifacts
    }
    
    // Unicode: assume supported on modern systems
    if os.tier() >= steel::os::OsTier::Compatible {
        // Allow unicode in paths
    }
}
```

### Diagnostics et logging

```rust
use steel::os::get_current_os;
use log::info;

fn log_system_info() {
    let os = get_current_os();
    
    info!("System: {}", os.diagnostic_info());
    info!("Temp dir: {}", os.temp_dir().display());
    info!("Cache dir: {}", os.cache_dir().display());
    info!("Available CPUs: {}", os.cpu_count());
}
```

## Gérer les modes fallback

### Mode fallback automatique

```rust
use steel::os::{get_current_os, PureRustFallback};

fn get_safe_adapter() -> Box<dyn steel::os::OsAdapter> {
    match get_current_os().spawn_process("test", &["-f", "/tmp"]) {
        Ok(_) => get_current_os(),
        Err(_) => {
            eprintln!("Native OS adapter failed, using fallback");
            Box::new(PureRustFallback)
        }
    }
}
```

### Fallback explicite

```rust
use steel::os::{get_current_os, PureRustFallback};

fn use_fallback_mode() {
    let fallback = PureRustFallback;
    
    // Fallback guarantees:
    assert!(!fallback.symlink_support());       // Safe: no symlinks
    assert!(!fallback.supports_parallel_jobs()); // Safe: single-threaded
    assert_eq!(fallback.tier(), steel::os::OsTier::Legacy); // Conservative
    
    // Use fallback for maximum compatibility
}
```

## Patterns courants

### Pattern 1 : Dégradation gracieuse

```rust
fn process_with_optimization() {
    let os = get_current_os();
    
    if os.tier() >= steel::os::OsTier::Modern {
        // Use advanced features
        process_parallel()
    } else if os.tier() >= steel::os::OsTier::Compatible {
        // Use moderate features
        process_threaded()
    } else {
        // Fallback to safe mode
        process_sequential()
    }
}
```

### Pattern 2 : Capacités conditionnelles

```rust
use std::collections::HashMap;

fn get_feature_flags() -> HashMap<&'static str, bool> {
    let os = get_current_os();
    let mut flags = HashMap::new();
    
    flags.insert("parallel", os.supports_parallel_jobs());
    flags.insert("symlinks", os.symlink_support());
    flags.insert("unicode", os.tier() >= steel::os::OsTier::Compatible);
    flags.insert("long_paths", os.tier() >= steel::os::OsTier::Modern);
    
    flags
}
```

### Pattern 3 : Configuration adaptative

```rust
struct BuildConfig {
    parallel_jobs: usize,
    use_cache: bool,
    use_symlinks: bool,
}

fn create_config() -> BuildConfig {
    let os = get_current_os();
    
    BuildConfig {
        parallel_jobs: if os.supports_parallel_jobs() {
            os.cpu_count()
        } else {
            1
        },
        use_cache: os.tier() >= steel::os::OsTier::Modern,
        use_symlinks: os.symlink_support(),
    }
}
```

## Troubleshooting

### Problème : Les tests échouent sur un vieux système

**Solution** :
```bash
# Compiler avec support legacy
cargo test --features legacy

# Ou utiliser le fallback
MUFFIN_FALLBACK=1 cargo test
```

### Problème : Pas de parallélisation sur un système

**Solution** :
```rust
// Vérifier les capacités
let os = get_current_os();
if !os.supports_parallel_jobs() {
    eprintln!("Warning: Parallel jobs not supported on {}", os.name());
    // Fallback to sequential
}
```

### Problème : Symlinks ne fonctionnent pas

**Solution** :
```rust
// Vérifier le support
if !os.symlink_support() {
    // Use hardlinks or copy instead
    fs::copy(src, dst)?;
}
```

## Ressources

- [Documentation complète](CROSS_PLATFORM_COMPATIBILITY.md)
- [Architecture interne](ARCHITECTURE.md)
- [Tests d'intégrité](tests/os_compatibility.rs)
- [API OS Adapter](src/os.rs)
- [Détection de versions](src/posixos.rs)
