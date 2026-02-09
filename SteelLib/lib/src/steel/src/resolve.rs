//! resolve.rs 
//!
//! Résolution AST -> HIR pour Steel Bakefile v2.
//!
//! - Entrée : AST (parser.rs) : `AstFile`
//! - Sortie : HIR (hir.rs) : `Program`
//!
//! Responsabilités :
//! - construire les tables de symboles (stores/capsules/tools/profiles/vars/bakes/ports/plans)
//! - résoudre `ref` (ident, bake.port) vers `ResolvedRef`
//! - injecter dans HIR des ids stables (StoreId, BakeId, PortId, …)
//! - valider : doublons, références inconnues, erreurs de forme sémantique simple
//! - préserver le déterminisme (BTreeMap, insertion stable)
//!
//! Notes :
//! - Ce module est volontairement “best-effort” : diagnostics + recovery.
//! - Les validations plus strictes (types, direction ports, coherences run bindings)
//!   sont aussi vérifiées en `lower.rs`.
//!
//! Dépendances :
//! - crate::parser::* (AST)
//! - crate::hir::* (HIR)
//! - crate::diag::* (diagnostics)

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{DiagBag, Diagnostic};
use crate::hir::*;
use crate::parser::*;

/// Résolution : construit un HIR et pousse les diagnostics dans `diags`.
pub fn resolve(ast: AstFile, mut interner: Interner, diags: &mut DiagBag) -> Program {
    let mut r = Resolver::new(interner.clone());
    let mut hir = r.resolve_file(ast);

    // Propager interner (celui du resolver est l’autorité)
    interner = r.interner.clone();
    hir.interner = interner;

    // Flush diagnostics
    diags.append(&mut r.diags);

    hir
}

/// ------------------------------------------------------------
/// Resolver
/// ------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Resolver {
    pub interner: Interner,
    pub diags: DiagBag,

    // symbol tables
    stores: BTreeMap<NameId, StoreId>,
    capsules: BTreeMap<NameId, CapsuleId>,
    tools: BTreeMap<NameId, ToolId>,
    profiles: BTreeMap<NameId, ProfileId>,
    vars: BTreeMap<NameId, VarId>,
    bakes: BTreeMap<NameId, BakeId>,
    plans: BTreeMap<NameId, PlanId>,

    // (bake_name, port_name) -> PortId
    ports: BTreeMap<(NameId, NameId), PortId>,
}

impl Resolver {
    pub fn new(interner: Interner) -> Self {
        Self { interner, diags: DiagBag::new(), ..Default::default() }
    }

    pub fn resolve_file(&mut self, ast: AstFile) -> Program {
        let mut hir = Program::new(ast.header.version);
        hir.interner = self.interner.clone();

        // Phase 0: sanity (version)
        if ast.header.version == 0 {
            self.diags.push(Diagnostic::error("invalid bakefile version (0)"));
        }

        // Phase 1: collect top-level definitions
        self.collect_defs(&ast, &mut hir);

        // Phase 2: fill bodies / resolve references
        self.resolve_bodies(&ast, &mut hir);

        // Phase 3: post checks (duplicates / unresolved)
        self.post_validate(&mut hir);

        hir
    }

    fn collect_defs(&mut self, ast: &AstFile, hir: &mut Program) {
        // globals set are collected as-is later

        // stores/capsules/tools/profiles/vars/bakes/plans
        for stmt in &ast.stmts {
            match stmt {
                AstStmt::Store(s) => self.collect_store(s, hir),
                AstStmt::Capsule(c) => self.collect_capsule(c, hir),
                AstStmt::Tool(t) => self.collect_tool(t, hir),
                AstStmt::Profile(p) => self.collect_profile(p, hir),
                AstStmt::Var(v) => self.collect_var(v, hir),
                AstStmt::Bake(b) => self.collect_bake_skeleton_and_ports(b, hir),
                AstStmt::Plan(p) => self.collect_plan_skeleton(p, hir),
                _ => {}
            }
        }
    }

