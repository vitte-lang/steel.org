//! validator.rs — MAX
//!
//! Validation “policy & invariants” pour Steel/MCFG, au-dessus du typecheck.
//!
//! - typecheck.rs : résolution + typage + cohérence sémantique
//! - validator.rs : règles projet (policy), conventions, intégrité du DAG, ergonomie
//!
//! Principes :
//! - warnings par défaut (non-bloquant) ; `strict=true` transforme en errors.
//! - aucune dépendance externe (std-only).
//!
//! Dépendances attendues :
//! - crate::diag::{DiagBag, Diagnostic}
//! - crate::span::{Span, FileId, Pos}
//! - crate::typecheck::{TcResult, Symbols, ResolvedWire, ResolvedExport, ResolvedPlan, ResolvedRef, PortDir, TypeRef}
//! - crate::schema::{TargetTriple, NormalPath} (optionnel, utilisé si tu valides des paths ailleurs)

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{DiagBag, Diagnostic};
use crate::span::{FileId, Pos, Span};
use crate::typecheck::{
    PortDir, ResolvedExport, ResolvedPlan, ResolvedPlanRun, ResolvedRef, ResolvedWire, Symbols, TcResult,
    TypeRef,
};

/// Entrée standard : valider un résultat de typecheck.
pub fn validate(tc: &TcResult, cfg: &ValidateCfg, diags: &mut DiagBag) -> ValidateReport {
    let mut v = Validator::new(cfg, diags);
    v.run(tc);
    v.finish()
}

/// Entrée “confort” : valider en absence de span global.
/// Si `cfg.global_span` est None, des erreurs “globales” utilisent un span dummy.
pub fn validate_default(tc: &TcResult, diags: &mut DiagBag) -> ValidateReport {
    validate(tc, &ValidateCfg::default(), diags)
}

/// Configuration des politiques.
#[derive(Debug, Clone)]
pub struct ValidateCfg {
    /// Transforme warnings -> errors.
    pub strict: bool,

    /// Span global (si disponible) pour erreurs non-localisables.
    pub global_span: Option<Span>,

    /// Exige au moins 1 plan (ou plan default).
    pub require_plan: bool,

    /// Exige au moins 1 export.
    pub require_exports: bool,

    /// Exige qu’un plan par défaut soit défini (via `set plan "..."`).
    pub require_default_plan: bool,

    /// Exige que le plan default (ou au moins un plan) “touche” des exports.
    pub require_exports_reachable_from_plan: bool,

    /// Politique wiring :
    /// - 0: permissif
    /// - 1: warn si in-port non câblé
    /// - 2: error si in-port non câblé
    pub require_in_ports_wired_level: u8,

    /// Politique wiring : un seul wire par in-port.
    /// - si false: autorise multi-wire (last-wins côté scheduler/impl)
    pub single_driver_per_in_port: bool,

    /// Politique : pas de ports orphelins (out ports jamais utilisés).
    pub warn_unused_out_ports: bool,

    /// Politique : déclarations non utilisées (tools/profiles/capsules/stores/vars/bakes/plans).
    pub warn_unused_decls: bool,

    /// Nommage : taille max ident.
    pub max_ident_len: usize,

    /// Nommage : impose charset portable.
    pub enforce_portable_idents: bool,

    /// Nommage : impose `snake_case` (soft) pour vars/ports/bakes.
    pub prefer_snake_case: bool,

    /// TypeRefs : impose artéfacts en dot-path strict.
    pub enforce_artifact_type_shape: bool,

    /// TypeRefs : liste allow de prim types (garde-fou).
    pub prim_types: BTreeSet<String>,
}

impl Default for ValidateCfg {
    fn default() -> Self {
        let mut prim = BTreeSet::new();
        prim.insert("text".into());
        prim.insert("int".into());
        prim.insert("bool".into());
        prim.insert("bytes".into());

        Self {
            strict: false,
            global_span: None,
            require_plan: true,
            require_exports: true,
            require_default_plan: false,
            require_exports_reachable_from_plan: true,
            require_in_ports_wired_level: 1,
            single_driver_per_in_port: true,
            warn_unused_out_ports: true,
            warn_unused_decls: true,
            max_ident_len: 128,
            enforce_portable_idents: true,
            prefer_snake_case: true,
            enforce_artifact_type_shape: true,
            prim_types: prim,
        }
    }
}

/// Rapport final.
#[derive(Debug, Clone)]
pub struct ValidateReport {
    pub ok: bool,
    pub notes: Vec<String>,
    pub stats: ValidateStats,
}

