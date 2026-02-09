// /Users/vincent/Documents/Github/steel/src/gettext.rs
//! gettext — i18n message catalogs (std-only, deterministic)
//!
//! This module implements a pragmatic, toolchain-friendly i18n layer for Steel.
//! It is "gettext-inspired" (keyed messages, domains, locale fallback), but does NOT
//! parse GNU .mo files.
//!
//! Design goals:
//! - std-only (no deps)
//! - deterministic load + lookup (BTreeMap/BTreeSet)
//! - explicit domains (steel/runner/etc.)
//! - locale negotiation with fallback chain (fr-FR -> fr -> default)
//! - minimal plural support (ngettext-like) without external rule engines
//! - placeholder formatting `{name}` and `{0}` positional
//! - safe best-effort parsing (malformed lines ignored)
//!
//! Catalog file format (text, UTF-8):
//! - comments: lines starting with `#`
//! - entries: `key = value`
//! - optional domain prefix in key: `domain.key = value`
//! - optional context: `key|ctx = value`  (context disambiguation)
//! - plural: `key[one] = ...`, `key[other] = ...` (or `zero`, `two`, `few`, `many`)
//! - multiline value:
//!     key = """
//!     line1
//!     line2
//!     """
//! - escapes in single-line values: \n \r \t \\ \" \' \{ \}
//!
//! Lookup strategy:
//! - domain -> key -> (context?) -> plural(form?) -> string
//! - if missing: falls back to (same key without ctx) then fallback locales then "key" itself.
//!
//! Placeholders:
//! - Named: `{path}`
//! - Positional: `{0}`, `{1}`
//! Unknown placeholders remain unchanged.
//!
//! Locale resolution:
//! - detect from env: MUFFIN_LANG, LC_ALL, LANG (in that order)
//! - normalize: strip encoding and modifiers ("fr_FR.UTF-8@euro" -> "fr_FR")
//! - expand fallback chain: fr-FR -> fr -> default
//!
//! Thread-safety:
//! - optional global registry for callers who want a shared catalog (std::sync::OnceLock).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

pub type Domain = String;
pub type Locale = String;

pub const DEFAULT_DOMAIN: &str = "steel";

/// Plural categories (subset aligned to CLDR names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluralForm {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralForm {
    pub fn as_str(self) -> &'static str {
        match self {
            PluralForm::Zero => "zero",
            PluralForm::One => "one",
            PluralForm::Two => "two",
            PluralForm::Few => "few",
            PluralForm::Many => "many",
            PluralForm::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "zero" => Some(PluralForm::Zero),
            "one" => Some(PluralForm::One),
            "two" => Some(PluralForm::Two),
            "few" => Some(PluralForm::Few),
            "many" => Some(PluralForm::Many),
            "other" => Some(PluralForm::Other),
            _ => None,
        }
    }
}

/// Minimal plural rules.
/// In practice Steel messages rarely need full CLDR; this is enough to be correct-ish
/// for common locales. You can extend rules later without breaking the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralRule {
    /// English-like: one if n==1 else other.
    OneOther,
    /// French-like: one if n==0||n==1 else other.
    ZeroOneOther,
    /// Always other.
    OtherOnly,
}

impl PluralRule {
    pub fn select(self, n: i64) -> PluralForm {
        match self {
            PluralRule::OneOther => {
                if n == 1 { PluralForm::One } else { PluralForm::Other }
            }
            PluralRule::ZeroOneOther => {
                if n == 0 || n == 1 { PluralForm::One } else { PluralForm::Other }
            }
            PluralRule::OtherOnly => PluralForm::Other,
        }
    }
}

/// An entry can have:
/// - a simple string
/// - plural forms
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryValue {
    Simple(String),
    Plural(BTreeMap<PluralForm, String>),
}