    fn resolve_bodies(&mut self, ast: &AstFile, hir: &mut Program) {
        // Global sets
        for stmt in &ast.stmts {
            if let AstStmt::Set(s) = stmt {
                hir.globals.push(GlobalSet { key: s.key, value: ast_value_to_hir(&s.value), origin: Origin::none() });
            }
        }

        // Fill store/capsule/tool/profile fully (we already have skeletons)
        for stmt in &ast.stmts {
            match stmt {
                AstStmt::Store(s) => self.fill_store(s, hir),
                AstStmt::Capsule(c) => self.fill_capsule(c, hir),
                AstStmt::Tool(t) => self.fill_tool(t, hir),
                AstStmt::Profile(p) => self.fill_profile(p, hir),
                _ => {}
            }
        }

        // Fill bakes: makes, runs, cache, outputs_at
        for stmt in &ast.stmts {
            if let AstStmt::Bake(b) = stmt {
                self.fill_bake(b, hir);
            }
        }

        // wires/exports
        for stmt in &ast.stmts {
            match stmt {
                AstStmt::Wire(w) => self.resolve_wire(w, hir),
                AstStmt::Export(e) => self.resolve_export(e, hir),
                _ => {}
            }
        }

        // plans + switch
        for stmt in &ast.stmts {
            match stmt {
                AstStmt::Plan(p) => self.fill_plan(p, hir),
                AstStmt::Switch(s) => self.fill_switch(s, hir),
                _ => {}
            }
        }
    }

    fn post_validate(&mut self, hir: &mut Program) {
        // Ensure exports unique & stable
        {
            let mut set = BTreeSet::new();
            hir.exports.retain(|e| set.insert(e.what));
        }

        // Warn about unresolved refs (if any)
        for w in &hir.wires {
            if matches!(w.from, ResolvedRef::Unresolved(_)) || matches!(w.to, ResolvedRef::Unresolved(_)) {
                self.diags.push(Diagnostic::error("wire references unresolved symbol"));
            }
        }
        for e in &hir.exports {
            if matches!(e.what, ResolvedRef::Unresolved(_)) {
                self.diags.push(Diagnostic::error("export references unresolved symbol"));
            }
        }
        for p in &hir.plans {
            for it in &p.items {
                if let PlanItem::Run { what } = it {
                    if matches!(what, ResolvedRef::Unresolved(_)) {
                        self.diags.push(Diagnostic::error("plan references unresolved symbol"));
                    }
                }
            }
        }
        for sw in &hir.switches {
            for f in &sw.flags {
                match &f.action {
                    SwitchAction::RunRef { what } if matches!(what, ResolvedRef::Unresolved(_)) => {
                        self.diags.push(Diagnostic::error("switch run references unresolved symbol"));
                    }
                    _ => {}
                }
            }
        }
    }

    /// --------------------------------------------------------
    /// Collect skeletons
    /// --------------------------------------------------------

    fn collect_store(&mut self, s: &AstStore, hir: &mut Program) {
        if self.stores.contains_key(&s.name) {
            self.diags.push(Diagnostic::error(format!("store `{}` already defined", hir.name(s.name))));
            return;
        }
        let id = StoreId(hir.stores.len() as u32);

        // Skeleton with defaults, will be filled later
        hir.stores.push(Store {
            id,
            name: s.name,
            path: hir.interner.intern(String::new()),
            mode: StoreMode::Content,
            origin: Origin::none(),
        });
        self.stores.insert(s.name, id);
    }

    fn collect_capsule(&mut self, c: &AstCapsule, hir: &mut Program) {
        if self.capsules.contains_key(&c.name) {
            self.diags.push(Diagnostic::error(format!("capsule `{}` already defined", hir.name(c.name))));
            return;
        }
        let id = CapsuleId(hir.capsules.len() as u32);

        hir.capsules.push(Capsule {
            id,
            name: c.name,
            env: Vec::new(),
            fs: Vec::new(),
            net: Vec::new(),
            time: None,
            origin: Origin::none(),
        });
        self.capsules.insert(c.name, id);
    }

