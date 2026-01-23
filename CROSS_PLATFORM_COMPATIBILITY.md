# Compatibilité Cross-Platform : Recommandations

## Vue d'ensemble

Steel doit supporter :
- **Anciens OS** : Windows XP/Vista, macOS 10.9+, CentOS 6, RHEL 7
- **Nouveaux OS** : Windows 11, macOS 13+, Ubuntu 22.04+, Fedora 38+
- **Architectures** : x86_64, ARM64, i686, PowerPC

## 1. Configuration Rust (Cargo.toml)

### Minimum Supported Rust Version (MSRV)

```toml
[package]
rust-version = "1.63"  # Stable: Jan 2023, compatible 2+ ans arrière
```

### Features conditionnelles

```toml
[dependencies]
# Core (tous les OS)
clap = { version = "4.4", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }

# Optimisation mémoire pour vieux systèmes
parking_lot = { version = "0.12", optional = true }
# Sur vieux OS, utiliser std::sync::Mutex (pas parking_lot)

# Platform-spécifique
[target.'cfg(unix)'.dependencies]
nix = { version = "0.27", features = ["process"] }
libc = "0.2"

[target.'cfg(windows)'.dependencies]
winapi = { version = "0.3", features = ["processthreadsapi", "fileapi"] }
windows = { version = "0.48", features = ["Win32_Foundation"] }

[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.25"  # Optional, pour accès APIs natives

# Logging adaptatif
[dependencies]
log = "0.4"
env_logger = "0.11"  # Fallback universel
fern = { version = "0.6", optional = true }  # Avancé, optionnel

[features]
default = []
modern = ["parking_lot"]  # Moderne
legacy = []  # Vieux systèmes
full = ["modern", "advanced-logging"]
```

## 2. Architecture des modules OS

### Structure recommandée

```
src/
├── os/
│   ├── mod.rs                 # Trait OS (abstraction)
│   ├── unix.rs                # POSIX (Linux, macOS, BSD)
│   ├── windows.rs             # Windows 7+
│   ├── windows_legacy.rs      # Windows XP/Vista (WinXP API)
│   ├── macos_legacy.rs        # macOS 10.9-10.14
│   ├── linux_legacy.rs        # CentOS 6, RHEL 7
│   └── fallback.rs            # Pure Rust fallback
├── platform/
│   ├── detection.rs           # Runtime OS/version detection
│   └── features.rs            # Feature detection
```

### Trait OS abstrait

```rust
// src/os/mod.rs
pub trait OsAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> (u32, u32);  // major, minor
    fn arch(&self) -> Architecture;
    
    // File operations
    fn path_separator(&self) -> char;
    fn symlink_support(&self) -> bool;
    fn hardlink_support(&self) -> bool;
    
    // Process management
    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child>;
    fn cpu_count(&self) -> usize;
    
    // Environment
    fn get_env(&self, key: &str) -> Option<String>;
    fn set_env(&self, key: &str, value: &str) -> Result<()>;
    
    // System-specific
    fn temp_dir(&self) -> &Path;
    fn cache_dir(&self) -> &Path;
    fn supports_parallel_jobs(&self) -> bool;
    
    // Fallback: pure Rust si native indisponible
    fn fallback_enabled(&self) -> bool;
}
```

## 3. Gestion des versions OS

### Detection runtime

```rust
// src/platform/detection.rs
#[derive(Debug, Clone, Copy)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub fn detect_os_version() -> Result<OsVersion> {
    #[cfg(target_os = "windows")]
    {
        detect_windows_version()
    }
    #[cfg(target_os = "macos")]
    {
        detect_macos_version()
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux_version()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(anyhow::anyhow!("Unsupported OS"))
    }
}

pub enum OsTier {
    Legacy,      // Windows XP, macOS 10.9, CentOS 6
    Compatible,  // Windows 7+, macOS 10.14+, Ubuntu 16.04+
    Modern,      // Windows 10+, macOS 11+, Ubuntu 20.04+
    Current,     // Windows 11, macOS 13+, Ubuntu 22.04+
}

pub fn classify_os(version: OsVersion) -> OsTier {
    // Logique de classification
}
```

## 4. Dépendances adaptatives

### Stratégie

```toml
# Dépendances avec options de compatibilité

# Windows: support WinXP via WinAPI layer
[target.'cfg(all(windows, not(feature = "legacy")))'.dependencies]
windows-latest = "0.48"

[target.'cfg(all(windows, feature = "legacy"))'.dependencies]
winapi = "0.3"  # Plus compatible, API stable

# Unix: utiliser libc plutôt que nix si legacy
[target.'cfg(all(unix, not(feature = "legacy")))'.dependencies]
nix = { version = "0.27", features = ["process"] }

[target.'cfg(all(unix, feature = "legacy"))'.dependencies]
libc = "0.2"  # Pure FFI, très stable
```

### Code adaptatif