#[derive(Debug, Clone, Default)]
pub struct ValidateStats {
    pub exports_count: usize,
    pub plans_count: usize,
    pub wires_count: usize,
    pub bakes_count: usize,
    pub vars_count: usize,
    pub tools_count: usize,
    pub stores_count: usize,
    pub capsules_count: usize,
    pub profiles_count: usize,
}

/// ------------------------------------------------------------
/// Validator
/// ------------------------------------------------------------

struct Validator<'d> {
    cfg: &'d ValidateCfg,
    diags: &'d mut DiagBag,
    notes: Vec<String>,
    stats: ValidateStats,
}

impl<'d> Validator<'d> {
    fn new(cfg: &'d ValidateCfg, diags: &'d mut DiagBag) -> Self {
        Self { cfg, diags, notes: Vec::new(), stats: ValidateStats::default() }
    }

    fn finish(self) -> ValidateReport {
        let ok = !self.diags.has_error();
        ValidateReport { ok, notes: self.notes, stats: self.stats }
    }

    fn run(&mut self, tc: &TcResult) {
        self.stats.exports_count = tc.exports.len();
        self.stats.plans_count = tc.plans.len();
        self.stats.wires_count = tc.wires.len();

        self.stats.bakes_count = tc.symbols.bakes.len();
        self.stats.vars_count = tc.symbols.vars.len();
        self.stats.tools_count = tc.symbols.tools.len();
        self.stats.stores_count = tc.symbols.stores.len();
        self.stats.capsules_count = tc.symbols.capsules.len();
        self.stats.profiles_count = tc.symbols.profiles.len();

        self.check_symbols(&tc.symbols);
        self.check_wires(&tc.symbols, &tc.wires);
        self.check_exports(&tc.exports);
        self.check_plans(&tc.plans, tc.default_plan.as_deref());
        self.check_reachability(tc);
        self.check_unused(tc);

        self.notes.push("validator: ok".into());
    }

    /// --------------------------------------------------------
    /// Symbol policies
    /// --------------------------------------------------------

    fn check_symbols(&mut self, sym: &Symbols) {
        for (k, s) in &sym.stores {
            self.check_ident(s.span, k, "store");
        }
        for (k, c) in &sym.capsules {
            self.check_ident(c.span, k, "capsule");
        }
        for (k, v) in &sym.vars {
            self.check_ident(v.span, k, "var");
            self.check_typeref(v.span, &v.ty, "var type");
        }
        for (k, p) in &sym.profiles {
            self.check_ident(p.span, k, "profile");
        }
        for (k, t) in &sym.tools {
            self.check_ident(t.span, k, "tool");
            if t.exec.is_none() {
                self.emit(false, t.span, format!("tool `{}` has no exec", k));
            }
            if let Some(cap) = &t.capsule {
                if !sym.capsules.contains_key(cap) {
                    self.emit(true, t.span, format!("tool `{}` references unknown capsule `{}`", k, cap));
                }
            }
        }
        for (k, b) in &sym.bakes {
            self.check_ident(b.span, k, "bake");
            for (pn, p) in &b.ports_in {
                self.check_ident(p.span, pn, "in port");
                self.check_typeref(p.span, &p.ty, "port type");
            }
            for (pn, p) in &b.ports_out {
                self.check_ident(p.span, pn, "out port");
                self.check_typeref(p.span, &p.ty, "port type");
            }
        }
        for (k, p) in &sym.plans {
            self.check_ident(p.span, k, "plan");
            if p.runs.is_empty() {
                self.emit(false, p.span, format!("plan `{}` has no runs", k));
            }
        }
    }

    fn check_ident(&mut self, span: Span, name: &str, what: &str) {
        if name.is_empty() {
            self.emit(true, span, format!("{} name is empty", what));
            return;
        }
        if name.len() > self.cfg.max_ident_len {
            self.emit(false, span, format!("{} `{}` too long (>{})", what, name, self.cfg.max_ident_len));
        }

        if self.cfg.enforce_portable_idents {
            let ok = name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
            if !ok {
                self.emit(false, span, format!("{} `{}` contains non-portable characters", what, name));
            }
        }

        if self.cfg.prefer_snake_case && (what == "var" || what.contains("port") || what == "bake") {
            if !is_snake_case(name) {
                self.emit(false, span, format!("{} `{}` not snake_case (recommended)", what, name));
            }
        }
    }

    fn check_typeref(&mut self, span: Span, ty: &TypeRef, ctx: &str) {
        if self.cfg.prim_types.contains(&ty.0) {
            return;
        }

        if !self.cfg.enforce_artifact_type_shape {
            return;
        }

        // dot-path d’identifiants : a.b.c
        let ok = ty
            .0
            .split('.')
            .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if !ok {
            self.emit(false, span, format!("{}: invalid artifact type `{}`", ctx, ty.0));
        }
    }