    fn collect_tool(&mut self, t: &AstTool, hir: &mut Program) {
        if self.tools.contains_key(&t.name) {
            self.diags.push(Diagnostic::error(format!("tool `{}` already defined", hir.name(t.name))));
            return;
        }
        let id = ToolId(hir.tools.len() as u32);

        hir.tools.push(Tool {
            id,
            name: t.name,
            exec: hir.interner.intern(String::new()),
            expect_version: false,
            sandbox: false,
            capsule: None,
            origin: Origin::none(),
        });
        self.tools.insert(t.name, id);
    }

    fn collect_profile(&mut self, p: &AstProfile, hir: &mut Program) {
        if self.profiles.contains_key(&p.name) {
            self.diags.push(Diagnostic::error(format!("profile `{}` already defined", hir.name(p.name))));
            return;
        }
        let id = ProfileId(hir.profiles.len() as u32);

        hir.profiles.push(Profile { id, name: p.name, settings: BTreeMap::new(), origin: Origin::none() });
        self.profiles.insert(p.name, id);
    }

    fn collect_var(&mut self, v: &AstVar, hir: &mut Program) {
        if self.vars.contains_key(&v.name) {
            self.diags.push(Diagnostic::error(format!("var `{}` already defined", hir.name(v.name))));
            return;
        }
        let id = VarId(hir.vars.len() as u32);

        hir.vars.push(VarDecl {
            id,
            name: v.name,
            ty: ast_type_to_hir(&v.ty),
            value: ast_value_to_hir(&v.value),
            origin: Origin::none(),
        });
        self.vars.insert(v.name, id);
    }

    fn collect_plan_skeleton(&mut self, p: &AstPlan, hir: &mut Program) {
        if self.plans.contains_key(&p.name) {
            self.diags.push(Diagnostic::error(format!("plan `{}` already defined", hir.name(p.name))));
            return;
        }
        let id = PlanId(hir.plans.len() as u32);

        hir.plans.push(Plan { id, name: p.name, items: Vec::new(), origin: Origin::none() });
        self.plans.insert(p.name, id);
    }

    fn collect_bake_skeleton_and_ports(&mut self, b: &AstBake, hir: &mut Program) {
        if self.bakes.contains_key(&b.name) {
            self.diags.push(Diagnostic::error(format!("bake `{}` already defined", hir.name(b.name))));
            return;
        }
        let id = BakeId(hir.bakes.len() as u32);

        // Create skeleton bake first
        hir.bakes.push(Bake {
            id,
            name: b.name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            runs: Vec::new(),
            makes: Vec::new(),
            cache: CacheMode::Content,
            outputs_at: Vec::new(),
            origin: Origin::none(),
        });
        self.bakes.insert(b.name, id);

        // Collect ports now (they define IDs used by wires/exports/run bindings)
        for it in &b.items {
            match it {
                AstBakeItem::InPort { name, ty, span } => {
                    self.collect_port(b.name, *name, PortDir::In, ty, *span, hir);
                }
                AstBakeItem::OutPort { name, ty, span } => {
                    self.collect_port(b.name, *name, PortDir::Out, ty, *span, hir);
                }
                _ => {}
            }
        }
    }

    fn collect_port(&mut self, bake: NameId, port: NameId, dir: PortDir, ty: &AstTypeRef, _span: Span, hir: &mut Program) {
        let key = (bake, port);
        if self.ports.contains_key(&key) {
            self.diags.push(Diagnostic::error(format!(
                "port `{}` already defined in bake `{}`",
                hir.name(port),
                hir.name(bake)
            )));
            return;
        }

        let pid = PortId(hir.ports.len() as u32);
        hir.ports.push(Port { id: pid, name: port, dir, ty: ast_type_to_hir(ty), origin: Origin::none() });
        self.ports.insert(key, pid);

        // attach to bake skeleton
        let bid = self.bakes[&bake];
        let bake_mut = hir.bake_mut(bid);
        match dir {
            PortDir::In => bake_mut.inputs.push(pid),
            PortDir::Out => bake_mut.outputs.push(pid),
        }
    }