/// A single catalog for a specific locale.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Locale tag (e.g. "fr-FR"). None means "default".
    pub locale: Option<Locale>,
    /// Domain -> Key -> Context? -> Value
    pub data: BTreeMap<Domain, BTreeMap<String, BTreeMap<Option<String>, EntryValue>>>,
    /// Optional plural rule override for this catalog.
    pub plural_rule: Option<PluralRule>,
    /// Metadata (free-form).
    pub meta: BTreeMap<String, String>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_locale(locale: Option<Locale>) -> Self {
        Self { locale, ..Self::default() }
    }

    pub fn set_plural_rule(&mut self, rule: PluralRule) {
        self.plural_rule = Some(rule);
    }

    pub fn set_meta(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.meta.insert(k.into(), v.into());
    }

    /// Insert/override a simple message.
    pub fn set(&mut self, domain: &str, key: &str, ctx: Option<&str>, value: impl Into<String>) {
        let d = self.data.entry(domain.to_string()).or_default();
        let k = d.entry(key.to_string()).or_default();
        let ctxk = ctx.map(|s| s.to_string());
        k.insert(ctxk, EntryValue::Simple(value.into()));
    }

    /// Insert/override a plural message form.
    pub fn set_plural(
        &mut self,
        domain: &str,
        key: &str,
        ctx: Option<&str>,
        form: PluralForm,
        value: impl Into<String>,
    ) {
        let d = self.data.entry(domain.to_string()).or_default();
        let k = d.entry(key.to_string()).or_default();
        let ctxk = ctx.map(|s| s.to_string());
        match k.entry(ctxk).or_insert_with(|| EntryValue::Plural(BTreeMap::new())) {
            EntryValue::Plural(map) => {
                map.insert(form, value.into());
            }
            EntryValue::Simple(_) => {
                // overwrite simple with plural
                let mut map = BTreeMap::new();
                map.insert(form, value.into());
                *k.get_mut(&ctx.map(|s| s.to_string())).unwrap() = EntryValue::Plural(map);
            }
        }
    }

    /// Merge another catalog into this one (other wins on conflicts).
    pub fn merge_from(&mut self, other: &Catalog) {
        if self.locale.is_none() {
            self.locale = other.locale.clone();
        }
        if self.plural_rule.is_none() {
            self.plural_rule = other.plural_rule;
        }
        for (k, v) in &other.meta {
            self.meta.insert(k.clone(), v.clone());
        }
        for (dom, dmap) in &other.data {
            let dd = self.data.entry(dom.clone()).or_default();
            for (key, cmap) in dmap {
                let kk = dd.entry(key.clone()).or_default();
                for (ctx, val) in cmap {
                    kk.insert(ctx.clone(), val.clone());
                }
            }
        }
    }

    /// Load from file path. Best-effort: malformed lines ignored.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let s = fs::read_to_string(path)?;
        self.load_str(&s);
        Ok(())
    }

    /// Load from raw text.
    pub fn load_str(&mut self, text: &str) {
        let mut lines = text.lines().enumerate().peekable();

        while let Some((lineno, raw)) = lines.next() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Metadata: @key = value
            if let Some(rest) = line.strip_prefix('@') {
                if let Some((k, v)) = split_kv(rest) {
                    self.meta.insert(k.trim().to_string(), parse_value(v.trim(), &mut lines));
                }
                continue;
            }

            let Some((kraw, vraw)) = split_kv(line) else {
                let _ = lineno;
                continue;
            };

            let key_spec = kraw.trim();
            if key_spec.is_empty() {
                continue;
            }

            let value = parse_value(vraw.trim(), &mut lines);

            // Parse key: [domain.]key[|ctx][[form]]
            let (domain, key, ctx, form) = parse_key_spec(key_spec);

            match form {
                Some(pf) => self.set_plural(&domain, &key, ctx.as_deref(), pf, value),
                None => self.set(&domain, &key, ctx.as_deref(), value),
            }
        }
    }

    /// Translate with default domain and no context.
    pub fn tr<'a>(&'a self, key: &'a str) -> Message<'a> {
        Message { catalog: self, domain: DEFAULT_DOMAIN, key, ctx: None }
    }

    /// Translate with explicit domain.
    pub fn trd<'a>(&'a self, domain: &'a str, key: &'a str) -> Message<'a> {
        Message { catalog: self, domain, key, ctx: None }
    }

    /// Translate with context.
    pub fn trc<'a>(&'a self, key: &'a str, ctx: &'a str) -> Message<'a> {
        Message { catalog: self, domain: DEFAULT_DOMAIN, key, ctx: Some(ctx) }
    }

    /// Translate with domain + context.
    pub fn trdc<'a>(&'a self, domain: &'a str, key: &'a str, ctx: &'a str) -> Message<'a> {
        Message { catalog: self, domain, key, ctx: Some(ctx) }
    }

    /// Low-level lookup. Returns reference to the stored string if found.
    pub fn lookup(&self, domain: &str, key: &str, ctx: Option<&str>) -> Option<&str> {
        let d = self.data.get(domain)?;
        let k = d.get(key)?;
        // try ctx then fallback ctx=None
        if let Some(c) = ctx {
            if let Some(v) = k.get(&Some(c.to_string())) {
                return entry_simple(v);
            }
        }
        k.get(&None).and_then(entry_simple)
    }

    /// Low-level plural lookup. Returns reference to the stored plural form if found.
    pub fn lookup_plural(&self, domain: &str, key: &str, ctx: Option<&str>, form: PluralForm) -> Option<&str> {
        let d = self.data.get(domain)?;
        let k = d.get(key)?;

        // try ctx then ctx=None
        let candidates: Vec<Option<String>> = match ctx {
            Some(c) => vec![Some(c.to_string()), None],
            None => vec![None],
        };

        for c in candidates {
            let v = k.get(&c)?;
            if let EntryValue::Plural(map) = v {
                if let Some(s) = map.get(&form) {
                    return Some(s.as_str());
                }
                // fallback to other if requested form missing
                if form != PluralForm::Other {
                    if let Some(s) = map.get(&PluralForm::Other) {
                        return Some(s.as_str());
                    }
                }
            } else if let EntryValue::Simple(s) = v {
                // degrade: return simple
                return Some(s.as_str());
            }
        }
        None
    }

    /// Select plural rule based on locale (best-effort) unless overridden.
    pub fn plural_rule(&self) -> PluralRule {
        if let Some(r) = self.plural_rule {
            return r;
        }
        let loc = self.locale.as_deref().unwrap_or("");
        plural_rule_for_locale(loc)
    }
}

