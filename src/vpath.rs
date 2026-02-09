//! vpath.rs
//!
//! Abstraction “Virtual Path” pour Steel:
//! - Manipulation de chemins indépendants de l’OS (séparateur '/')
//! - Normalisation (., .., //, trailing slash)
//! - Gestion de “root” logique (absolute vs relative)
//! - Conversion contrôlée vers std::path::PathBuf (host path)
//!
//! Conçu pour:
//! - steelconf / manifests (paths déclaratifs)
//! - résolution de workspaces / stores / capsules
//! - hashing stable de chemins (cache, fingerprints)
//!
//! Dépendances: std uniquement.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};

/// Séparateur canonique des chemins virtuels.
pub const VSEP: char = '/';

/// Erreurs de parsing/validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VPathError {
    Empty,
    InvalidChar { ch: char },
    InvalidNul,
    InvalidDrivePrefix,
    AboveRoot,
}

impl fmt::Display for VPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VPathError::Empty => write!(f, "empty vpath"),
            VPathError::InvalidChar { ch } => write!(f, "invalid char in vpath: {:?}", ch),
            VPathError::InvalidNul => write!(f, "nul byte in vpath"),
            VPathError::InvalidDrivePrefix => write!(f, "windows drive prefix not allowed in vpath"),
            VPathError::AboveRoot => write!(f, "vpath normalization would go above root"),
        }
    }
}

impl std::error::Error for VPathError {}

/// Représentation canonique d’un chemin virtuel.
/// - Séparateur interne: '/'
/// - Pas de segments vides (pas de `//`)
/// - Pas de `.`
/// - `..` résolu si possible (sinon erreur si absolute)
/// - Absolute si commence par `/`
#[derive(Clone)]
pub struct VPath {
    // Invariant: toujours normalisé.
    inner: String,
}

impl VPath {
    /// Parse + normalise.
    pub fn parse(s: impl AsRef<str>) -> Result<Self, VPathError> {
        let s = s.as_ref();
        if s.is_empty() {
            return Err(VPathError::Empty);
        }
        if s.bytes().any(|b| b == 0) {
            return Err(VPathError::InvalidNul);
        }

        // Interdire les préfixes Windows style "C:\"
        if looks_like_windows_drive(s) {
            return Err(VPathError::InvalidDrivePrefix);
        }

        // Validation des chars (permissif mais safe)
        for ch in s.chars() {
            if ch == '\\' {
                // Force l’usage de '/'
                return Err(VPathError::InvalidChar { ch });
            }
            // Interdit quelques contrôles (sauf tab/spaces: acceptés)
            if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' {
                return Err(VPathError::InvalidChar { ch });
            }
        }