    /// --------------------------------------------------------
    /// Fill skeletons
    /// --------------------------------------------------------

    fn fill_store(&mut self, s: &AstStore, hir: &mut Program) {
        let sid = match self.stores.get(&s.name).copied() {
            Some(x) => x,
            None => return,
        };

        let st = hir.store_mut(sid);

        for it in &s.items {
            match it {
                AstStoreItem::Path(p) => st.path = *p,
                AstStoreItem::Mode(m) => {
                    st.mode = match hir.name(*m) {
                        "mtime" => StoreMode::Mtime,
                        "off" => StoreMode::Off,
                        _ => StoreMode::Content,
                    };
                }
            }
        }

        if hir.name(st.path).is_empty() {
            self.diags.push(Diagnostic::error(format!("store `{}` has empty path", hir.name(st.name))));
        }
    }

    fn fill_capsule(&mut self, c: &AstCapsule, hir: &mut Program) {
        let cid = match self.capsules.get(&c.name).copied() {
            Some(x) => x,
            None => return,
        };
        let cap = hir.capsule_mut(cid);

        for it in &c.items {
            match it {
                AstCapsuleItem::Env { kind, list, .. } => {
                    let k = hir.name(*kind);
                    let pol = if k == "deny" { EnvPolicyKind::Deny } else { EnvPolicyKind::Allow };
                    cap.env.push(EnvPolicy { kind: pol, vars: list.clone() });
                }
                AstCapsuleItem::Fs { kind, list, .. } => {
                    let k = hir.name(*kind);
                    let pol = match k {
                        "allow_read" => FsPolicyKind::AllowRead,
                        "allow_write" => FsPolicyKind::AllowWrite,
                        "allow_write_exact" => FsPolicyKind::AllowWriteExact,
                        _ => FsPolicyKind::Deny,
                    };
                    cap.fs.push(FsPolicy { kind: pol, paths: list.clone() });
                }
                AstCapsuleItem::Net { kind, .. } => {
                    let pol = if hir.name(*kind) == "deny" { NetPolicy::Deny } else { NetPolicy::Allow };
                    cap.net.push(pol);
                }
                AstCapsuleItem::TimeStable { value, .. } => {
                    cap.time = Some(TimePolicy { stable: *value });
                }
            }
        }
    }

    fn fill_tool(&mut self, t: &AstTool, hir: &mut Program) {
        let tid = match self.tools.get(&t.name).copied() {
            Some(x) => x,
            None => return,
        };
        let tool = hir.tool_mut(tid);

        for it in &t.items {
            match it {
                AstToolItem::Exec(s) => tool.exec = *s,
                AstToolItem::ExpectVersion(s) => {
                    // on garde le texte, mais HIR a un bool; on fait "true si non vide"
                    tool.expect_version = !hir.name(*s).is_empty();
                }
                AstToolItem::Sandbox(b) => tool.sandbox = *b,
                AstToolItem::Capsule(name) => {
                    if let Some(cid) = self.capsules.get(name).copied() {
                        tool.capsule = Some(cid);
                    } else {
                        self.diags.push(Diagnostic::error(format!(
                            "tool `{}` references unknown capsule `{}`",
                            hir.name(tool.name),
                            hir.name(*name)
                        )));
                    }
                }
            }
        }

        if hir.name(tool.exec).is_empty() {
            self.diags.push(Diagnostic::error(format!("tool `{}` has empty exec", hir.name(tool.name))));
        }
    }

    fn fill_profile(&mut self, p: &AstProfile, hir: &mut Program) {
        let pid = match self.profiles.get(&p.name).copied() {
            Some(x) => x,
            None => return,
        };
        let prof = hir.profile_mut(pid);

        for it in &p.items {
            let val = ast_value_to_hir(&it.value);
            prof.settings.insert(it.key, val);
        }
    }