fn entry_simple(v: &EntryValue) -> Option<&str> {
    match v {
        EntryValue::Simple(s) => Some(s.as_str()),
        EntryValue::Plural(map) => map.get(&PluralForm::Other).map(|s| s.as_str()),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Message<'a> {
    catalog: &'a Catalog,
    domain: &'a str,
    key: &'a str,
    ctx: Option<&'a str>,
}

impl<'a> Message<'a> {
    pub fn raw(self) -> &'a str {
        self.catalog
            .lookup(self.domain, self.key, self.ctx)
            .unwrap_or(self.key)
    }

    pub fn fmt(self, named: &[(&str, &str)]) -> String {
        format_placeholders(self.raw(), named, &[])
    }

    pub fn fmt_pos(self, pos: &[&str]) -> String {
        format_placeholders(self.raw(), &[], pos)
    }

    pub fn fmt_all(self, named: &[(&str, &str)], pos: &[&str]) -> String {
        format_placeholders(self.raw(), named, pos)
    }

    /// ngettext-like: plural selection + formatting.
    pub fn nfmt(self, n: i64, named: &[(&str, &str)], pos: &[&str]) -> String {
        let form = self.catalog.plural_rule().select(n);
        let s = self
            .catalog
            .lookup_plural(self.domain, self.key, self.ctx, form)
            .unwrap_or(self.key);
        // common convenience: make `{n}` available
        // Keep lifetime simple: build owned pairs and then borrow them.
        let mut owned: Vec<(String, String)> = Vec::with_capacity(named.len() + 1);
        owned.push(("n".to_string(), n.to_string()));
        for (k, v) in named {
            owned.push(((*k).to_string(), (*v).to_string()));
        }
        let refs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        format_placeholders(s, &refs, pos)
    }
}

/// Bundle of multiple catalogs with locale fallback chain.
#[derive(Debug, Clone, Default)]
pub struct CatalogBundle {
    /// Preferred locales in order (e.g., ["fr-FR","fr"]).
    pub locales: Vec<Locale>,
    /// Locale -> Catalog
    pub catalogs: BTreeMap<Locale, Catalog>,
    /// Default (no-locale) catalog.
    pub default: Catalog,
}