```rust
// src/os/mod.rs
#[cfg(all(windows, not(feature = "legacy")))]
use windows_latest as win;

#[cfg(all(windows, feature = "legacy"))]
use winapi as win;

pub fn get_current_os() -> Box<dyn OsAdapter> {
    let version = detect_os_version().unwrap_or_default();
    
    match std::env::consts::OS {
        "windows" => {
            if version.major < 10 {
                Box::new(crate::os::windows_legacy::WindowsLegacy)
            } else {
                Box::new(crate::os::windows::WindowsModern)
            }
        }
        "macos" => {
            if version.major < 10 || (version.major == 10 && version.minor < 14) {
                Box::new(crate::os::macos_legacy::MacOSLegacy)
            } else {
                Box::new(crate::os::unix::UnixModern)
            }
        }
        "linux" => {
            // Détecter distro et version
            if is_rhel_6_or_centos_6(version) {
                Box::new(crate::os::linux_legacy::LinuxLegacy)
            } else {
                Box::new(crate::os::unix::UnixModern)
            }
        }
        _ => Box::new(crate::os::fallback::PureRustFallback),
    }
}
```

## 5. Fallback Mode (Mode dégradé)

### Approche pure Rust

Pour les systèmes ou architectures exotiques, fournir un fallback :

```rust
// src/os/fallback.rs
pub struct PureRustFallback;

impl OsAdapter for PureRustFallback {
    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child> {
        // Utiliser std::process::Command (universel)
        std::process::Command::new(cmd)
            .args(args)
            .spawn()
            .map_err(Into::into)
    }
    
    fn symlink_support(&self) -> bool {
        // Fallback: pas de symlinks (safer)
        false
    }
    
    fn hardlink_support(&self) -> bool {
        // Fallback: copie au lieu de hardlinks
        false
    }
    
    fn supports_parallel_jobs(&self) -> bool {
        // Fallback: single-threaded
        false
    }
}
```

## 6. Tests de compatibilité

### Strategy

```toml
# Cargo.toml
[dev-dependencies]
rstest = "0.18"  # Parametrized tests
mockito = "1.2"  # Mocking HTTP/system calls

[[test]]
name = "os_compatibility"
path = "tests/os_compatibility.rs"
required-features = ["testing"]
```

### Exemples de tests

```rust
// tests/os_compatibility.rs
#[cfg(test)]
mod tests {
    use steel::os::*;
    use steel::platform::detection::*;

    #[test]
    fn test_os_detection() {
        let version = detect_os_version().unwrap();
        assert!(version.major > 0);
    }

    #[test]
    fn test_os_adapter_available() {
        let adapter = get_current_os();
        assert!(!adapter.name().is_empty());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_path_separator() {
        let adapter = get_current_os();
        assert_eq!(adapter.path_separator(), '\\');
    }

    #[test]
    #[cfg(unix)]
    fn test_unix_path_separator() {
        let adapter = get_current_os();
        assert_eq!(adapter.path_separator(), '/');
    }

    #[test]
    fn test_fallback_works() {
        // Vérifier que mode fallback n'échoue pas
        let fallback = PureRustFallback;
        let temp = fallback.temp_dir();
        assert!(!temp.as_os_str().is_empty());
    }
}
```

## 7. Documentation pour utilisateurs

### README section

```markdown
## Compatibilité OS

### Support garanti

| OS | Versions | Statut |
|---|---|---|
| **Windows** | 7, 8, 10, 11 | ✅ Production |
| **Windows Legacy** | XP, Vista | ⚠️ Limited (feature: legacy) |
| **macOS** | 10.14+ | ✅ Production |
| **macOS Legacy** | 10.9-10.13 | ⚠️ Limited (feature: legacy) |
| **Linux** | Ubuntu 16.04+, Fedora 32+, CentOS 7+ | ✅ Production |
| **Linux Legacy** | CentOS 6, RHEL 7 | ⚠️ Limited (feature: legacy) |

### Construire pour legacy OS

```bash
# Avec support legacy
cargo build --release --features legacy

# Avec support modern optimisé
cargo build --release --features modern
```

### Minimal system requirements

- Rust 1.63+ (MSRV)
- CPU: x86_64 (i686 supporté avec limitations)
- RAM: 128 MB minimum
- Disk: 50 MB
```

## 8. CI/CD pour compatibilité

### GitHub Actions

```yaml
# .github/workflows/compatibility.yml
name: OS Compatibility

on: [push, pull_request]

jobs:
  test-legacy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --features legacy
      - run: cargo build --release --features legacy

  test-modern:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --features modern
      - run: cargo build --release

  test-fallback:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      # Forcer fallback
      - run: MUFFIN_FALLBACK=1 cargo test
```

## 9. Checklist d'implémentation

- [ ] Créer `src/os/mod.rs` avec trait `OsAdapter`
- [ ] Implémenter `src/os/unix.rs` (POSIX générique)
- [ ] Implémenter `src/os/windows.rs` (Modern Windows)
- [ ] Implémenter `src/os/fallback.rs` (Pure Rust)
- [ ] Ajouter `src/platform/detection.rs` pour OS detection
- [ ] Ajouter features `legacy`, `modern` dans Cargo.toml
- [ ] Créer tests d'intégrité `tests/os_compatibility.rs`
- [ ] Documenter dans README
- [ ] Ajouter CI/CD checks

## 10. Dépendances recommandées par tier

### Tier 1 (Core - tous OS)
- `anyhow`, `thiserror` — Error handling
- `log` — Logging (stable)
- `serde` — Serialization

### Tier 2 (Modern)
- `nix` — POSIX advanced features
- `windows` — Modern Windows API
- `tokio` — Async (optionnel)

### Tier 3 (Legacy)
- `libc` — C FFI (très stable)
- `winapi` — Older Windows API
- Pure Rust alternatives

### Tier 4 (Fallback)
- `std` only (fichiers, process, env)