    fn fill_bake(&mut self, b: &AstBake, hir: &mut Program) {
        let bid = match self.bakes.get(&b.name).copied() {
            Some(x) => x,
            None => return,
        };
        let bake = hir.bake_mut(bid);

        // Ports sont déjà collectés; ici on ne traite que les items “non-port”.
        for it in &b.items {
            match it {
                AstBakeItem::InPort { .. } | AstBakeItem::OutPort { .. } => {}
                AstBakeItem::Cache { mode, .. } => {
                    bake.cache = match hir.name(*mode) {
                        "mtime" => CacheMode::Mtime,
                        "off" => CacheMode::Off,
                        _ => CacheMode::Content,
                    };
                }
                AstBakeItem::OutputAt { port, at, .. } => {
                    let pid = self.resolve_port(b.name, *port, hir);
                    if let Some(pid) = pid {
                        bake.outputs_at.push(OutputAt { port: pid, at: *at });
                    }
                }
                AstBakeItem::Make { name, kind, arg, .. } => {
                    let mk = match hir.name(*kind) {
                        "glob" => MakeKind::Glob,
                        "file" => MakeKind::File,
                        "text" => MakeKind::Text,
                        "value" => MakeKind::Value,
                        _ => MakeKind::Text,
                    };
                    bake.makes.push(Make { name: *name, kind: mk, arg: *arg, origin: Origin::none() });
                }
                AstBakeItem::Run(rb) => {
                    let tid = match self.tools.get(&rb.tool).copied() {
                        Some(x) => x,
                        None => {
                            self.diags.push(Diagnostic::error(format!(
                                "bake `{}` references unknown tool `{}`",
                                hir.name(b.name),
                                hir.name(rb.tool)
                            )));
                            continue;
                        }
                    };

                    let mut run = Run { tool: tid, takes: Vec::new(), emits: Vec::new(), sets: Vec::new(), origin: Origin::none() };

                    for item in &rb.items {
                        match item {
                            AstRunItem::Takes { port, flag, .. } => {
                                if let Some(pid) = self.resolve_port(b.name, *port, hir) {
                                    run.takes.push(RunBinding { port: pid, as_flag: *flag });
                                }
                            }
                            AstRunItem::Emits { port, flag, .. } => {
                                if let Some(pid) = self.resolve_port(b.name, *port, hir) {
                                    run.emits.push(RunBinding { port: pid, as_flag: *flag });
                                }
                            }
                            AstRunItem::Set { flag, value, .. } => {
                                run.sets.push(RunSet { flag: *flag, value: ast_value_to_hir(value) });
                            }
                        }
                    }

                    bake.runs.push(run);
                }
            }
        }
    }

    fn fill_plan(&mut self, p: &AstPlan, hir: &mut Program) {
        let pid = match self.plans.get(&p.name).copied() {
            Some(x) => x,
            None => return,
        };
        let plan = hir.plan_mut(pid);

        for it in &p.items {
            match it {
                AstPlanItem::RunExports { .. } => plan.items.push(PlanItem::RunExports),
                AstPlanItem::RunRef { what, .. } => plan.items.push(PlanItem::Run { what: self.resolve_ref(what, hir) }),
            }
        }
    }

    fn fill_switch(&mut self, s: &AstSwitch, hir: &mut Program) {
        let mut flags = Vec::new();

        for f in &s.flags {
            let action = match &f.action {
                AstSwitchAction::Set { key, value, .. } => SwitchAction::Set { key: *key, value: ast_value_to_hir(value) },
                AstSwitchAction::SetPlan { plan, .. } => SwitchAction::SetPlan { plan: *plan },
                AstSwitchAction::RunExports { .. } => SwitchAction::RunExports,
                AstSwitchAction::RunRef { what, .. } => SwitchAction::RunRef { what: self.resolve_ref(what, hir) },
            };
            flags.push(SwitchFlag { flag: f.flag, action });
        }

        hir.switches.push(Switch { flags, origin: Origin::none() });
    }

    fn resolve_wire(&mut self, w: &AstWire, hir: &mut Program) {
        let from = self.resolve_ref(&w.from, hir);
        let to = self.resolve_ref(&w.to, hir);
        hir.wires.push(Wire { from, to, origin: Origin::none() });
    }

