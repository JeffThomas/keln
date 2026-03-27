use std::collections::{HashMap, HashSet};

use crate::ast::{self, BinaryOp, Expr, FieldPattern, HelperDecl, Pattern, Program, TopLevelDecl};
use crate::vm::ir::{
    BuiltinTable, Instruction, KelnFn, KelnModule, SelectArm as IrSelectArm,
    TimeoutArm as IrTimeoutArm,
};

/// Sentinel: returned by lower_expr when a terminal instruction (RETURN/TAIL_CALL)
/// was emitted and no result register is available.
const NO_REG: usize = usize::MAX;

// =============================================================================
// Error type
// =============================================================================

#[derive(Debug)]
pub struct LowerError {
    pub message: String,
}

impl LowerError {
    fn new(msg: impl Into<String>) -> Self {
        LowerError { message: msg.into() }
    }
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lower error: {}", self.message)
    }
}

// =============================================================================
// Patch — forward jump to a not-yet-known IP
// =============================================================================

#[derive(Debug, Clone)]
enum PatchKind {
    Jump,
    MatchTagEq,
    MatchLitEq,
}

#[derive(Debug)]
struct Patch {
    instr_idx: usize,
    label:     String,
    kind:      PatchKind,
}

// =============================================================================
// Per-function lowering context
// =============================================================================

struct FnCtx {
    name:       String,
    next_reg:   usize,
    env_stack:  Vec<HashMap<String, usize>>,
    instrs:     Vec<Instruction>,
    patches:    Vec<Patch>,
    labels:     HashMap<String, usize>,
    next_label: usize,
    debug_names: Vec<Option<String>>,
}

impl FnCtx {
    fn new(name: impl Into<String>) -> Self {
        FnCtx {
            name: name.into(),
            next_reg: 1, // R0 is always the function input
            env_stack: vec![HashMap::new()],
            instrs: Vec::new(),
            patches: Vec::new(),
            labels: HashMap::new(),
            next_label: 0,
            debug_names: vec![Some("input".to_string())],
        }
    }

    fn alloc_reg(&mut self) -> usize {
        let r = self.next_reg;
        self.next_reg += 1;
        self.debug_names.push(None);
        r
    }

    fn alloc_named_reg(&mut self, name: &str) -> usize {
        let r = self.alloc_reg();
        let slot = r;
        if slot < self.debug_names.len() {
            self.debug_names[slot] = Some(name.to_string());
        }
        r
    }

    fn emit(&mut self, instr: Instruction) {
        self.instrs.push(instr);
    }

    fn current_ip(&self) -> usize {
        self.instrs.len()
    }

    fn fresh_label(&mut self) -> String {
        let l = format!("__L{}", self.next_label);
        self.next_label += 1;
        l
    }

    fn mark_label(&mut self, label: String) {
        self.labels.insert(label, self.current_ip());
    }

    fn push_scope(&mut self) {
        self.env_stack.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.env_stack.pop();
    }

    fn bind_var(&mut self, name: &str, reg: usize) {
        if let Some(top) = self.env_stack.last_mut() {
            top.insert(name.to_string(), reg);
        }
    }

    fn lookup_var(&self, name: &str) -> Option<usize> {
        for scope in self.env_stack.iter().rev() {
            if let Some(&r) = scope.get(name) {
                return Some(r);
            }
        }
        None
    }

    /// Emit a JUMP with a forward label — patched in fixup_patches.
    fn emit_jump(&mut self, label: String) {
        let idx = self.current_ip();
        self.instrs.push(Instruction::Jump { target_ip: 0 });
        self.patches.push(Patch { instr_idx: idx, label, kind: PatchKind::Jump });
    }

    /// Emit a MATCH_TAG_EQ with a forward label — patched in fixup_patches.
    fn emit_match_tag_eq(&mut self, tag_id: u32, src: usize, label: String) {
        let idx = self.current_ip();
        self.instrs.push(Instruction::MatchTagEq { tag_id, src, target_ip: 0 });
        self.patches.push(Patch { instr_idx: idx, label, kind: PatchKind::MatchTagEq });
    }

    /// Emit a MATCH_LIT_EQ with a forward label — patched in fixup_patches.
    fn emit_match_lit_eq(&mut self, const_idx: u32, src: usize, label: String) {
        let idx = self.current_ip();
        self.instrs.push(Instruction::MatchLitEq { const_idx, src, target_ip: 0 });
        self.patches.push(Patch { instr_idx: idx, label, kind: PatchKind::MatchLitEq });
    }

