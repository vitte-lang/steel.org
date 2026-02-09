// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\resolve\variable.rs

//! Variable resolution layer.
//!
//! This module is responsible for:
//! - collecting variables from different sources
//! - applying precedence rules
//! - exposing a resolved variable map for expansion
//!
//! Order of precedence (lowest → highest):
//! 1. Built-in defaults
//! 2. Environment variables
//! 3. steelconf explicit variables
//! 4. CLI overrides (-D KEY=VALUE)

use crate::error::SteelError;
use std::collections::BTreeMap;

/// Container for resolved variables.
///
/// Uses `BTreeMap` to guarantee deterministic ordering.
#[derive(Debug, Clone, Default)]
pub struct VarSet {
    vars: BTreeMap<String, String>,
}

impl VarSet {
    /// Create an empty variable set.
    pub fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
        }
    }

    /// Insert a variable if not already defined.
    pub fn insert_default(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.entry(key.into()).or_insert_with(|| value.into());
    }

    /// Force-set a variable (overrides any previous value).
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    /// Merge another VarSet with override semantics.
    pub fn merge_override(&mut self, other: &VarSet) {
        for (k, v) in other.vars.iter() {
            self.vars.insert(k.clone(), v.clone());
        }
    }

    /// Lookup a variable value.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    /// Return an immutable view of all variables.
    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.vars
    }

    /// Validate variable names and values.
    ///
    /// Rules:
    /// - names must be ASCII, uppercase, and use `_`
    /// - values must be non-empty
    pub fn validate(&self) -> Result<(), SteelError> {
        for (k, v) in self.vars.iter() {
            if k.is_empty()
                || !k
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(SteelError::ValidationFailed(format!(
                    "invalid variable name: {}",
                    k
                )));
            }
            if v.is_empty() {
                return Err(SteelError::ValidationFailed(format!(
                    "variable {} has empty value",
                    k
                )));
            }
        }
        Ok(())
    }
}

/// Collect built-in default variables.
pub fn builtin_vars() -> VarSet {
    let mut v = VarSet::new();

    v.insert_default("CONFIG", "debug");
    v.insert_default("PROFILE", "debug");
    v.insert_default("TARGET_DIR", "target");

    v
}

/// Collect variables from environment.
///
/// Only variables prefixed with `MUFFIN_` are imported,
/// and the prefix is stripped.
pub fn env_vars() -> VarSet {
    let mut v = VarSet::new();

    for (k, val) in std::env::vars() {
        if let Some(rest) = k.strip_prefix("MUFFIN_") {
            v.set(rest.to_string(), val);
        }
    }

    v
}

/// Build the final resolved variable set.
///
/// This is the canonical entry point used by the resolver pipeline.
pub fn resolve_vars(
    explicit: &BTreeMap<String, String>,
    cli_overrides: &BTreeMap<String, String>,
) -> Result<VarSet, SteelError> {
    let mut vars = builtin_vars();

    // env overrides defaults
    vars.merge_override(&env_vars());

    // explicit steelconf overrides env
    for (k, v) in explicit {
        vars.set(k.clone(), v.clone());
    }

    // CLI overrides everything
    for (k, v) in cli_overrides {
        vars.set(k.clone(), v.clone());
    }

    vars.validate()?;
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_order() {
        let mut explicit = BTreeMap::new();
        explicit.insert("PROFILE".into(), "release".into());

        let mut cli = BTreeMap::new();
        cli.insert("PROFILE".into(), "debug".into());

        let vars = resolve_vars(&explicit, &cli).unwrap();
        assert_eq!(vars.get("PROFILE").unwrap(), "debug");
    }

    #[test]
    fn invalid_name_rejected() {
        let mut v = VarSet::new();
        v.set("bad-name", "x");
        assert!(v.validate().is_err());
    }
}