impl CatalogBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_locales(locales: Vec<Locale>) -> Self {
        Self { locales, ..Self::default() }
    }

    pub fn set_default(mut self, cat: Catalog) -> Self {
        self.default = cat;
        self
    }

    pub fn insert_catalog(&mut self, locale: impl Into<Locale>, cat: Catalog) {
        self.catalogs.insert(locale.into(), cat);
    }

    /// Resolve a message from locale chain.
    pub fn lookup(&self, domain: &str, key: &str, ctx: Option<&str>) -> Option<&str> {
        for loc in &self.locales {
            if let Some(cat) = self.catalogs.get(loc) {
                if let Some(s) = cat.lookup(domain, key, ctx) {
                    return Some(s);
                }
            }
        }
        self.default.lookup(domain, key, ctx)
    }

    pub fn lookup_plural(&self, domain: &str, key: &str, ctx: Option<&str>, n: i64) -> Option<&str> {
        for loc in &self.locales {
            if let Some(cat) = self.catalogs.get(loc) {
                let form = cat.plural_rule().select(n);
                if let Some(s) = cat.lookup_plural(domain, key, ctx, form) {
                    return Some(s);
                }
            }
        }
        let form = self.default.plural_rule().select(n);
        self.default.lookup_plural(domain, key, ctx, form)
    }

    /// Translate using bundle (default domain).
    pub fn tr<'a>(&'a self, key: &'a str) -> BundleMessage<'a> {
        BundleMessage { bundle: self, domain: DEFAULT_DOMAIN, key, ctx: None }
    }

    pub fn trd<'a>(&'a self, domain: &'a str, key: &'a str) -> BundleMessage<'a> {
        BundleMessage { bundle: self, domain, key, ctx: None }
    }

    pub fn trc<'a>(&'a self, key: &'a str, ctx: &'a str) -> BundleMessage<'a> {
        BundleMessage { bundle: self, domain: DEFAULT_DOMAIN, key, ctx: Some(ctx) }
    }

    pub fn trdc<'a>(&'a self, domain: &'a str, key: &'a str, ctx: &'a str) -> BundleMessage<'a> {
        BundleMessage { bundle: self, domain, key, ctx: Some(ctx) }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BundleMessage<'a> {
    bundle: &'a CatalogBundle,
    domain: &'a str,
    key: &'a str,
    ctx: Option<&'a str>,
}

impl<'a> BundleMessage<'a> {
    pub fn raw(self) -> &'a str {
        self.bundle.lookup(self.domain, self.key, self.ctx).unwrap_or(self.key)
    }

    pub fn fmt(self, named: &[(&str, &str)]) -> String {
        format_placeholders(self.raw(), named, &[])
    }

    pub fn fmt_pos(self, pos: &[&str]) -> String {
        format_placeholders(self.raw(), &[], pos)
    }

    pub fn fmt_all(self, named: &[(&str, &str)], pos: &[&str]) -> String {
        format_placeholders(self.raw(), named, pos)
    }

    pub fn nfmt(self, n: i64, named: &[(&str, &str)], pos: &[&str]) -> String {
        let s = self.bundle.lookup_plural(self.domain, self.key, self.ctx, n).unwrap_or(self.key);

        let mut owned: Vec<(String, String)> = Vec::with_capacity(named.len() + 1);
        owned.push(("n".to_string(), n.to_string()));
        for (k, v) in named {
            owned.push(((*k).to_string(), (*v).to_string()));
        }
        let refs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        format_placeholders(s, &refs, pos)
    }
}

/// Global bundle registry (optional usage).
static GLOBAL_BUNDLE: OnceLock<RwLock<CatalogBundle>> = OnceLock::new();

pub fn global_bundle() -> &'static RwLock<CatalogBundle> {
    GLOBAL_BUNDLE.get_or_init(|| RwLock::new(CatalogBundle::new()))
}

/// Install a new global bundle.
pub fn set_global_bundle(bundle: CatalogBundle) {
    let lock = global_bundle();
    if let Ok(mut w) = lock.write() {
        *w = bundle;
    }
}

/// Helpers to detect/normalize locale + fallback chain.
pub fn detect_locale_from_env() -> Option<Locale> {
    for k in ["MUFFIN_LANG", "LC_ALL", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(normalize_locale(t));
            }
        }
    }
    None
}

pub fn normalize_locale(raw: &str) -> Locale {
    // strip encoding ".UTF-8" and modifiers "@..."
    let base = raw.split('.').next().unwrap_or(raw);
    let base = base.split('@').next().unwrap_or(base);
    // normalize underscores to dashes in region: fr_FR -> fr-FR
    let mut s = base.trim().to_string();
    if s.contains('_') {
        s = s.replace('_', "-");
    }
    s
}