    fn fixup_patches(&mut self) -> Result<(), LowerError> {
        for patch in &self.patches {
            let target = *self.labels.get(&patch.label).ok_or_else(|| {
                LowerError::new(format!("unresolved label '{}'", patch.label))
            })?;
            match patch.kind {
                PatchKind::Jump => {
                    if let Some(Instruction::Jump { target_ip }) =
                        self.instrs.get_mut(patch.instr_idx)
                    {
                        *target_ip = target;
                    }
                }
                PatchKind::MatchTagEq => {
                    if let Some(Instruction::MatchTagEq { target_ip, .. }) =
                        self.instrs.get_mut(patch.instr_idx)
                    {
                        *target_ip = target;
                    }
                }
                PatchKind::MatchLitEq => {
                    if let Some(Instruction::MatchLitEq { target_ip, .. }) =
                        self.instrs.get_mut(patch.instr_idx)
                    {
                        *target_ip = target;
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<KelnFn, LowerError> {
        self.fixup_patches()?;
        let register_count = self.next_reg;
        Ok(KelnFn {
            name: self.name,
            register_count,
            instructions: self.instrs,
            debug_names: self.debug_names,
        })
    }
}

// =============================================================================
// Lowerer
// =============================================================================

pub struct Lowerer {
    module:   KelnModule,
    builtins: BuiltinTable,
    /// Names of all user-defined functions (for CALL vs CALL_BUILTIN disambiguation).
    user_fns: HashSet<String>,
}

impl Default for Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

impl Lowerer {
    pub fn new() -> Self {
        Lowerer {
            module:   KelnModule::new(),
            builtins: BuiltinTable::new(),
            user_fns: HashSet::new(),
        }
    }

    pub fn finish(self) -> KelnModule {
        self.module
    }

    // =========================================================================
    // Program lowering (two-pass: register all names first, then lower bodies)
    // =========================================================================

    pub fn lower_program(&mut self, program: &Program) -> Result<(), LowerError> {
        // Pass 1: collect all function names and register empty placeholder slots.
        // This makes all fn_idx values stable before any body is lowered,
        // enabling self-recursion and forward references.
        for decl in &program.declarations {
            if let TopLevelDecl::FnDecl(fd) = decl {
                if let Some(helpers) = &fd.helpers {
                    for helper in helpers {
                        let qname = match helper {
                            HelperDecl::Compact { name, .. } =>
                                format!("{}::{}", fd.name, name),
                            HelperDecl::Full(inner) =>
                                format!("{}::{}", fd.name, inner.name),
                        };
                        self.user_fns.insert(qname.clone());
                        self.module.add_fn(KelnFn::new(qname));
                    }
                }
                self.user_fns.insert(fd.name.clone());
                self.module.add_fn(KelnFn::new(&fd.name));
            }
        }

        // Pass 2: lower each function body and replace its placeholder slot.
        for decl in &program.declarations {
            if let TopLevelDecl::FnDecl(fd) = decl {
                if let Some(helpers) = &fd.helpers {
                    for helper in helpers {
                        let kfn = self.lower_helper_fn(&fd.name, helper)?;
                        let idx = self.module.fn_idx(&kfn.name)
                            .ok_or_else(|| LowerError::new(format!(
                                "helper '{}' not pre-registered", kfn.name)))?;
                        self.module.fns[idx] = kfn;
                    }
                }
                let kfn = self.lower_fn_decl(fd)?;
                let idx = self.module.fn_idx(&fd.name)
                    .ok_or_else(|| LowerError::new(format!(
                        "fn '{}' not pre-registered", fd.name)))?;
                self.module.fns[idx] = kfn;
            }
        }

        Ok(())
    }

    // =========================================================================
    // Helper lowering (returns KelnFn; caller places it at the pre-registered slot)
    // =========================================================================

    fn lower_helper_fn(&mut self, parent_name: &str, helper: &HelperDecl) -> Result<KelnFn, LowerError> {
        match helper {
            HelperDecl::Compact { name, body, .. } => {
                let qname = format!("{}::{}", parent_name, name);
                let mut ctx = FnCtx::new(&qname);
                ctx.bind_var("it", 0);
                ctx.bind_var("_input", 0);
                let result = self.lower_expr(&mut ctx, body, true)?;
                if result != NO_REG {
                    ctx.emit(Instruction::Return { src: result });
                }
                ctx.finish()
            }
            HelperDecl::Full(fd) => {
                let mut inner = fd.clone();
                inner.name = format!("{}::{}", parent_name, fd.name);
                inner.helpers = None;
                self.lower_fn_decl(&inner)
            }
        }
    }

    // =========================================================================
    // Function declaration lowering
    // =========================================================================

    fn lower_fn_decl(&mut self, fd: &ast::FnDecl) -> Result<KelnFn, LowerError> {
        let mut ctx = FnCtx::new(&fd.name);
        // Bind in: pattern to R0
        self.lower_in_pattern(&mut ctx, &fd.in_clause, 0)?;
        // Lower out: expression in tail position
        let result = self.lower_expr(&mut ctx, &fd.out_clause, true)?;
        if result != NO_REG {
            ctx.emit(Instruction::Return { src: result });
        }
        ctx.finish()
    }

    // =========================================================================
    // In-pattern binding (bind R0 to named variables)
    // =========================================================================

    fn lower_in_pattern(
        &mut self,
        ctx: &mut FnCtx,
        pattern: &Pattern,
        src: usize,
    ) -> Result<(), LowerError> {
        match pattern {
            Pattern::Binding(name, _) => {
                ctx.bind_var(name, src);
            }
            Pattern::Typed { name, .. } => {
                ctx.bind_var(name, src);
            }
            Pattern::Wildcard(_) => {}
            Pattern::Record { fields, .. } => {
                for fp in fields {
                    self.lower_field_pattern_binding(ctx, src, fp)?;
                }
            }
            Pattern::TupleVariant { inner, .. } => {
                let payload_reg = ctx.alloc_reg();
                ctx.emit(Instruction::VariantPayload { dst: payload_reg, src });
                self.lower_in_pattern(ctx, inner, payload_reg)?;
            }
            Pattern::RecordVariant { fields, .. } => {
                let payload_reg = ctx.alloc_reg();
                ctx.emit(Instruction::VariantPayload { dst: payload_reg, src });
                for fp in fields {
                    self.lower_field_pattern_binding(ctx, payload_reg, fp)?;
                }
            }
            Pattern::Literal(_) | Pattern::UnitVariant(_, _) => {}
            Pattern::List(_, _) => {}
        }
        Ok(())
    }

    fn lower_field_pattern_binding(
        &mut self,
        ctx: &mut FnCtx,
        record_reg: usize,
        fp: &FieldPattern,
    ) -> Result<(), LowerError> {
        match fp {
            FieldPattern::Named(field_name, pat) => {
                let field_reg = ctx.alloc_named_reg(field_name);
                let name_idx = self.module.constants.intern_str(field_name);
                ctx.emit(Instruction::FieldGetNamed { dst: field_reg, src: record_reg, name_idx });
                self.lower_in_pattern(ctx, pat, field_reg)?;
            }
            FieldPattern::Shorthand(field_name) => {
                let field_reg = ctx.alloc_named_reg(field_name);
                let name_idx = self.module.constants.intern_str(field_name);
                ctx.emit(Instruction::FieldGetNamed { dst: field_reg, src: record_reg, name_idx });
                ctx.bind_var(field_name, field_reg);
            }
            FieldPattern::Wildcard => {}
        }
        Ok(())
    }

    // =========================================================================
    // Expression lowering
    // =========================================================================

    fn lower_expr(
        &mut self,
        ctx: &mut FnCtx,
        expr: &Expr,
        tail: bool,
    ) -> Result<usize, LowerError> {
        match expr {
            // -----------------------------------------------------------------
            // Literals
            // -----------------------------------------------------------------
            Expr::IntLiteral(n, _) => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadInt { dst, val: *n });
                Ok(dst)
            }
            Expr::FloatLiteral(f, _) => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadFloat { dst, val: *f });
                Ok(dst)
            }
            Expr::BoolLiteral(b, _) => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadBool { dst, val: *b });
                Ok(dst)
            }
            Expr::StringLiteral(s, _) => {
                let const_idx = self.module.constants.intern_str(s);
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadStr { dst, const_idx });
                Ok(dst)
            }
            Expr::UnitLiteral(_) => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadUnit { dst });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Identifiers
            // -----------------------------------------------------------------
            Expr::Var(name, span) => {
                if let Some(reg) = ctx.lookup_var(name) {
                    // Return the bound register directly — sources are cloned by
                    // the consuming instruction (Add, Call, etc.) per the spec.
                    Ok(reg)
                } else if self.user_fns.contains(name.as_str()) {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::LoadFnRef { dst, name: name.clone() });
                    Ok(dst)
                } else {
                    Err(LowerError::new(format!(
                        "undefined variable '{}' at {}:{}",
                        name, span.line, span.column
                    )))
                }
            }

            Expr::UpperVar(name, _) => {
                // Zero-argument upper var = unit variant
                let tag_id = self.module.tags.intern(name);
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeVariant { dst, tag_id, payload: None });
                Ok(dst)
            }

            Expr::QualifiedName(parts, _) => {
                // e.g. List.map — produce a FnRef value for use with List.map etc.
                let name = parts.join(".");
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadFnRef { dst, name });
                Ok(dst)
            }

            Expr::Wildcard(_) => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadUnit { dst });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Parenthesized
            // -----------------------------------------------------------------
            Expr::Paren(inner, _) => self.lower_expr(ctx, inner, tail),

            // -----------------------------------------------------------------
            // Binary operations
            // -----------------------------------------------------------------
            Expr::BinaryOp { left, op, right, .. } => {
                let l = self.lower_expr(ctx, left, false)?;
                let r = self.lower_expr(ctx, right, false)?;
                let dst = ctx.alloc_reg();
                let instr = match op {
                    BinaryOp::Add => Instruction::Add { dst, src1: l, src2: r },
                    BinaryOp::Sub => Instruction::Sub { dst, src1: l, src2: r },
                    BinaryOp::Mul => Instruction::Mul { dst, src1: l, src2: r },
                    BinaryOp::Div => Instruction::Div { dst, src1: l, src2: r },
                    BinaryOp::Mod => Instruction::Rem { dst, src1: l, src2: r },
                    BinaryOp::Eq  => Instruction::Eq  { dst, src1: l, src2: r },
                    BinaryOp::Ne  => Instruction::Ne  { dst, src1: l, src2: r },
                    BinaryOp::Lt  => Instruction::Lt  { dst, src1: l, src2: r },
                    BinaryOp::Le  => Instruction::Le  { dst, src1: l, src2: r },
                    BinaryOp::Gt  => Instruction::Gt  { dst, src1: l, src2: r },
                    BinaryOp::Ge  => Instruction::Ge  { dst, src1: l, src2: r },
                };
                ctx.emit(instr);
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Field access (FieldGetNamed: runtime name lookup for all records in 4a)
            // -----------------------------------------------------------------
            Expr::FieldAccess { object, field, .. } => {
                let src = self.lower_expr(ctx, object, false)?;
                let name_idx = self.module.constants.intern_str(field);
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::FieldGetNamed { dst, src, name_idx });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Clone
            // -----------------------------------------------------------------
            Expr::Clone(inner, _) => {
                let src = self.lower_expr(ctx, inner, false)?;
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::Clone { dst, src });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Record construction
            // -----------------------------------------------------------------
            Expr::Record { name, fields, .. } => {
                // Gather field names in source order
                let field_names: Vec<String> = fields.iter().map(|fv| fv.name.clone()).collect();
                let layout_idx = self.module.layouts.register_anon(field_names.clone());

                // Lower each field value
                let mut field_regs = Vec::new();
                for fv in fields {
                    let r = self.lower_expr(ctx, &fv.value, false)?;
                    field_regs.push(r);
                }

                let dst = ctx.alloc_reg();

                match name {
                    Some(name_expr) => {
                        // Named record variant: Name { field: val, ... }
                        if let Expr::UpperVar(type_name, _) = name_expr.as_ref() {
                            // Build the record value first, then wrap in variant
                            let rec_dst = ctx.alloc_reg();
                            ctx.emit(Instruction::MakeRecord {
                                dst: rec_dst,
                                layout_idx,
                                fields: field_regs,
                            });
                            let tag_id = self.module.tags.intern(type_name);
                            ctx.emit(Instruction::MakeVariant {
                                dst,
                                tag_id,
                                payload: Some(rec_dst),
                            });
                        } else {
                            ctx.emit(Instruction::MakeRecord { dst, layout_idx, fields: field_regs });
                        }
                    }
                    None => {
                        ctx.emit(Instruction::MakeRecord { dst, layout_idx, fields: field_regs });
                    }
                }
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // List
            // -----------------------------------------------------------------
            Expr::List(items, _) => {
                let mut item_regs = Vec::new();
                for item in items {
                    let r = self.lower_expr(ctx, item, false)?;
                    item_regs.push(r);
                }
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeList { dst, items: item_regs });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Let binding (as expression — let in do blocks)
            // -----------------------------------------------------------------
            Expr::Let(binding) => {
                let val_reg = self.lower_expr(ctx, &binding.value, false)?;
                self.lower_in_pattern(ctx, &binding.pattern, val_reg)?;
                Ok(val_reg)
            }

            // -----------------------------------------------------------------
            // Do block
            // -----------------------------------------------------------------
            Expr::DoBlock { stmts, final_expr, .. } => {
                ctx.push_scope();
                for stmt in stmts {
                    self.lower_do_stmt(ctx, stmt)?;
                }
                let result = self.lower_expr(ctx, final_expr, tail)?;
                ctx.pop_scope();
                Ok(result)
            }

            // -----------------------------------------------------------------
            // Function call
            // -----------------------------------------------------------------
            Expr::Call { function, args, .. } => {
                self.lower_call(ctx, function, args, tail)
            }

            // -----------------------------------------------------------------
            // Pipeline
            // -----------------------------------------------------------------
            Expr::Pipeline { left, steps, .. } => {
                self.lower_pipeline(ctx, left, steps, tail)
            }

            // -----------------------------------------------------------------
            // Match
            // -----------------------------------------------------------------
            Expr::Match { scrutinee, arms, .. } => {
                let scr = self.lower_expr(ctx, scrutinee, false)?;
                self.lower_match(ctx, scr, arms, tail)
            }

            // -----------------------------------------------------------------
            // Partial application (With)
            // -----------------------------------------------------------------
            Expr::With { function, binding, .. } => {
                self.lower_with(ctx, function, binding)
            }

            // -----------------------------------------------------------------
            // Channel operations
            // -----------------------------------------------------------------
            Expr::ChannelNew { .. } => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::ChanNew { dst });
                Ok(dst)
            }
            Expr::ChannelNewCloseable { .. } => {
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::ChanNewCloseable { dst });
                Ok(dst)
            }
            Expr::ChannelClose { channel, .. } => {
                let chan_reg = self.lower_expr(ctx, channel, false)?;
                ctx.emit(Instruction::ChanClose { chan_reg });
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadUnit { dst });
                Ok(dst)
            }
            Expr::TypeRefExpr(type_expr, _) => {
                let name = match type_expr {
                    ast::TypeExpr::Named(n, _) => n.clone(),
                    ast::TypeExpr::Primitive(p, _) => format!("{:?}", p),
                    ast::TypeExpr::Generic { name, .. } => name.clone(),
                    _ => "Unknown".to_string(),
                };
                let dst = ctx.alloc_reg();
                let const_idx = self.module.constants.intern_str(&name);
                ctx.emit(Instruction::LoadStr { dst, const_idx });
                Ok(dst)
            }
            Expr::ChannelSend { channel, value, .. } => {
                let chan_reg = self.lower_expr(ctx, channel, false)?;
                let val_reg = self.lower_expr(ctx, value, false)?;
                ctx.emit(Instruction::ChanSend { chan_reg, val_reg });
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::LoadUnit { dst });
                Ok(dst)
            }
            Expr::ChannelRecv(chan_expr, _) => {
                let chan_reg = self.lower_expr(ctx, chan_expr, false)?;
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::ChanRecv { dst, chan_reg });
                Ok(dst)
            }

            // -----------------------------------------------------------------
            // Select
            // -----------------------------------------------------------------
            Expr::Select { arms, timeout, .. } => {
                self.lower_select(ctx, arms, timeout.as_ref())
            }
        }
    }

    // =========================================================================
    // Do-statement lowering
    // =========================================================================

    fn lower_do_stmt(&mut self, ctx: &mut FnCtx, stmt: &ast::DoStmt) -> Result<(), LowerError> {
        match stmt {
            ast::DoStmt::Expr(e) => {
                self.lower_expr(ctx, e, false)?;
                Ok(())
            }
            ast::DoStmt::Let(binding) => {
                let val_reg = self.lower_expr(ctx, &binding.value, false)?;
                self.lower_in_pattern(ctx, &binding.pattern, val_reg)?;
                Ok(())
            }
            ast::DoStmt::ChannelSend { channel, value } => {
                let chan_reg = self.lower_expr(ctx, channel, false)?;
                let val_reg = self.lower_expr(ctx, value, false)?;
                ctx.emit(Instruction::ChanSend { chan_reg, val_reg });
                Ok(())
            }
        }
    }

    // =========================================================================
    // Function call lowering
    // =========================================================================

    fn lower_call(
        &mut self,
        ctx: &mut FnCtx,
        function: &Expr,
        args: &[ast::Arg],
        tail: bool,
    ) -> Result<usize, LowerError> {
        match function {
            // Stdlib / qualified name call: List.map(fn, list)
            Expr::QualifiedName(parts, _) => {
                let name = parts.join(".");
                let arg_regs = self.lower_args(ctx, args)?;
                if let Some(builtin) = self.builtins.lookup(&name) {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin { dst, builtin, args: arg_regs });
                    Ok(dst)
                } else if self.user_fns.contains(&name) {
                    let fn_idx = self.resolve_fn_idx(&name);
                    let arg_reg = self.pack_args(ctx, arg_regs)?;
                    if tail {
                        ctx.emit(Instruction::TailCall { fn_idx, arg_reg });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::Call { dst, fn_idx, arg_reg });
                        Ok(dst)
                    }
                } else {
                    // Unknown qualified name — dispatch via stdlib fallback at runtime
                    let builtin_fallback = self.builtins.lookup(&name).unwrap_or(u16::MAX);
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin { dst, builtin: builtin_fallback, args: arg_regs });
                    Ok(dst)
                }
            }

            // Simple variable call: fn(arg) — user function
            Expr::Var(name, _) => {
                let arg_regs = self.lower_args(ctx, args)?;
                let arg_reg = self.pack_args(ctx, arg_regs)?;

                if let Some(builtin) = self.builtins.lookup(name) {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin { dst, builtin, args: vec![arg_reg] });
                    Ok(dst)
                } else if self.user_fns.contains(name.as_str()) {
                    // Two-pass guarantees fn_idx is already registered.
                    let fn_idx = self.module.fn_idx(name).unwrap_or(0);
                    if tail {
                        ctx.emit(Instruction::TailCall { fn_idx, arg_reg });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::Call { dst, fn_idx, arg_reg });
                        Ok(dst)
                    }
                } else if let Some(var_reg) = ctx.lookup_var(name) {
                    // Calling a FnRef variable
                    if tail {
                        ctx.emit(Instruction::TailCallDyn { fn_reg: var_reg, arg_reg });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::CallDyn { dst, fn_reg: var_reg, arg_reg });
                        Ok(dst)
                    }
                } else {
                    Err(LowerError::new(format!("call to unknown function '{}'", name)))
                }
            }

            // Variant constructor call: Ok(val), Some(val), etc.
            Expr::UpperVar(name, _) => {
                let arg_regs = self.lower_args(ctx, args)?;
                let payload_reg = self.pack_args(ctx, arg_regs)?;
                let tag_id = self.module.tags.intern(name);
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeVariant { dst, tag_id, payload: Some(payload_reg) });
                Ok(dst)
            }

            // Expression in function position (FnRef variable, etc.)
            other => {
                let fn_reg = self.lower_expr(ctx, other, false)?;
                let arg_regs = self.lower_args(ctx, args)?;
                let arg_reg = self.pack_args(ctx, arg_regs)?;
                if tail {
                    ctx.emit(Instruction::TailCallDyn { fn_reg, arg_reg });
                    Ok(NO_REG)
                } else {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallDyn { dst, fn_reg, arg_reg });
                    Ok(dst)
                }
            }
        }
    }

    fn resolve_fn_idx(&self, name: &str) -> usize {
        self.module.fn_idx(name).unwrap_or(0)
    }

    /// Lower a list of Arg to a list of registers.
    fn lower_args(&mut self, ctx: &mut FnCtx, args: &[ast::Arg]) -> Result<Vec<usize>, LowerError> {
        let mut regs = Vec::new();
        for arg in args {
            match arg {
                ast::Arg::Positional(e) => {
                    regs.push(self.lower_expr(ctx, e, false)?);
                }
                ast::Arg::Named(name, e) => {
                    let r = self.lower_expr(ctx, e, false)?;
                    // Named args are used to build a record — handled by pack_args
                    // For now just collect the register; pack_args needs names too
                    // TODO: named args proper handling
                    let _ = name;
                    regs.push(r);
                }
            }
        }
        Ok(regs)
    }

    /// Pack multiple arg registers into a single register.
    /// - 0 args → LOAD_UNIT
    /// - 1 arg → use directly
    /// - N args → MAKE_RECORD with positional fields
    fn pack_args(&mut self, ctx: &mut FnCtx, arg_regs: Vec<usize>) -> Result<usize, LowerError> {
        match arg_regs.len() {
            0 => {
                let r = ctx.alloc_reg();
                ctx.emit(Instruction::LoadUnit { dst: r });
                Ok(r)
            }
            1 => Ok(arg_regs[0]),
            _ => {
                // Multi-arg: build a positional record
                let fields: Vec<String> = (0..arg_regs.len())
                    .map(|i| format!("_{}", i))
                    .collect();
                let layout_idx = self.module.layouts.register_anon(fields);
                let dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeRecord { dst, layout_idx, fields: arg_regs });
                Ok(dst)
            }
        }
    }

    // =========================================================================
    // Pipeline lowering
    // =========================================================================

    fn lower_pipeline(
        &mut self,
        ctx: &mut FnCtx,
        left: &Expr,
        steps: &[Expr],
        tail: bool,
    ) -> Result<usize, LowerError> {
        let mut current = self.lower_expr(ctx, left, false)?;
        let last_idx = steps.len().saturating_sub(1);
        for (i, step) in steps.iter().enumerate() {
            let is_last = i == last_idx;
            let step_tail = tail && is_last;
            current = self.lower_pipeline_step(ctx, current, step, step_tail)?;
            if current == NO_REG {
                return Ok(NO_REG);
            }
        }
        Ok(current)
    }

    fn lower_pipeline_step(
        &mut self,
        ctx: &mut FnCtx,
        piped: usize,
        step: &Expr,
        tail: bool,
    ) -> Result<usize, LowerError> {
        match step {
            Expr::Var(name, _) => {
                if let Some(builtin) = self.builtins.lookup(name) {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin { dst, builtin, args: vec![piped] });
                    Ok(dst)
                } else if self.user_fns.contains(name.as_str()) {
                    let fn_idx = self.resolve_fn_idx(name);
                    if tail {
                        ctx.emit(Instruction::TailCall { fn_idx, arg_reg: piped });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::Call { dst, fn_idx, arg_reg: piped });
                        Ok(dst)
                    }
                } else if let Some(fn_reg) = ctx.lookup_var(name) {
                    if tail {
                        ctx.emit(Instruction::TailCallDyn { fn_reg, arg_reg: piped });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::CallDyn { dst, fn_reg, arg_reg: piped });
                        Ok(dst)
                    }
                } else {
                    Err(LowerError::new(format!("pipeline step: unknown function '{}'", name)))
                }
            }
            Expr::QualifiedName(parts, _) => {
                let name = parts.join(".");
                if let Some(builtin) = self.builtins.lookup(&name) {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin { dst, builtin, args: vec![piped] });
                    Ok(dst)
                } else if let Some(fn_idx) = self.module.fn_idx(&name) {
                    if tail {
                        ctx.emit(Instruction::TailCall { fn_idx, arg_reg: piped });
                        Ok(NO_REG)
                    } else {
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::Call { dst, fn_idx, arg_reg: piped });
                        Ok(dst)
                    }
                } else {
                    Err(LowerError::new(format!("pipeline step: unknown qualified name '{}'", name)))
                }
            }
            Expr::Call { function, args, .. } => {
                // Pipeline step with extra args: f(a, b) with piped prepended
                let mut all_regs = vec![piped];
                for arg in args {
                    match arg {
                        ast::Arg::Positional(e) => all_regs.push(self.lower_expr(ctx, e, false)?),
                        ast::Arg::Named(_, e) => all_regs.push(self.lower_expr(ctx, e, false)?),
                    }
                }
                match function.as_ref() {
                    Expr::QualifiedName(parts, _) => {
                        let name = parts.join(".");
                        if let Some(builtin) = self.builtins.lookup(&name) {
                            let dst = ctx.alloc_reg();
                            ctx.emit(Instruction::CallBuiltin { dst, builtin, args: all_regs });
                            Ok(dst)
                        } else {
                            Err(LowerError::new(format!("pipeline call: unknown '{}'", name)))
                        }
                    }
                    Expr::Var(name, _) => {
                        if let Some(builtin) = self.builtins.lookup(name) {
                            let dst = ctx.alloc_reg();
                            ctx.emit(Instruction::CallBuiltin { dst, builtin, args: all_regs });
                            Ok(dst)
                        } else {
                            let arg_reg = self.pack_args(ctx, all_regs)?;
                            if let Some(fn_idx) = self.module.fn_idx(name) {
                                let dst = ctx.alloc_reg();
                                ctx.emit(Instruction::Call { dst, fn_idx, arg_reg });
                                Ok(dst)
                            } else {
                                Err(LowerError::new(format!("pipeline call: unknown fn '{}'", name)))
                            }
                        }
                    }
                    _ => {
                        let arg_reg = self.pack_args(ctx, all_regs)?;
                        let fn_reg = self.lower_expr(ctx, function, false)?;
                        let dst = ctx.alloc_reg();
                        ctx.emit(Instruction::CallDyn { dst, fn_reg, arg_reg });
                        Ok(dst)
                    }
                }
            }
            other => {
                // expr in pipeline step position — call it as a FnRef
                let fn_reg = self.lower_expr(ctx, other, false)?;
                if tail {
                    ctx.emit(Instruction::TailCallDyn { fn_reg, arg_reg: piped });
                    Ok(NO_REG)
                } else {
                    let dst = ctx.alloc_reg();
                    ctx.emit(Instruction::CallDyn { dst, fn_reg, arg_reg: piped });
                    Ok(dst)
                }
            }
        }
    }

    // =========================================================================
    // Match lowering
    // =========================================================================

    fn lower_match(
        &mut self,
        ctx: &mut FnCtx,
        scrutinee: usize,
        arms: &[ast::MatchArm],
        tail: bool,
    ) -> Result<usize, LowerError> {
        if arms.is_empty() {
            return Err(LowerError::new("empty match expression"));
        }

        let match_end_label = ctx.fresh_label();
        let dst = if tail { NO_REG } else { ctx.alloc_reg() };

        for arm in arms.iter() {
            let body_label = ctx.fresh_label();
            let next_arm_label = ctx.fresh_label();

            // emit_pattern_test: jumps to body_label on match; falls through on no match.
            // fail_label is used by intermediate record field tests to short-circuit.
            let always_matches = self.emit_pattern_test(
                ctx, scrutinee, &arm.pattern, body_label.clone(), &next_arm_label,
            )?;

            if !always_matches {
                ctx.emit_jump(next_arm_label.clone());
            }

            ctx.mark_label(body_label);
            ctx.push_scope();
            self.bind_pattern_vars(ctx, scrutinee, &arm.pattern)?;

            if tail {
                let r = self.lower_expr(ctx, &arm.body, true)?;
                if r != NO_REG {
                    ctx.emit(Instruction::Return { src: r });
                }
            } else {
                let r = self.lower_expr(ctx, &arm.body, false)?;
                if r != dst {
                    ctx.emit(Instruction::LoadReg { dst, src: r });
                }
                ctx.emit_jump(match_end_label.clone());
            }

            ctx.pop_scope();
            ctx.mark_label(next_arm_label);

            if always_matches {
                break; // wildcard/binding — subsequent arms unreachable
            }
        }

        if !tail {
            ctx.mark_label(match_end_label);
        }

        Ok(if tail { NO_REG } else { dst })
    }

    /// Emit pattern test instructions.
    /// - On MATCH: jumps to `body_label`.
    /// - On NO MATCH: falls through (caller should emit JUMP next_arm after this returns).
    /// - `fail_label`: used by record patterns for intermediate field test failures.
    ///
    /// Returns `true` if the pattern always matches (wildcard/binding — no conditional test).
    fn emit_pattern_test(
        &mut self,
        ctx: &mut FnCtx,
        scrutinee: usize,
        pattern: &Pattern,
        body_label: String,
        fail_label: &str,
    ) -> Result<bool, LowerError> {
        match pattern {
            Pattern::Wildcard(_) | Pattern::Binding(_, _) | Pattern::Typed { .. } => {
                ctx.emit_jump(body_label);
                Ok(true)
            }

            Pattern::Literal(lit_expr) => {
                let const_idx = self.literal_to_const_idx(lit_expr)?;
                ctx.emit_match_lit_eq(const_idx, scrutinee, body_label);
                Ok(false)
            }

            Pattern::UnitVariant(name, _) => {
                let tag_id = self.module.tags.intern(name);
                ctx.emit_match_tag_eq(tag_id, scrutinee, body_label);
                Ok(false)
            }

            Pattern::TupleVariant { name, .. } => {
                let tag_id = self.module.tags.intern(name);
                ctx.emit_match_tag_eq(tag_id, scrutinee, body_label);
                Ok(false)
            }

            Pattern::RecordVariant { name, .. } => {
                let tag_id = self.module.tags.intern(name);
                ctx.emit_match_tag_eq(tag_id, scrutinee, body_label);
                Ok(false)
            }

            Pattern::Record { fields, .. } => {
                // Collect fields that need literal/variant tests.
                let literal_tests: Vec<(&str, &Pattern)> = fields
                    .iter()
                    .filter_map(|fp| {
                        if let FieldPattern::Named(fname, sub) = fp {
                            match sub {
                                Pattern::Literal(_) | Pattern::UnitVariant(_, _) =>
                                    Some((fname.as_str(), sub)),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                if literal_tests.is_empty() {
                    // Pure binding/wildcard record — always matches.
                    ctx.emit_jump(body_label);
                    return Ok(true);
                }

                let n = literal_tests.len();
                for (li, (field_name, sub_pattern)) in literal_tests.iter().enumerate() {
                    let is_last = li == n - 1;
                    let field_reg = ctx.alloc_reg();
                    let name_idx = self.module.constants.intern_str(field_name);
                    ctx.emit(Instruction::FieldGetNamed { dst: field_reg, src: scrutinee, name_idx });

                    match sub_pattern {
                        Pattern::Literal(lit_expr) => {
                            let const_idx = self.literal_to_const_idx(lit_expr)?;
                            if is_last {
                                // Last test: jump to body on match; fall-through = fail
                                ctx.emit_match_lit_eq(const_idx, field_reg, body_label.clone());
                            } else {
                                // Intermediate: match → ok_label, fall-through → fail_label
                                let ok_label = ctx.fresh_label();
                                ctx.emit_match_lit_eq(const_idx, field_reg, ok_label.clone());
                                ctx.emit_jump(fail_label.to_string());
                                ctx.mark_label(ok_label);
                            }
                        }
                        Pattern::UnitVariant(name, _) => {
                            let tag_id = self.module.tags.intern(name);
                            if is_last {
                                ctx.emit_match_tag_eq(tag_id, field_reg, body_label.clone());
                            } else {
                                let ok_label = ctx.fresh_label();
                                ctx.emit_match_tag_eq(tag_id, field_reg, ok_label.clone());
                                ctx.emit_jump(fail_label.to_string());
                                ctx.mark_label(ok_label);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(false)
            }

            Pattern::List(pats, _) => {
                if pats.is_empty() {
                    let is_empty = self.builtins.lookup("List.isEmpty").unwrap_or(26);
                    let empty_reg = ctx.alloc_reg();
                    ctx.emit(Instruction::CallBuiltin {
                        dst: empty_reg,
                        builtin: is_empty,
                        args: vec![scrutinee],
                    });
                    let true_idx = self.module.constants.intern_bool(true);
                    ctx.emit_match_lit_eq(true_idx, empty_reg, body_label);
                    Ok(false)
                } else {
                    // Non-empty list pattern — treated as always-match for now
                    ctx.emit_jump(body_label);
                    Ok(true)
                }
            }
        }
    }

    /// Convert a literal expression to a constant table index.
    fn literal_to_const_idx(&mut self, expr: &Expr) -> Result<u32, LowerError> {
        match expr {
            Expr::IntLiteral(n, _) => Ok(self.module.constants.intern_int(*n)),
            Expr::FloatLiteral(f, _) => Ok(self.module.constants.intern_float(*f)),
            Expr::BoolLiteral(b, _) => Ok(self.module.constants.intern_bool(*b)),
            Expr::StringLiteral(s, _) => Ok(self.module.constants.intern_str(s)),
            Expr::UnitLiteral(_) => Ok(self.module.constants.intern_unit()),
            other => Err(LowerError::new(format!("non-literal in pattern: {:?}", other))),
        }
    }

    /// Bind pattern variables to registers, assuming the pattern has already matched.
    fn bind_pattern_vars(
        &mut self,
        ctx: &mut FnCtx,
        scrutinee: usize,
        pattern: &Pattern,
    ) -> Result<(), LowerError> {
        match pattern {
            Pattern::Binding(name, _) => {
                ctx.bind_var(name, scrutinee);
            }
            Pattern::Typed { name, .. } => {
                ctx.bind_var(name, scrutinee);
            }
            Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::UnitVariant(_, _) => {}

            Pattern::TupleVariant { inner, .. } => {
                let payload_reg = ctx.alloc_reg();
                ctx.emit(Instruction::VariantPayload { dst: payload_reg, src: scrutinee });
                self.bind_pattern_vars(ctx, payload_reg, inner)?;
            }

            Pattern::RecordVariant { fields, .. } => {
                let payload_reg = ctx.alloc_reg();
                ctx.emit(Instruction::VariantPayload { dst: payload_reg, src: scrutinee });
                for fp in fields {
                    self.lower_field_pattern_binding(ctx, payload_reg, fp)?;
                }
            }

            Pattern::Record { fields, .. } => {
                for fp in fields {
                    match fp {
                        FieldPattern::Named(_, Pattern::Literal(_))
                        | FieldPattern::Named(_, Pattern::UnitVariant(_, _))
                        | FieldPattern::Wildcard => {}
                        _ => self.lower_field_pattern_binding(ctx, scrutinee, fp)?,
                    }
                }
            }

            Pattern::List(_, _) => {}
        }
        Ok(())
    }

    // =========================================================================
    // Partial application (With)
    // =========================================================================

    fn lower_with(
        &mut self,
        ctx: &mut FnCtx,
        function: &Expr,
        binding: &ast::WithBinding,
    ) -> Result<usize, LowerError> {
        // .with(param: value) or .with({ field: value, ... })
        // Creates a PartialFn value; lowered as a MAKE_PARTIAL instruction.
        // For Phase 4a, we produce a record that stores the function + bound args,
        // which exec.rs will handle.
        let fn_reg = self.lower_expr(ctx, function, false)?;
        let bound_reg = match binding {
            ast::WithBinding::Named(name, val_expr) => {
                let val_reg = self.lower_expr(ctx, val_expr, false)?;
                let layout_idx = self.module.layouts.register_anon(vec![name.clone()]);
                let rec_dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeRecord { dst: rec_dst, layout_idx, fields: vec![val_reg] });
                rec_dst
            }
            ast::WithBinding::Record(fields) => {
                let field_names: Vec<String> = fields.iter().map(|fv| fv.name.clone()).collect();
                let layout_idx = self.module.layouts.register_anon(field_names);
                let mut field_regs = Vec::new();
                for fv in fields {
                    field_regs.push(self.lower_expr(ctx, &fv.value, false)?);
                }
                let rec_dst = ctx.alloc_reg();
                ctx.emit(Instruction::MakeRecord { dst: rec_dst, layout_idx, fields: field_regs });
                rec_dst
            }
        };
        let dst = ctx.alloc_reg();
        ctx.emit(Instruction::MakePartial { dst, fn_reg, bound_reg });
        Ok(dst)
    }

    // =========================================================================
    // Select lowering
    // =========================================================================

    fn lower_select(
        &mut self,
        ctx: &mut FnCtx,
        arms: &[ast::SelectArm],
        timeout: Option<&ast::TimeoutArm>,
    ) -> Result<usize, LowerError> {
        // Lower all channel expressions first
        let mut ir_arms = Vec::new();
        for arm in arms {
            let channel_reg = self.lower_expr(ctx, &arm.channel, false)?;
            let binding_reg = if arm.binding == "_" {
                0
            } else {
                ctx.alloc_named_reg(&arm.binding)
            };
            // body_ip and end_ip will be patched after bodies are lowered
            ir_arms.push((binding_reg, channel_reg, arm.body.clone()));
        }

        let timeout_ir = if let Some(t) = timeout {
            let duration_reg = self.lower_expr(ctx, &t.duration, false)?;
            Some((duration_reg, t.body.clone()))
        } else {
            None
        };

        let select_end_label = ctx.fresh_label();
        let dst = ctx.alloc_reg();

        // Emit the SELECT instruction with placeholder body_ip / end_ip
        let select_ip = ctx.current_ip();
        let arm_body_labels: Vec<String> =
            ir_arms.iter().map(|_| ctx.fresh_label()).collect();
        let timeout_label = timeout_ir.as_ref().map(|_| ctx.fresh_label());

        // Build IrSelectArm placeholders
        let ir_select_arms: Vec<IrSelectArm> = ir_arms
            .iter()
            .zip(&arm_body_labels)
            .map(|((binding_reg, channel_reg, _), _)| IrSelectArm {
                binding_reg: *binding_reg,
                channel_reg: *channel_reg,
                body_ip: 0,
                end_ip: 0,
            })
            .collect();
        let ir_timeout = timeout_ir.as_ref().zip(timeout_label.as_ref()).map(|((dur_reg, _), _)| {
            IrTimeoutArm { duration_reg: *dur_reg, body_ip: 0 }
        });

        ctx.emit(Instruction::Select { dst, arms: ir_select_arms, timeout: ir_timeout });

        // Emit arm bodies after SELECT instruction
        for (i, (binding_reg, _, body)) in ir_arms.iter().enumerate() {
            ctx.mark_label(arm_body_labels[i].clone());
            if *binding_reg != 0 {
                // The runtime will have written the received value to binding_reg already
            }
            let arm_result = self.lower_expr(ctx, body, false)?;
            if arm_result != dst {
                ctx.emit(Instruction::LoadReg { dst, src: arm_result });
            }
            ctx.emit_jump(select_end_label.clone());
        }

        if let Some((_, timeout_body)) = &timeout_ir {
            ctx.mark_label(timeout_label.clone().unwrap());
            let tr = self.lower_expr(ctx, timeout_body, false)?;
            if tr != dst {
                ctx.emit(Instruction::LoadReg { dst, src: tr });
            }
            ctx.emit_jump(select_end_label.clone());
        }

        ctx.mark_label(select_end_label);

        // Patch the SELECT instruction with resolved body_ip values
        // This requires a post-pass since the labels were marked after the SELECT was emitted
        let resolved_arms: Vec<IrSelectArm> = ir_arms
            .iter()
            .zip(&arm_body_labels)
            .map(|((binding_reg, channel_reg, _), label)| {
                let body_ip = ctx.labels.get(label).copied().unwrap_or(0);
                IrSelectArm {
                    binding_reg: *binding_reg,
                    channel_reg: *channel_reg,
                    body_ip,
                    end_ip: 0, // not used in current impl
                }
            })
            .collect();
        let resolved_timeout = timeout_ir.as_ref().zip(timeout_label.as_ref()).map(|((dur_reg, _), label)| {
            IrTimeoutArm {
                duration_reg: *dur_reg,
                body_ip: ctx.labels.get(label).copied().unwrap_or(0),
            }
        });

        if let Some(Instruction::Select { arms, timeout, .. }) = ctx.instrs.get_mut(select_ip) {
            *arms = resolved_arms;
            *timeout = resolved_timeout;
        }

        Ok(dst)
    }
}

// =============================================================================
// Public API
// =============================================================================

pub fn lower_program(program: &Program) -> Result<KelnModule, String> {
    let mut lowerer = Lowerer::new();
    lowerer.lower_program(program).map_err(|e| e.message)?;
    Ok(lowerer.finish())
}