    /// --------------------------------------------------------
    /// Wiring policies
    /// --------------------------------------------------------

    fn check_wires(&mut self, sym: &Symbols, wires: &[ResolvedWire]) {
        // 1) single-driver per in port
        if self.cfg.single_driver_per_in_port {
            let mut driven: BTreeMap<String, Span> = BTreeMap::new();
            for w in wires {
                let to = ref_key(&w.to);
                if let Some(prev) = driven.insert(to.clone(), w.span) {
                    self.emit(
                        false,
                        w.span,
                        format!("multiple wires drive the same input `{}` (previous at {:?})", to, prev),
                    );
                }
            }
        }

        // 2) require in-ports wired
        if self.cfg.require_in_ports_wired_level > 0 {
            let wired_targets = collect_in_port_targets(wires);

            for (bname, b) in &sym.bakes {
                for (pname, p) in &b.ports_in {
                    let key = format!("port:in:{}:{}", bname, pname);
                    if !wired_targets.contains(&key) {
                        let is_err = self.cfg.require_in_ports_wired_level >= 2;
                        self.emit(is_err, p.span, format!("in port `{}`.{} is not wired", bname, pname));
                    }
                }
            }
        }

        // 3) warn unused out ports (not referenced by any wire/export/plan ref)
        if self.cfg.warn_unused_out_ports {
            // handled later in check_unused, but keep fast local check when needed
            let _ = wires;
        }
    }

    /// --------------------------------------------------------
    /// Exports / Plans
    /// --------------------------------------------------------

    fn check_exports(&mut self, exports: &[ResolvedExport]) {
        if self.cfg.require_exports && exports.is_empty() {
            self.emit(true, self.global_span(), "no exports defined");
        }
    }

    fn check_plans(&mut self, plans: &[ResolvedPlan], default_plan: Option<&str>) {
        if self.cfg.require_plan && plans.is_empty() {
            self.emit(true, self.global_span(), "no plans defined");
            return;
        }

        if self.cfg.require_default_plan && default_plan.is_none() {
            self.emit(true, self.global_span(), "default plan not set (use: set plan \"...\")");
        }

        if let Some(dp) = default_plan {
            if !plans.iter().any(|p| p.name == dp) {
                self.emit(true, self.global_span(), format!("default plan `{}` not found", dp));
            }
        }
    }

    fn check_reachability(&mut self, tc: &TcResult) {
        if !self.cfg.require_exports_reachable_from_plan {
            return;
        }
        if tc.exports.is_empty() || tc.plans.is_empty() {
            return;
        }

        let plan_name = tc
            .default_plan
            .as_deref()
            .or_else(|| tc.plans.first().map(|p| p.name.as_str()));

        let Some(pn) = plan_name else {
            return;
        };

        let plan = tc.plans.iter().find(|p| p.name == pn);
        let Some(plan) = plan else { return; };

        // If plan contains "Exports", it is reachable by definition.
        if plan.runs.iter().any(|r| matches!(r, ResolvedPlanRun::Exports(_))) {
            return;
        }

        // Otherwise, check if plan references at least one exported port
        let exported: BTreeSet<String> = tc.exports.iter().map(|e| ref_key(&e.target)).collect();
        let mut touched = false;

        for r in &plan.runs {
            if let ResolvedPlanRun::Ref(rr) = r {
                let k = ref_key(rr);
                if exported.contains(&k) {
                    touched = true;
                    break;
                }
            }
        }

        if !touched {
            self.emit(
                false,
                plan.span,
                format!("plan `{}` does not reference exports (add `run exports` or run an exported ref)", pn),
            );
        }
    }

    /// --------------------------------------------------------
    /// Unused declarations / ports
    /// --------------------------------------------------------