    fn resolve_export(&mut self, e: &AstExport, hir: &mut Program) {
        let what = self.resolve_ref(&e.what, hir);
        hir.exports.push(Export { what, origin: Origin::none() });
    }

    /// --------------------------------------------------------
    /// Reference resolution
    /// --------------------------------------------------------

    fn resolve_port(&mut self, bake: NameId, port: NameId, hir: &mut Program) -> Option<PortId> {
        match self.ports.get(&(bake, port)).copied() {
            Some(pid) => Some(pid),
            None => {
                self.diags.push(Diagnostic::error(format!(
                    "unknown port `{}` in bake `{}`",
                    hir.name(port),
                    hir.name(bake)
                )));
                None
            }
        }
    }

    fn resolve_ref(&mut self, r: &AstRef, hir: &mut Program) -> ResolvedRef {
        match r {
            AstRef::Var(name) => self.resolve_bare_ident(*name, hir),
            AstRef::BakePort { bake, port, .. } => {
                if let Some(pid) = self.ports.get(&(*bake, *port)).copied() {
                    ResolvedRef::Port(pid)
                } else {
                    self.diags.push(Diagnostic::error(format!(
                        "unknown port ref `{}`.`{}`",
                        hir.name(*bake),
                        hir.name(*port)
                    )));
                    ResolvedRef::Unresolved(*port)
                }
            }
        }
    }

    fn resolve_bare_ident(&mut self, name: NameId, hir: &mut Program) -> ResolvedRef {
        if let Some(id) = self.vars.get(&name).copied() {
            return ResolvedRef::Var(id);
        }
        if let Some(id) = self.bakes.get(&name).copied() {
            return ResolvedRef::Bake(id);
        }
        if let Some(id) = self.tools.get(&name).copied() {
            return ResolvedRef::Tool(id);
        }
        if let Some(id) = self.profiles.get(&name).copied() {
            return ResolvedRef::Profile(id);
        }
        if let Some(id) = self.capsules.get(&name).copied() {
            return ResolvedRef::Capsule(id);
        }
        if let Some(id) = self.stores.get(&name).copied() {
            return ResolvedRef::Store(id);
        }
        if let Some(id) = self.plans.get(&name).copied() {
            return ResolvedRef::Plan(id);
        }

        self.diags.push(Diagnostic::error(format!("unresolved identifier `{}`", hir.name(name))));
        ResolvedRef::Unresolved(name)
    }
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn resolve_smoke() {
        let src = r#"
steel bake 2

store cache
  path "./.steel/store"
  mode content
.end

capsule cap
  env allow ["PATH"]
  fs allow_read ["./src"]
  net deny
  time stable true
.end

tool vittec
  exec "vittec"
  sandbox true
  capsule cap
.end

profile debug
  set opt 0
  set symbols true
.end

var target: text = "x86_64-apple-darwin"

bake app
  in src: src.glob
  out exe: bin.exe
  make src glob "src/**/*.vit"
  run tool vittec
    takes src as "--src"
    emits exe as "--out"
    set "--emit" "exe"
  .end
  cache content
  output exe at "./out/app"
.end

export app.exe
plan default
  run exports
.end
"#;

        let mut diags = DiagBag::new();
        let mut interner = Interner::new();
        let toks = Lexer::new(0, src).lex_all(&mut diags);
        let mut p = Parser::new(toks, &mut interner, &mut diags);
        let ast = p.parse_file();

        let _hir = resolve(ast, interner, &mut diags);
        assert!(!diags.has_error());
    }

    #[test]
    fn unresolved_is_error() {
        let src = "steel bake 2\nwire a -> b\n";
        let mut diags = DiagBag::new();
        let mut interner = Interner::new();
        let toks = Lexer::new(0, src).lex_all(&mut diags);
        let mut p = Parser::new(toks, &mut interner, &mut diags);
        let ast = p.parse_file();
        let _hir = resolve(ast, interner, &mut diags);
        assert!(diags.has_error());
    }
}