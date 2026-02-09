//! typecheck.rs — MAX
//!
//! Typechecking / validation sémantique pour le buildfile Steel (EBNF v2) et configs MCFG.
//!
//! Rôle:
//! - vérifie la cohérence des déclarations (store/capsule/var/profile/tool/bake/wire/export/plan/switch)
//! - construit une table de symboles (vars, types, tools, bakes, ports, plans, etc.)
//! - vérifie le typage des valeurs (text/int/bool/bytes + artifact types: a.b.c)
//! - vérifie les connexions wire (out -> in) + compatibilité de type
//! - vérifie exports (doit pointer vers un port out) + plans (run exports/ref)
//! - fournit diagnostics détaillés (span + message) via DiagBag
//!
//! Dépendances attendues:
//! - crate::diag::*
//! - crate::span::*
//! - crate::token / parser / ast / hir (selon pipeline)
//!
//! Notes:
//! - Ce module ne décide pas des règles d’implémentation runtime (cache, scheduler), seulement la validité.
//! - Le lowering (AST->HIR) peut déjà “désucrer” certaines formes; ici on typecheck sur HIR si dispo.
//!
//! Hypothèse HIR:
//! - hir::BuildFile { items: Vec<Item> }
//! - Items: Store, Capsule, Var, Profile, Tool, Bake, Wire, Export, Plan, Switch, SetGlobal
//! - Bake contient ports + steps (make/run/cache/output)
//!
//! Si ton HIR diffère, ce fichier reste une base: adapter les enum/struct.

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{DiagBag, Diagnostic};
use crate::span::{Span};
use crate::hir as hir;

/// ------------------------------------------------------------
/// Public API
/// ------------------------------------------------------------

pub fn typecheck_buildfile(file: &hir::BuildFile, diags: &mut DiagBag) -> TcResult {
    let mut tc = TypeChecker::new(diags);
    tc.collect(file);
    tc.check(file);
    tc.finish()
}

/// Résultat “utilisable” après typecheck: symbol table + resolved wiring.
#[derive(Debug, Clone)]
pub struct TcResult {
    pub ok: bool,

    pub symbols: Symbols,
    pub wires: Vec<ResolvedWire>,
    pub exports: Vec<ResolvedExport>,
    pub plans: Vec<ResolvedPlan>,

    // convenience: default plan (if resolved)
    pub default_plan: Option<String>,
}

impl TcResult {
    pub fn has_errors(&self) -> bool {
        !self.ok
    }
}

/// ------------------------------------------------------------
/// Resolved entities
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeRef(pub String);

impl TypeRef {
    pub fn prim(name: &str) -> Self {
        TypeRef(name.to_string())
    }
    pub fn artifact(path: &str) -> Self {
        TypeRef(path.to_string())
    }

    pub fn is_prim(&self) -> bool {
        matches!(self.0.as_str(), "text" | "int" | "bool" | "bytes")
    }
}

#[derive(Debug, Clone)]
pub struct Symbols {
    pub stores: BTreeMap<String, SymStore>,
    pub capsules: BTreeMap<String, SymCapsule>,
    pub vars: BTreeMap<String, SymVar>,
    pub profiles: BTreeMap<String, SymProfile>,
    pub tools: BTreeMap<String, SymTool>,
    pub bakes: BTreeMap<String, SymBake>,
    pub plans: BTreeMap<String, SymPlan>,
}