        Ok(Self {
            inner: normalize_vpath(s)?,
        })
    }

    /// Constructeur “unchecked” si vous garantissez déjà l’invariant.
    pub fn from_normalized_unchecked(s: impl Into<String>) -> Self {
        Self { inner: s.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with(VSEP)
    }

    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Renvoie "/" si absolute root, sinon "." si relative vide (mais on ne stocke jamais vide).
    pub fn is_root(&self) -> bool {
        self.inner == "/"
    }

    /// Nombre de segments (hors racine).
    pub fn len_segments(&self) -> usize {
        self.segments().count()
    }

    /// Itérateur sur les segments (sans segments vides).
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        let s = self.as_str();
        let s = s.strip_prefix('/').unwrap_or(s);
        s.split('/').filter(|p| !p.is_empty())
    }

    /// Parent (None si root, ou si relative en 1 segment).
    pub fn parent(&self) -> Option<VPath> {
        if self.is_root() {
            return None;
        }

        let abs = self.is_absolute();
        let mut segs: Vec<&str> = self.segments().collect();
        if segs.is_empty() {
            return None;
        }
        segs.pop();

        if abs {
            if segs.is_empty() {
                return Some(VPath::from_normalized_unchecked("/".to_string()));
            }
            Some(VPath::from_normalized_unchecked(format!("/{}", segs.join("/"))))
        } else {
            if segs.is_empty() {
                None
            } else {
                Some(VPath::from_normalized_unchecked(segs.join("/")))
            }
        }
    }

    /// Nom de fichier (dernier segment), None pour root.
    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            return None;
        }
        self.segments().last()
    }

    /// Extension du dernier segment (sans le '.').
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name()?;
        let (base, ext) = split_ext(name);
        let _ = base;
        ext
    }

    /// Ajoute un segment (ou un chemin relatif) à la fin.
    /// - si rhs est absolute => retourne rhs normalisé.
    pub fn join(&self, rhs: &VPath) -> Result<VPath, VPathError> {
        if rhs.is_absolute() {
            return Ok(rhs.clone());
        }
        if self.is_root() {
            return VPath::parse(format!("/{}", rhs.as_str()));
        }
        if self.is_absolute() {
            VPath::parse(format!("{}/{}", self.as_str().trim_end_matches('/'), rhs.as_str()))
        } else {
            VPath::parse(format!("{}/{}", self.as_str().trim_end_matches('/'), rhs.as_str()))
        }
    }

    /// Join “string” côté RHS (chemin relatif attendu).
    pub fn join_str(&self, rhs: impl AsRef<str>) -> Result<VPath, VPathError> {
        let rhs = VPath::parse(rhs.as_ref())?;
        self.join(&rhs)
    }

    /// Rend un chemin relatif à `base` si possible.
    /// Exemple:
    /// - base=/a/b, self=/a/b/c => "c"
    /// - base=/a/b, self=/a/d => None
    pub fn strip_prefix(&self, base: &VPath) -> Option<VPath> {
        if self.is_absolute() != base.is_absolute() {
            return None;
        }
        let a: Vec<&str> = self.segments().collect();
        let b: Vec<&str> = base.segments().collect();

        if b.len() > a.len() {
            return None;
        }
        if a[..b.len()] != b[..] {
            return None;
        }
        let rest = &a[b.len()..];
        if rest.is_empty() {
            return Some(VPath::from_normalized_unchecked(".".to_string()));
        }
        Some(VPath::from_normalized_unchecked(rest.join("/")))
    }

    /// Convertit en PathBuf “host”.
    ///
    /// Règles:
    /// - Un VPath absolute n’est pas forcément absolu côté OS: on l’ancre sur `root`.
    /// - Un VPath relative est résolu sous `cwd`.
    ///
    /// Ceci évite qu’un manifest puisse adresser arbitrairement le FS de la machine
    /// sans passer par une racine contrôlée (store/capsule/workspace).
    pub fn to_host_path(&self, root: &Path, cwd: &Path) -> PathBuf {
        let mut out = PathBuf::new();

        if self.is_absolute() {
            out.push(root);
            for seg in self.segments() {
                out.push(seg);
            }
        } else {
            out.push(cwd);
            for seg in self.segments() {
                out.push(seg);
            }
        }

        out
    }

    /// Convertit un chemin OS en VPath (normalisé), en tentant de produire une forme stable.
    /// - Si le path est absolu, produit un vpath absolu (avec `/`).
    /// - Sinon, produit un vpath relatif.
    ///
    /// Note: sur Windows, les préfixes type `C:\` sont acceptés en input mais encodés sans drive
    /// uniquement si `allow_windows_drive = true` et en perdant le drive (usage interne rare).
    pub fn from_host_path(path: &Path, allow_windows_drive: bool) -> Result<VPath, VPathError> {
        // Convertit les components en segments UTF-8 lossless si possible.
        let mut segs: Vec<String> = Vec::new();
        let mut is_abs = path.is_absolute();

        for c in path.components() {
            match c {
                Component::Prefix(p) => {
                    if !allow_windows_drive {
                        return Err(VPathError::InvalidDrivePrefix);
                    }
                    // ignore le prefix drive pour stabilité (optionnel)
                    let _ = p;
                    is_abs = true; // on traite comme absolu.
                }
                Component::RootDir => {
                    is_abs = true;
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    segs.push("..".to_string());
                }
                Component::Normal(os) => {
                    segs.push(os.to_string_lossy().to_string());
                }
            }
        }

        let s = if is_abs {
            format!("/{}", segs.join("/"))
        } else {
            segs.join("/")
        };
        VPath::parse(s)
    }

    /// Normalise explicitement (utile si vous construisez via concat).
    pub fn normalize(&self) -> Result<VPath, VPathError> {
        VPath::parse(self.as_str())
    }

    /// Renvoie une version “display stable”:
    /// - absolute => commence par '/'
    /// - relative => tel quel
    pub fn display(&self) -> &str {
        self.as_str()
    }

    /// Hash stable (sur la représentation normalisée).
    pub fn stable_hash64(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        self.inner.hash(&mut h);
        h.finish()
    }
}

impl fmt::Debug for VPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VPath").field(&self.inner).finish()
    }
}

impl fmt::Display for VPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq for VPath {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl Eq for VPath {}

impl PartialOrd for VPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.inner.cmp(&other.inner))
    }
}
impl Ord for VPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl Hash for VPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

/// Représentation “part” (un segment).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VPart(Cow<'static, str>);

