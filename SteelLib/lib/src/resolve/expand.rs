// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\resolve\expand.rs

use crate::error::SteelError;
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::{Component, Path, PathBuf},
};

/// Resolver context for interpolation / expansion.
#[derive(Debug, Clone, Default)]
pub struct ExpandCtx {
    /// Variables available for `${KEY}` expansion.
    ///
    /// Determinism rule: prefer BTreeMap for stable iteration.
    pub vars: BTreeMap<String, String>,

    /// Root directory for resolving relative paths.
    pub root: PathBuf,

    /// Target triple (optional) used by `${TRIPLE}`.
    pub triple: Option<String>,

    /// Selected profile (optional) used by `${PROFILE}`.
    pub profile: Option<String>,
}

impl ExpandCtx {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            vars: BTreeMap::new(),
            root: root.into(),
            triple: None,
            profile: None,
        }
    }

    pub fn with_var(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.vars.insert(k.into(), v.into());
        self
    }

    pub fn set_var(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.vars.insert(k.into(), v.into());
    }

    pub fn set_profile(&mut self, profile: impl Into<String>) {
        self.profile = Some(profile.into());
    }

    pub fn set_triple(&mut self, triple: impl Into<String>) {
        self.triple = Some(triple.into());
    }
}

/// Perform `${VAR}` interpolation for a single string.
///
/// Supported forms:
/// - `${KEY}`              -> ctx.vars[KEY] or env[KEY]
/// - `${ENV:KEY}`          -> env[KEY]
/// - `${TRIPLE}`           -> ctx.triple
/// - `${PROFILE}`          -> ctx.profile
/// - `${ROOT}`             -> ctx.root (normalized)
/// - `$$`                  -> `$` (escape)
///
/// Policy:
/// - unknown variables are an error (strict determinism)
pub fn expand_str(input: &str, ctx: &ExpandCtx) -> Result<String, SteelError> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '$' => {
                match chars.peek().copied() {
                    Some('$') => {
                        // $$ => $
                        chars.next();
                        out.push('$');
                    }
                    Some('{') => {
                        // ${...}
                        chars.next(); // consume '{'
                        let mut key = String::new();
                        let mut closed = false;
                        while let Some(ch) = chars.next() {
                            if ch == '}' {
                                closed = true;
                                break;
                            }
                            key.push(ch);
                        }
                        if !closed {
                            return Err(SteelError::ValidationFailed(
                                "unterminated ${...} expansion".into(),
                            ));
                        }

                        let val = resolve_key(&key, ctx)?;
                        out.push_str(&val);
                    }
                    _ => {
                        // single '$' is not allowed: ambiguous
                        return Err(SteelError::ValidationFailed(
                            "unexpected '$' (use $$ for literal '$' or ${VAR} for expansion)".into(),
                        ));
                    }
                }
            }
            _ => out.push(c),
        }
    }

    Ok(out)
}

fn resolve_key(key: &str, ctx: &ExpandCtx) -> Result<String, SteelError> {
    // reserved keys
    match key {
        "TRIPLE" => ctx
            .triple
            .clone()
            .ok_or_else(|| SteelError::ValidationFailed("missing TRIPLE in expansion context".into())),
        "PROFILE" => ctx
            .profile
            .clone()
            .ok_or_else(|| SteelError::ValidationFailed("missing PROFILE in expansion context".into())),
        "ROOT" => Ok(normalize_path_lossy(&ctx.root)),
        _ => {
            // ENV:FOO forces environment lookup
            if let Some(rest) = key.strip_prefix("ENV:") {
                return env::var(rest).map_err(|_| {
                    SteelError::ValidationFailed(format!("missing environment variable: {}", rest))
                });
            }

            // ctx var first, then env var
            if let Some(v) = ctx.vars.get(key) {
                return Ok(v.clone());
            }
            env::var(key).map_err(|_| {
                SteelError::ValidationFailed(format!("unknown expansion variable: {}", key))
            })
        }
    }
}

/// Expand then normalize a path string.
/// - expands variables
/// - resolves relative paths against ctx.root
/// - normalizes separators and removes `.` segments
pub fn expand_path(input: &str, ctx: &ExpandCtx) -> Result<PathBuf, SteelError> {
    let s = expand_str(input, ctx)?;
    let p = PathBuf::from(s);

    let joined = if p.is_absolute() {
        p
    } else {
        ctx.root.join(p)
    };

    Ok(normalize_path(&joined))
}

/// Expand a set/map of string values (stable order).
pub fn expand_map(
    map: &BTreeMap<String, String>,
    ctx: &ExpandCtx,
) -> Result<BTreeMap<String, String>, SteelError> {
    let mut out = BTreeMap::new();
    for (k, v) in map.iter() {
        out.insert(k.clone(), expand_str(v, ctx)?);
    }
    Ok(out)
}

/// Expand a list of strings and return a deterministic set (dedup).
pub fn expand_list_dedup_sorted(
    items: &[String],
    ctx: &ExpandCtx,
) -> Result<Vec<String>, SteelError> {
    let mut set = BTreeSet::new();
    for it in items {
        set.insert(expand_str(it, ctx)?);
    }
    Ok(set.into_iter().collect())
}

// --- normalization ----------------------------------------------------------

fn normalize_path(p: &Path) -> PathBuf {
    // Keep prefix (Windows) and root if present, strip '.' components.
    let mut out = PathBuf::new();

    for c in p.components() {
        match c {
            Component::CurDir => {}
            // We keep ParentDir as-is; resolver should avoid producing it if possible.
            Component::ParentDir => out.push(".."),
            Component::Prefix(pref) => out.push(pref.as_os_str()),
            Component::RootDir => out.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::Normal(x) => out.push(x),
        }
    }

    out
}

fn normalize_path_lossy(p: &Path) -> String {
    // Normalize, then stringify with platform separators.
    normalize_path(p).to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_basic() {
        let mut ctx = ExpandCtx::new("C:/repo");
        ctx.set_profile("debug");
        ctx.set_triple("x86_64-pc-windows-msvc");
        ctx.set_var("OUT", "target/out");

        let s = expand_str("bin/${TRIPLE}/${PROFILE}/$${OUT}", &ctx).unwrap();
        assert!(s.contains("x86_64-pc-windows-msvc"));
        assert!(s.contains("debug"));
        assert!(s.contains("${OUT}") == false); // because $${OUT} -> ${OUT} literal
    }

    #[test]
    fn expand_path_rel_root() {
        let mut ctx = ExpandCtx::new("C:/repo");
        ctx.set_profile("debug");
        ctx.set_triple("x86_64-pc-windows-msvc");
        let p = expand_path("target/build/${TRIPLE}/${PROFILE}", &ctx).unwrap();
        let s = p.to_string_lossy().to_string();
        assert!(s.contains("C:") || s.starts_with("C"));
        assert!(s.contains("target"));
    }
}