impl Default for Symbols {
    fn default() -> Self {
        Self {
            stores: BTreeMap::new(),
            capsules: BTreeMap::new(),
            vars: BTreeMap::new(),
            profiles: BTreeMap::new(),
            tools: BTreeMap::new(),
            bakes: BTreeMap::new(),
            plans: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SymStore {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SymCapsule {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SymVar {
    pub name: String,
    pub span: Span,
    pub ty: TypeRef,
    pub value: hir::Value,
}

#[derive(Debug, Clone)]
pub struct SymProfile {
    pub name: String,
    pub span: Span,
    pub sets: BTreeMap<String, hir::Value>,
}

#[derive(Debug, Clone)]
pub struct SymTool {
    pub name: String,
    pub span: Span,
    pub exec: Option<String>,
    pub expect_version: Option<String>,
    pub sandbox: Option<bool>,
    pub capsule: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SymPort {
    pub name: String,
    pub span: Span,
    pub ty: TypeRef,
    pub dir: PortDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDir {
    In,
    Out,
}

#[derive(Debug, Clone)]
pub struct SymBake {
    pub name: String,
    pub span: Span,
    pub ports_in: BTreeMap<String, SymPort>,
    pub ports_out: BTreeMap<String, SymPort>,
}

#[derive(Debug, Clone)]
pub struct SymPlan {
    pub name: String,
    pub span: Span,
    pub runs: Vec<hir::PlanRun>,
}

#[derive(Debug, Clone)]
pub struct ResolvedWire {
    pub span: Span,
    pub from: ResolvedRef,
    pub to: ResolvedRef,
    pub ty: TypeRef,
}

#[derive(Debug, Clone)]
pub struct ResolvedExport {
    pub span: Span,
    pub target: ResolvedRef,
    pub ty: TypeRef,
}

#[derive(Debug, Clone)]
pub struct ResolvedPlan {
    pub span: Span,
    pub name: String,
    pub runs: Vec<ResolvedPlanRun>,
}

#[derive(Debug, Clone)]
pub enum ResolvedPlanRun {
    Exports(Span),
    Ref(ResolvedRef),
}

#[derive(Debug, Clone)]
pub enum ResolvedRef {
    GlobalVar { span: Span, name: String, ty: TypeRef },
    Port { span: Span, bake: String, port: String, dir: PortDir, ty: TypeRef },
}

/// ------------------------------------------------------------
/// TypeChecker
/// ------------------------------------------------------------

struct TypeChecker<'d> {
    diags: &'d mut DiagBag,

    symbols: Symbols,

    wires: Vec<ResolvedWire>,
    exports: Vec<ResolvedExport>,
    plans: Vec<ResolvedPlan>,

    // global setters
    global_set: BTreeMap<String, hir::Value>,
    default_plan: Option<String>,
}

impl<'d> TypeChecker<'d> {
    fn new(diags: &'d mut DiagBag) -> Self {
        Self {
            diags,
            symbols: Symbols::default(),
            wires: Vec::new(),
            exports: Vec::new(),
            plans: Vec::new(),
            global_set: BTreeMap::new(),
            default_plan: None,
        }
    }

    fn finish(self) -> TcResult {
        let ok = !self.diags.has_error();
        TcResult {
            ok,
            symbols: self.symbols,
            wires: self.wires,
            exports: self.exports,
            plans: self.plans,
            default_plan: self.default_plan,
        }
    }

    /// Phase 1: collect names + basics, detect duplicates.
    fn collect(&mut self, file: &hir::BuildFile) {
        for it in &file.items {
            match it {
                hir::Item::Store(s) => self.collect_store(s),
                hir::Item::Capsule(c) => self.collect_capsule(c),
                hir::Item::Var(v) => self.collect_var(v),
                hir::Item::Profile(p) => self.collect_profile(p),
                hir::Item::Tool(t) => self.collect_tool(t),
                hir::Item::Bake(b) => self.collect_bake(b),
                hir::Item::Plan(p) => self.collect_plan(p),
                hir::Item::SetGlobal(s) => self.collect_set_global(s),
                // wire/export/switch verified later (need symbols)
                _ => {}
            }
        }
    }

    fn collect_store(&mut self, s: &hir::StoreDecl) {
        if self.symbols.stores.contains_key(&s.name) {
            self.err(s.span, format!("duplicate store `{}`", s.name));
            return;
        }
        self.symbols.stores.insert(
            s.name.clone(),
            SymStore { name: s.name.clone(), span: s.span },
        );
    }

    fn collect_capsule(&mut self, c: &hir::CapsuleDecl) {
        if self.symbols.capsules.contains_key(&c.name) {
            self.err(c.span, format!("duplicate capsule `{}`", c.name));
            return;
        }
        self.symbols.capsules.insert(
            c.name.clone(),
            SymCapsule { name: c.name.clone(), span: c.span },
        );
    }

    fn collect_var(&mut self, v: &hir::VarDecl) {
        if self.symbols.vars.contains_key(&v.name) {
            self.err(v.span, format!("duplicate var `{}`", v.name));
            return;
        }
        let ty = TypeRef(v.ty.clone());
        self.symbols.vars.insert(
            v.name.clone(),
            SymVar { name: v.name.clone(), span: v.span, ty, value: v.value.clone() },
        );
    }

    fn collect_profile(&mut self, p: &hir::ProfileDecl) {
        if self.symbols.profiles.contains_key(&p.name) {
            self.err(p.span, format!("duplicate profile `{}`", p.name));
            return;
        }
        let mut sets = BTreeMap::new();
        for set in &p.sets {
            if sets.contains_key(&set.key) {
                self.warn(set.span, format!("duplicate profile set `{}` (last-wins)", set.key));
            }
            sets.insert(set.key.clone(), set.value.clone());
        }
        self.symbols.profiles.insert(
            p.name.clone(),
            SymProfile { name: p.name.clone(), span: p.span, sets },
        );
    }

    fn collect_tool(&mut self, t: &hir::ToolDecl) {
        if self.symbols.tools.contains_key(&t.name) {
            self.err(t.span, format!("duplicate tool `{}`", t.name));
            return;
        }

        let mut exec = None;
        let mut expect_version = None;
        let mut sandbox = None;
        let mut capsule = None;

        for item in &t.items {
            match item {
                hir::ToolItem::Exec { span: _, value } => exec = Some(value.clone()),
                hir::ToolItem::ExpectVersion { span: _, value } => expect_version = Some(value.clone()),
                hir::ToolItem::Sandbox { span: _, value } => sandbox = Some(*value),
                hir::ToolItem::Capsule { span: _, name } => capsule = Some(name.clone()),
            }
        }

        self.symbols.tools.insert(
            t.name.clone(),
            SymTool { name: t.name.clone(), span: t.span, exec, expect_version, sandbox, capsule },
        );
    }

    fn collect_bake(&mut self, b: &hir::BakeDecl) {
        if self.symbols.bakes.contains_key(&b.name) {
            self.err(b.span, format!("duplicate bake `{}`", b.name));
            return;
        }

        let mut ports_in = BTreeMap::new();
        let mut ports_out = BTreeMap::new();

        for p in &b.ports {
            let ty = TypeRef(p.ty.clone());
            let sym = SymPort {
                name: p.name.clone(),
                span: p.span,
                ty,
                dir: if p.dir == hir::PortDir::In { PortDir::In } else { PortDir::Out },
            };

            match sym.dir {
                PortDir::In => {
                    if ports_in.contains_key(&sym.name) {
                        self.err(sym.span, format!("duplicate in port `{}` in bake `{}`", sym.name, b.name));
                    } else {
                        ports_in.insert(sym.name.clone(), sym);
                    }
                }
                PortDir::Out => {
                    if ports_out.contains_key(&sym.name) {
                        self.err(sym.span, format!("duplicate out port `{}` in bake `{}`", sym.name, b.name));
                    } else {
                        ports_out.insert(sym.name.clone(), sym);
                    }
                }
            }
        }

        self.symbols.bakes.insert(
            b.name.clone(),
            SymBake { name: b.name.clone(), span: b.span, ports_in, ports_out },
        );
    }

    fn collect_plan(&mut self, p: &hir::PlanDecl) {
        if self.symbols.plans.contains_key(&p.name) {
            self.err(p.span, format!("duplicate plan `{}`", p.name));
            return;
        }
        self.symbols.plans.insert(
            p.name.clone(),
            SymPlan { name: p.name.clone(), span: p.span, runs: p.runs.clone() },
        );
    }

    fn collect_set_global(&mut self, s: &hir::SetGlobalDecl) {
        if self.global_set.contains_key(&s.key) {
            self.warn(s.span, format!("duplicate global set `{}` (last-wins)", s.key));
        }
        if s.key == "plan" {
            if let Some(name) = value_as_string(&s.value) {
                self.default_plan = Some(name);
            } else {
                self.err(s.span, "set plan expects string value");
            }
        }
        self.global_set.insert(s.key.clone(), s.value.clone());
    }

    /// Phase 2: checks that require symbol tables.
    fn check(&mut self, file: &hir::BuildFile) {
        // validate vars typed values
        for v in self.symbols.vars.values() {
            self.check_typed_value(v.span, &v.ty, &v.value, &format!("var `{}`", v.name));
        }

        // validate tools (capsule refs)
        for t in self.symbols.tools.values() {
            if let Some(c) = &t.capsule {
                if !self.symbols.capsules.contains_key(c) {
                    self.err(t.span, format!("tool `{}` references unknown capsule `{}`", t.name, c));
                }
            }
            if t.exec.is_none() {
                self.warn(t.span, format!("tool `{}` has no exec", t.name));
            }
        }

        // validate bakes: steps refer to ports/tools/vars
        for b in &file.items {
            if let hir::Item::Bake(bake) = b {
                self.check_bake(bake);
            }
        }

        // wires/exports/plans/switch
        for it in &file.items {
            match it {
                hir::Item::Wire(w) => self.check_wire(w),
                hir::Item::Export(e) => self.check_export(e),
                hir::Item::Plan(p) => self.check_plan_resolved(p),
                hir::Item::Switch(sw) => self.check_switch(sw),
                _ => {}
            }
        }

        // default plan must exist if set
        if let Some(p) = &self.default_plan {
            if !self.symbols.plans.contains_key(p) {
                self.err(Span::new(file.file, file.span_lo, file.span_lo), format!("default plan `{}` not found", p));
            }
        }
    }

    fn check_bake(&mut self, b: &hir::BakeDecl) {
        let sym = match self.symbols.bakes.get(&b.name) {
            Some(x) => x.clone(),
            None => return,
        };

        // Each step: validate refs
        for step in &b.steps {
            match step {
                hir::BakeStep::Make(m) => {
                    // make <ident> <kind> <string>
                    // the output variable must correspond to a declared out port OR a local alias;
                    // policy: require out port with same name for make outputs.
                    if !sym.ports_out.contains_key(&m.name) {
                        self.warn(m.span, format!(
                            "bake `{}`: make `{}` does not match an out port (recommended: declare out {}: ...)",
                            b.name, m.name, m.name
                        ));
                    }
                }
                hir::BakeStep::Run(r) => {
                    // run tool <toolname> ...
                    if !self.symbols.tools.contains_key(&r.tool) {
                        self.err(r.span, format!("bake `{}`: run references unknown tool `{}`", b.name, r.tool));
                    }
                    // takes/emits must refer to existing ports or vars
                    for item in &r.items {
                        match item {
                            hir::RunItem::Takes { span, ident, flag: _ } => {
                                if !sym.ports_in.contains_key(ident) && !self.symbols.vars.contains_key(ident) {
                                    self.err(*span, format!(
                                        "bake `{}`: takes `{}` must be an in port or a global var",
                                        b.name, ident
                                    ));
                                }
                            }
                            hir::RunItem::Emits { span, ident, flag: _ } => {
                                if !sym.ports_out.contains_key(ident) {
                                    self.err(*span, format!(
                                        "bake `{}`: emits `{}` must be an out port",
                                        b.name, ident
                                    ));
                                }
                            }
                            hir::RunItem::Set { span, flag: _, value } => {
                                // accept any scalar/list
                                if !is_value_valid(value) {
                                    self.err(*span, format!("bake `{}`: invalid run set value", b.name));
                                }
                            }
                        }
                    }
                }
                hir::BakeStep::Cache(c) => {
                    if !matches!(c.mode.as_str(), "content" | "mtime" | "off") {
                        self.err(c.span, format!("bake `{}`: invalid cache mode `{}`", b.name, c.mode));
                    }
                }
                hir::BakeStep::Output(o) => {
                    if !sym.ports_out.contains_key(&o.ident) {
                        self.err(o.span, format!("bake `{}`: output `{}` must reference an out port", b.name, o.ident));
                    }
                }
            }
        }
    }

    fn check_wire(&mut self, w: &hir::WireDecl) {
        let from = self.resolve_ref(&w.from, RefExpect::Out);
        let to = self.resolve_ref(&w.to, RefExpect::In);

        let (from, to) = match (from, to) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };

        // type compatibility
        let ty_from = ref_type(&from);
        let ty_to = ref_type(&to);

        if !type_compatible(&ty_from, &ty_to) {
            self.err(w.span, format!(
                "wire type mismatch: {} -> {} (from `{}` to `{}`)",
                ty_from.0, ty_to.0, ref_name(&from), ref_name(&to)
            ));
            return;
        }

        self.wires.push(ResolvedWire { span: w.span, from, to, ty: ty_from });
    }

    fn check_export(&mut self, e: &hir::ExportDecl) {
        let r = self.resolve_ref(&e.target, RefExpect::Out);
        let r = match r {
            Some(x) => x,
            None => return,
        };

        // exports must be out port
        match r {
            ResolvedRef::Port { dir: PortDir::Out, .. } => {}
            _ => {
                self.err(e.span, "export must reference an out port (bake.port)");
                return;
            }
        }

        self.exports.push(ResolvedExport { span: e.span, target: r, ty: ref_type(&r) });
    }

    fn check_plan_resolved(&mut self, p: &hir::PlanDecl) {
        let mut runs = Vec::new();
        for r in &p.runs {
            match r {
                hir::PlanRun::Exports { span } => runs.push(ResolvedPlanRun::Exports(*span)),
                hir::PlanRun::Ref { span, r } => {
                    let rr = self.resolve_ref(r, RefExpect::Any);
                    if let Some(rr) = rr {
                        runs.push(ResolvedPlanRun::Ref(rr));
                    } else {
                        self.err(*span, format!("plan `{}` references unknown ref", p.name));
                    }
                }
            }
        }
        self.plans.push(ResolvedPlan { span: p.span, name: p.name.clone(), runs });
    }

    fn check_switch(&mut self, sw: &hir::SwitchDecl) {
        // switch actions: set <ident> <value> | set plan "..." | run exports/ref
        for item in &sw.items {
            match item {
                hir::SwitchItem::Flag { span, flag: _, action } => match action {
                    hir::SwitchAction::Set { key, value } => {
                        if key == "plan" {
                            if value_as_string(value).is_none() {
                                self.err(*span, "switch: set plan expects string");
                            }
                        }
                        // accept setting unknown keys, but warn
                        if key != "plan" && key != "profile" {
                            self.warn(*span, format!("switch: setting unknown key `{}` (allowed: plan/profile/vars)", key));
                        }
                    }
                    hir::SwitchAction::SetPlan { span: _, name } => {
                        if !self.symbols.plans.contains_key(name) {
                            self.err(*span, format!("switch: unknown plan `{}`", name));
                        }
                    }
                    hir::SwitchAction::Run { span: _, target } => {
                        match target {
                            hir::RunTarget::Exports => {}
                            hir::RunTarget::Ref(r) => {
                                if self.resolve_ref(r, RefExpect::Any).is_none() {
                                    self.err(*span, "switch: run references unknown ref");
                                }
                            }
                        }
                    }
                },
            }
        }
    }

    /// --------------------------------------------------------
    /// Ref resolution
    /// --------------------------------------------------------

    fn resolve_ref(&mut self, r: &hir::Ref, expect: RefExpect) -> Option<ResolvedRef> {
        match r {
            hir::Ref::Var { span, name } => {
                let v = self.symbols.vars.get(name);
                if let Some(v) = v {
                    if matches!(expect, RefExpect::In | RefExpect::Out) {
                        // var can be used as input only by policy
                        if expect == RefExpect::Out {
                            self.err(*span, format!("ref `{}` is a var; expected out port", name));
                            return None;
                        }
                    }
                    return Some(ResolvedRef::GlobalVar { span: *span, name: name.clone(), ty: v.ty.clone() });
                }
                self.err(*span, format!("unknown var `{}`", name));
                None
            }
            hir::Ref::Port { span, bake, port } => {
                let b = match self.symbols.bakes.get(bake) {
                    Some(x) => x,
                    None => {
                        self.err(*span, format!("unknown bake `{}`", bake));
                        return None;
                    }
                };

                if let Some(p) = b.ports_out.get(port) {
                    if expect == RefExpect::In {
                        self.err(*span, format!("ref `{}`.{} is out port; expected in", bake, port));
                        return None;
                    }
                    return Some(ResolvedRef::Port {
                        span: *span,
                        bake: bake.clone(),
                        port: port.clone(),
                        dir: PortDir::Out,
                        ty: p.ty.clone(),
                    });
                }

                if let Some(p) = b.ports_in.get(port) {
                    if expect == RefExpect::Out {
                        self.err(*span, format!("ref `{}`.{} is in port; expected out", bake, port));
                        return None;
                    }
                    return Some(ResolvedRef::Port {
                        span: *span,
                        bake: bake.clone(),
                        port: port.clone(),
                        dir: PortDir::In,
                        ty: p.ty.clone(),
                    });
                }

                self.err(*span, format!("unknown port `{}` in bake `{}`", port, bake));
                None
            }
        }
    }

    /// --------------------------------------------------------
    /// Typed values
    /// --------------------------------------------------------

    fn check_typed_value(&mut self, span: Span, ty: &TypeRef, v: &hir::Value, ctx: &str) {
        // primitives
        if ty.0 == "text" {
            if value_as_string(v).is_none() {
                self.err(span, format!("{}: expected text/string", ctx));
            }
            return;
        }
        if ty.0 == "int" {
            if value_as_int(v).is_none() {
                self.err(span, format!("{}: expected int", ctx));
            }
            return;
        }
        if ty.0 == "bool" {
            if value_as_bool(v).is_none() {
                self.err(span, format!("{}: expected bool", ctx));
            }
            return;
        }
        if ty.0 == "bytes" {
            // bytes accepté comme string (base64/hex) ou list[int]
            if value_as_string(v).is_some() {
                return;
            }
            if let hir::Value::List(xs) = v {
                let ok = xs.iter().all(|x| matches!(x, hir::Value::Int(_)));
                if ok {
                    return;
                }
            }
            self.err(span, format!("{}: expected bytes (string or list[int])", ctx));
            return;
        }

        // artifact type: accept string (path/ref) or ident value
        // policy: keep permissive at buildfile level
        match v {
            hir::Value::String(_) | hir::Value::Ident(_) => {}
            hir::Value::List(_) => {}
            _ => self.warn(span, format!("{}: value kind unusual for artifact type {}", ctx, ty.0)),
        }
    }

    /// --------------------------------------------------------
    /// Diagnostics
    /// --------------------------------------------------------

    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error_at(span, msg.into()));
    }

    fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::warning_at(span, msg.into()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefExpect {
    In,
    Out,
    Any,
}

/// ------------------------------------------------------------
/// Helpers
/// ------------------------------------------------------------

fn ref_type(r: &ResolvedRef) -> TypeRef {
    match r {
        ResolvedRef::GlobalVar { ty, .. } => ty.clone(),
        ResolvedRef::Port { ty, .. } => ty.clone(),
    }
}

fn ref_name(r: &ResolvedRef) -> String {
    match r {
        ResolvedRef::GlobalVar { name, .. } => name.clone(),
        ResolvedRef::Port { bake, port, .. } => format!("{}.{}", bake, port),
    }
}

fn type_compatible(a: &TypeRef, b: &TypeRef) -> bool {
    // exact match for now
    a.0 == b.0
}

fn is_value_valid(v: &hir::Value) -> bool {
    match v {
        hir::Value::String(_) => true,
        hir::Value::Int(_) => true,
        hir::Value::Bool(_) => true,
        hir::Value::Ident(_) => true,
        hir::Value::List(xs) => xs.iter().all(is_value_valid),
    }
}

fn value_as_string(v: &hir::Value) -> Option<String> {
    match v {
        hir::Value::String(s) => Some(s.clone()),
        hir::Value::Ident(s) => Some(s.clone()),
        _ => None,
    }
}

fn value_as_int(v: &hir::Value) -> Option<i64> {
    match v {
        hir::Value::Int(x) => Some(*x),
        hir::Value::Ident(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn value_as_bool(v: &hir::Value) -> Option<bool> {
    match v {
        hir::Value::Bool(b) => Some(*b),
        hir::Value::Ident(s) => match s.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// ------------------------------------------------------------
/// Tests (structure-only; nécessite hir mock)
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_compat_exact() {
        assert!(type_compatible(&TypeRef::prim("text"), &TypeRef::prim("text")));
        assert!(!type_compatible(&TypeRef::prim("text"), &TypeRef::prim("int")));
    }
}