/// Build fallback chain, e.g. "fr-FR" -> ["fr-FR","fr"].
pub fn locale_fallback_chain(locale: &str) -> Vec<Locale> {
    let mut out = Vec::new();
    let loc = normalize_locale(locale);
    if loc.is_empty() {
        return out;
    }
    out.push(loc.clone());
    if let Some((lang, _rest)) = loc.split_once('-') {
        if !lang.trim().is_empty() {
            out.push(lang.trim().to_string());
        }
    }
    // deterministic unique
    let mut seen = BTreeSet::new();
    out.retain(|l| seen.insert(l.clone()));
    out
}

pub fn plural_rule_for_locale(locale: &str) -> PluralRule {
    let l = normalize_locale(locale).to_ascii_lowercase();
    let lang = l.split('-').next().unwrap_or("").to_string();

    // pragmatic selection
    match lang.as_str() {
        "fr" => PluralRule::ZeroOneOther,
        "en" => PluralRule::OneOther,
        "" => PluralRule::OneOther,
        _ => PluralRule::OneOther,
    }
}

/// Load catalogs from a directory convention:
/// - <dir>/<locale>/<domain>.txt  (or .cat)
/// - <dir>/default/<domain>.txt
///
/// Example:
///   i18n/fr-FR/steel.txt
///   i18n/fr/steel.txt
///   i18n/default/steel.txt
pub fn load_bundle_from_dir(dir: impl AsRef<Path>, preferred_locale: Option<&str>, domain_files: &[(&str, &str)]) -> std::io::Result<CatalogBundle> {
    let dir = dir.as_ref();

    let mut bundle = CatalogBundle::new();

    // locales chain
    if let Some(loc) = preferred_locale {
        bundle.locales = locale_fallback_chain(loc);
    } else if let Some(loc) = detect_locale_from_env() {
        bundle.locales = locale_fallback_chain(&loc);
    }

    // load default
    for (_domain, filename) in domain_files {
        let p = dir.join("default").join(filename);
        if p.is_file() {
            let mut cat = Catalog::with_locale(None);
            cat.load_file(&p)?;
            // merge into bundle default (domain keys can be mixed)
            bundle.default.merge_from(&cat);
        }
    }

    // load each locale catalog
    for loc in &bundle.locales.clone() {
        let mut cat = Catalog::with_locale(Some(loc.clone()));
        let mut loaded_any = false;

        for (_domain, filename) in domain_files {
            let p = dir.join(loc).join(filename);
            if p.is_file() {
                cat.load_file(&p)?;
                loaded_any = true;
            }
        }

        if loaded_any {
            bundle.insert_catalog(loc.clone(), cat);
        }
    }

    Ok(bundle)
}

// ------------------------------ parsing helpers ------------------------------

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut it = line.splitn(2, '=');
    Some((it.next()?, it.next()?))
}

fn parse_key_spec(spec: &str) -> (Domain, String, Option<String>, Option<PluralForm>) {
    // domain prefix: domain.key...
    let (domain, rest) = if let Some((d, r)) = spec.split_once('.') {
        if is_domain_like(d) { (d.to_string(), r) } else { (DEFAULT_DOMAIN.to_string(), spec) }
    } else {
        (DEFAULT_DOMAIN.to_string(), spec)
    };

    // context: key|ctx
    let (key_ctx, ctx) = if let Some((k, c)) = rest.split_once('|') {
        (k.trim(), Some(c.trim().to_string()))
    } else {
        (rest.trim(), None)
    };

    // plural form: key[one]
    if let Some((k, form)) = key_ctx.rsplit_once('[') {
        if let Some(form) = form.strip_suffix(']') {
            if let Some(pf) = PluralForm::parse(form) {
                return (domain, k.trim().to_string(), ctx, Some(pf));
            }
        }
    }

    (domain, key_ctx.to_string(), ctx, None)
}