    fn check_unused(&mut self, tc: &TcResult) {
        if !self.cfg.warn_unused_decls && !self.cfg.warn_unused_out_ports {
            return;
        }

        // used sets
        let mut used_bakes: BTreeSet<String> = BTreeSet::new();
        let mut used_vars: BTreeSet<String> = BTreeSet::new();
        let mut used_ports: BTreeSet<String> = BTreeSet::new(); // ref_key
        let mut used_plans: BTreeSet<String> = BTreeSet::new();

        // wires reference ports/vars
        for w in &tc.wires {
            mark_used_ref(&w.from, &mut used_bakes, &mut used_vars, &mut used_ports);
            mark_used_ref(&w.to, &mut used_bakes, &mut used_vars, &mut used_ports);
        }

        // exports reference ports
        for e in &tc.exports {
            mark_used_ref(&e.target, &mut used_bakes, &mut used_vars, &mut used_ports);
        }

        // plans reference exports/ref
        for p in &tc.plans {
            used_plans.insert(p.name.clone());
            for r in &p.runs {
                if let ResolvedPlanRun::Ref(rr) = r {
                    mark_used_ref(rr, &mut used_bakes, &mut used_vars, &mut used_ports);
                }
            }
        }

        // warn unused out ports
        if self.cfg.warn_unused_out_ports {
            for (bname, b) in &tc.symbols.bakes {
                for (pname, p) in &b.ports_out {
                    let key = format!("port:out:{}:{}", bname, pname);
                    if !used_ports.contains(&key) {
                        self.emit(false, p.span, format!("out port `{}`.{} is unused", bname, pname));
                    }
                }
            }
        }

        if !self.cfg.warn_unused_decls {
            return;
        }

        // unused vars
        for (k, v) in &tc.symbols.vars {
            let key = format!("var:{}", k);
            if !used_ports.contains(&key) && !used_vars.contains(k) {
                self.emit(false, v.span, format!("var `{}` declared but not used", k));
            }
        }

        // unused bakes (if no port ref)
        for (k, b) in &tc.symbols.bakes {
            if !used_bakes.contains(k) {
                self.emit(false, b.span, format!("bake `{}` declared but not referenced", k));
            }
        }

        // unused plans (rarely an issue, but useful)
        // If default plan exists, other plans may be legitimately unused.
        if tc.default_plan.is_some() {
            // soft: only warn on plans with empty runs already handled
        } else {
            // if no default plan, warn if more than 1 plan and none referenced explicitly (CLI mapping)
            let _ = used_plans;
        }

        // capsules/stores/profiles/tools: we can’t know “used” accurately without scanning HIR steps.
        // Still: warn only if there are 0 tools/bakes etc is fine; otherwise keep minimal.
        // This is intentional: avoid false positives.
    }

    /// --------------------------------------------------------
    /// Diagnostics helpers
    /// --------------------------------------------------------

    fn global_span(&self) -> Span {
        self.cfg.global_span.unwrap_or_else(|| Span::new(FileId(0), Pos(0), Pos(0)))
    }

    fn emit(&mut self, is_error: bool, span: Span, msg: impl Into<String>) {
        let msg = msg.into();
        let force_err = self.cfg.strict;
        if is_error || force_err {
            self.diags.push(Diagnostic::error_at(span, msg));
        } else {
            self.diags.push(Diagnostic::warning_at(span, msg));
        }
    }
}

/// ------------------------------------------------------------
/// Graph / usage helpers
/// ------------------------------------------------------------

fn ref_key(r: &ResolvedRef) -> String {
    match r {
        ResolvedRef::GlobalVar { name, .. } => format!("var:{}", name),
        ResolvedRef::Port { bake, port, dir, .. } => {
            let d = match dir {
                PortDir::In => "in",
                PortDir::Out => "out",
            };
            format!("port:{}:{}:{}", d, bake, port)
        }
    }
}

fn collect_in_port_targets(wires: &[ResolvedWire]) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for w in wires {
        if matches!(w.to, ResolvedRef::Port { dir: PortDir::In, .. }) {
            s.insert(ref_key(&w.to));
        }
    }
    s
}

fn mark_used_ref(r: &ResolvedRef, used_bakes: &mut BTreeSet<String>, used_vars: &mut BTreeSet<String>, used_ports: &mut BTreeSet<String>) {
    used_ports.insert(ref_key(r));
    match r {
        ResolvedRef::GlobalVar { name, .. } => {
            used_vars.insert(name.clone());
        }
        ResolvedRef::Port { bake, .. } => {
            used_bakes.insert(bake.clone());
        }
    }
}

fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_us = false;
    for (i, c) in s.chars().enumerate() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
        if !ok {
            return false;
        }
        if c == '_' {
            if i == 0 {
                return false;
            }
            if prev_us {
                return false;
            }
            prev_us = true;
        } else {
            prev_us = false;
        }
    }
    !s.ends_with('_')
}

/// ------------------------------------------------------------
/// Tests (helpers only)
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_rule() {
        assert!(is_snake_case("a"));
        assert!(is_snake_case("a_b"));
        assert!(is_snake_case("a1_b2"));
        assert!(!is_snake_case("_a"));
        assert!(!is_snake_case("A"));
        assert!(!is_snake_case("a__b"));
        assert!(!is_snake_case("a_"));
        assert!(!is_snake_case("a-b"));
    }
}