impl VPart {
    pub fn parse(s: impl AsRef<str>) -> Result<Self, VPathError> {
        let s = s.as_ref();
        if s.is_empty() {
            return Err(VPathError::Empty);
        }
        if s.contains('/') || s.contains('\\') {
            return Err(VPathError::InvalidChar { ch: '/' });
        }
        if s.bytes().any(|b| b == 0) {
            return Err(VPathError::InvalidNul);
        }
        if s == "." || s == ".." {
            return Err(VPathError::InvalidChar { ch: '.' });
        }
        Ok(Self(Cow::Owned(s.to_string())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/* =================
 * Normalization
 * ================= */

fn looks_like_windows_drive(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        let a = b[0];
        let is_alpha = (b'A'..=b'Z').contains(&a) || (b'a'..=b'z').contains(&a);
        return is_alpha;
    }
    false
}

fn normalize_vpath(input: &str) -> Result<String, VPathError> {
    // Accept "." en tant que relative current dir
    if input == "." {
        return Ok(".".to_string());
    }

    let abs = input.starts_with('/');

    // Split manuel pour gérer // et trailing slash.
    let mut parts: Vec<&str> = Vec::new();
    for raw in input.split('/') {
        if raw.is_empty() {
            continue;
        }
        parts.push(raw);
    }

    let mut stack: Vec<&str> = Vec::new();
    for p in parts {
        if p == "." {
            continue;
        }
        if p == ".." {
            if let Some(last) = stack.pop() {
                let _ = last;
                continue;
            }
            // stack vide
            if abs {
                return Err(VPathError::AboveRoot);
            } else {
                // Relative: on garde .. en tête (ex: "../../a")
                stack.push("..");
            }
            continue;
        }
        stack.push(p);
    }

    if abs {
        if stack.is_empty() {
            return Ok("/".to_string());
        }
        Ok(format!("/{}", stack.join("/")))
    } else {
        if stack.is_empty() {
            // Canonique: "." plutôt que vide.
            Ok(".".to_string())
        } else {
            Ok(stack.join("/"))
        }
    }
}

/* =========
 * Utils
 * ========= */

fn split_ext(name: &str) -> (&str, Option<&str>) {
    // "a.b" => ("a", Some("b")), "a" => ("a", None), ".gitignore" => (".gitignore", None)
    let mut iter = name.rsplitn(2, '.');
    let last = iter.next().unwrap_or(name);
    let before = iter.next();
    if before.is_none() {
        return (name, None);
    }
    let before = before.unwrap();
    if before.is_empty() {
        // leading dot file
        (name, None)
    } else {
        (before, Some(last))
    }
}

/* =====================
 * Convenience API
 * ===================== */

/// Résolution de chemin virtuel avec base (base doit être absolute).
/// - Si `p` est absolute => renvoie p.
/// - Si `p` est relative => base.join(p)
pub fn resolve_under(base_abs: &VPath, p: &VPath) -> Result<VPath, VPathError> {
    if !base_abs.is_absolute() {
        // Choix: on pourrait panic, mais c’est une API de lib.
        return Err(VPathError::InvalidChar { ch: ':' });
    }
    if p.is_absolute() {
        Ok(p.clone())
    } else {
        base_abs.join(p)
    }
}

/// Combine root/cwd et retourne un PathBuf host.
pub fn resolve_host(root: &Path, cwd: &Path, p: &VPath) -> PathBuf {
    p.to_host_path(root, cwd)
}

/* =====================
 * Tests (std only)
 * ===================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_absolute_root() {
        let p = VPath::parse("/").unwrap();
        assert!(p.is_absolute());
        assert!(p.is_root());
        assert_eq!(p.as_str(), "/");
    }

    #[test]
    fn parse_relative_dot() {
        let p = VPath::parse(".").unwrap();
        assert!(p.is_relative());
        assert_eq!(p.as_str(), ".");
    }

    #[test]
    fn normalize_basic() {
        let p = VPath::parse("a/./b//c").unwrap();
        assert_eq!(p.as_str(), "a/b/c");
    }

    #[test]
    fn normalize_parent_relative_kept() {
        let p = VPath::parse("../a").unwrap();
        assert_eq!(p.as_str(), "../a");
    }

    #[test]
    fn normalize_parent_absolute_fails_above_root() {
        let err = VPath::parse("/../a").unwrap_err();
        assert_eq!(err, VPathError::AboveRoot);
    }

    #[test]
    fn join_relative() {
        let base = VPath::parse("/a/b").unwrap();
        let rhs = VPath::parse("c/d").unwrap();
        let j = base.join(&rhs).unwrap();
        assert_eq!(j.as_str(), "/a/b/c/d");
    }

    #[test]
    fn join_absolute_rhs_wins() {
        let base = VPath::parse("/a/b").unwrap();
        let rhs = VPath::parse("/x").unwrap();
        let j = base.join(&rhs).unwrap();
        assert_eq!(j.as_str(), "/x");
    }

    #[test]
    fn strip_prefix_ok() {
        let base = VPath::parse("/a/b").unwrap();
        let p = VPath::parse("/a/b/c/d").unwrap();
        let rel = p.strip_prefix(&base).unwrap();
        assert_eq!(rel.as_str(), "c/d");
    }

    #[test]
    fn strip_prefix_none() {
        let base = VPath::parse("/a/b").unwrap();
        let p = VPath::parse("/a/x").unwrap();
        assert!(p.strip_prefix(&base).is_none());
    }

    #[test]
    fn to_host_path_anchors_absolute() {
        let root = Path::new("/sandbox/root");
        let cwd = Path::new("/sandbox/cwd");
        let p = VPath::parse("/a/b").unwrap();
        let hp = p.to_host_path(root, cwd);
        assert!(hp.to_string_lossy().contains("sandbox"));
    }

    #[test]
    fn extension_logic() {
        let p = VPath::parse("a/b/file.tar.gz").unwrap();
        assert_eq!(p.extension(), Some("gz"));
        let q = VPath::parse(".gitignore").unwrap();
        assert_eq!(q.extension(), None);
    }
}