fn is_domain_like(s: &str) -> bool {
    // conservative: [a-zA-Z0-9_-]+
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_value<'a, I>(v: &str, lines: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = (usize, &'a str)>,
{
    // multiline """ ... """
    if v == r#""""# || v == r#"""""# || v.starts_with(r#""""#) {
        // allow: key = """ on same line
        let mut acc = String::new();

        // if value is exactly """ then body starts on next lines
        // if value starts with """ and has trailing content, treat that as first line
        let mut started_inline = false;
        let inline = v;

        if inline.starts_with(r#""""#) {
            let rest = inline.trim_start_matches(r#""""#);
            if !rest.trim().is_empty() {
                acc.push_str(rest);
                acc.push('\n');
                started_inline = true;
            }
        }

        while let Some((_ln, raw)) = lines.next() {
            let l = raw;
            if l.trim_end() == r#""""# {
                break;
            }
            acc.push_str(l);
            acc.push('\n');
        }

        if !started_inline && acc.ends_with('\n') {
            acc.pop();
        }
        return acc;
    }

    parse_escapes(v)
}

fn parse_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('{') => out.push('{'),
            Some('}') => out.push('}'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out.trim().to_string()
}

// ------------------------------ formatting ------------------------------

/// Replace `{name}` with named args and `{0}` with positional args.
/// Unknown placeholders are preserved.
pub fn format_placeholders(template: &str, named: &[(&str, &str)], pos: &[&str]) -> String {
    let mut nmap: BTreeMap<&str, &str> = BTreeMap::new();
    for (k, v) in named {
        nmap.insert(*k, *v);
    }

    let bytes = template.as_bytes();
    let mut i = 0usize;
    let mut out = String::with_capacity(template.len() + 16);

    while i < bytes.len() {
        if bytes[i] != b'{' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // read until '}'
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != b'}' {
            j += 1;
        }
        if j >= bytes.len() {
            out.push('{');
            i += 1;
            continue;
        }

        let key = &template[start..j];

        // positional?
        if let Ok(idx) = key.trim().parse::<usize>() {
            if let Some(v) = pos.get(idx) {
                out.push_str(v);
            } else {
                out.push('{'); out.push_str(key); out.push('}');
            }
            i = j + 1;
            continue;
        }

        // named
        if let Some(v) = nmap.get(key) {
            out.push_str(v);
        } else {
            out.push('{'); out.push_str(key); out.push('}');
        }

        i = j + 1;
    }

    out
}

// ------------------------------ errors ------------------------------

#[derive(Debug, Clone)]
pub struct CatalogError {
    pub message: String,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CatalogError {}

// ------------------------------ tests ------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_simple_and_domain_ctx() {
        let mut c = Catalog::new();
        c.load_str(
            r#"
            steel.hello = Hello
            runner.hello|button = Run
            runner.hello|menu = Run action
        "#,
        );

        assert_eq!(c.lookup("steel", "hello", None), Some("Hello"));
        assert_eq!(c.lookup("runner", "hello", Some("button")), Some("Run"));
        assert_eq!(c.lookup("runner", "hello", Some("menu")), Some("Run action"));
        // ctx fallback to None not present -> None
        assert_eq!(c.lookup("runner", "hello", Some("missing")), None);
    }

    #[test]
    fn parse_plural_forms() {
        let mut c = Catalog::with_locale(Some("en".into()));
        c.load_str(
            r#"
            steel.files[one] = {n} file
            steel.files[other] = {n} files
        "#,
        );

        let msg = c.tr("files");
        assert_eq!(msg.nfmt(1, &[], &[]), "1 file");
        assert_eq!(msg.nfmt(2, &[], &[]), "2 files");
    }

    #[test]
    fn formatting_named_and_positional() {
        let s = format_placeholders("x={x} y={0}", &[("x", "1")], &["2"]);
        assert_eq!(s, "x=1 y=2");

        let s = format_placeholders("{missing} {1}", &[], &["a"]);
        assert_eq!(s, "{missing} {1}");
    }

    #[test]
    fn locale_chain() {
        let v = locale_fallback_chain("fr_FR.UTF-8@euro");
        assert_eq!(v[0], "fr-FR");
        assert_eq!(v[1], "fr");
    }

    #[test]
    fn load_bundle_dir_convention_smoke() {
        // This is a smoke test for path building; not creating files here.
        let _ = load_bundle_from_dir(
            PathBuf::from("i18n"),
            Some("fr-FR"),
            &[("steel", "steel.txt"), ("runner", "runner.txt")],
        );
    }
}
