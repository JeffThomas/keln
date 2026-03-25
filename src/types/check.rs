use std::collections::HashMap;

use crate::ast;
use crate::ast::Span;
use super::{Type, TypeError, EffectSet, TypeDef, VariantDef, VariantPayload, FnSig};
use super::env::TypeEnv;

// =============================================================================
// Checker — walks the AST and collects type errors
// =============================================================================

pub struct Checker {
    pub env: TypeEnv,
    pub errors: Vec<TypeError>,
    /// The effect set of the function currently being checked
    current_effects: EffectSet,
}

impl Checker {
    pub fn new() -> Self {
        Checker {
            env: TypeEnv::new(),
            errors: Vec::new(),
            current_effects: EffectSet::pure_set(),
        }
    }

    fn err(&mut self, msg: impl Into<String>, span: &Span) {
        self.errors.push(TypeError::new(msg, span));
    }

    // =========================================================================
    // Program
    // =========================================================================

    pub fn check(&mut self, program: &ast::Program) {
        // First pass: register all type definitions and function signatures
        for decl in &program.declarations {
            self.register_decl(decl);
        }
        // Second pass: check function bodies and expressions
        for decl in &program.declarations {
            self.check_decl(decl);
        }
    }

    // =========================================================================
    // Registration pass — collect types and signatures
    // =========================================================================

    fn register_decl(&mut self, decl: &ast::TopLevelDecl) {
        match decl {
            ast::TopLevelDecl::TypeDecl(td) => self.register_type_decl(td),
            ast::TopLevelDecl::FnDecl(fd) => self.register_fn_decl(fd),
            ast::TopLevelDecl::ModuleDecl(md) => self.register_module_decl(md),
            ast::TopLevelDecl::TrustedModuleDecl(tmd) => self.register_trusted_module_decl(tmd),
            ast::TopLevelDecl::EffectDecl(ed) => self.register_effect_decl(ed),
            ast::TopLevelDecl::LetBinding(_) => {} // checked in second pass
        }
    }

    fn register_type_decl(&mut self, td: &ast::TypeDecl) {
        let params = td.type_params.clone();
        let def = match &td.def {
            ast::TypeDef::Sum(variants) => {
                let vdefs: Vec<VariantDef> = variants.iter().map(|v| {
                    VariantDef {
                        name: v.name.clone(),
                        payload: self.resolve_variant_payload(&v.payload, &params),
                    }
                }).collect();
                TypeDef::Sum { type_params: params, variants: vdefs }
            }
            ast::TypeDef::Product(fields) => {
                let flds = self.resolve_field_list(fields, &params);
                TypeDef::Product { type_params: params, fields: flds }
            }
            ast::TypeDef::Refinement { base, .. } => {
                let base_ty = self.resolve_type_expr(base, &params);
                TypeDef::Refinement { type_params: params, base: base_ty }
            }
            ast::TypeDef::Alias(te) => {
                let target = self.resolve_type_expr(te, &params);
                TypeDef::Alias { type_params: params, target }
            }
        };
        self.env.register_type(&td.name, def);
    }

    fn register_fn_decl(&mut self, fd: &ast::FnDecl) {
        let effects = EffectSet::from_names(&fd.signature.effects.effects);
        let input = self.resolve_type_expr(&fd.signature.input_type, &[]);
        let output = self.resolve_type_expr(&fd.signature.output_type, &[]);
        self.env.register_fn(&fd.name, FnSig { effects, input, output });
    }

    fn register_module_decl(&mut self, md: &ast::ModuleDecl) {
        let mut methods = HashMap::new();
        for sig in &md.provides {
            let effects = EffectSet::from_names(&sig.effects.effects);
            let input = self.resolve_type_expr(&sig.input_type, &[]);
            let output = self.resolve_type_expr(&sig.output_type, &[]);
            methods.insert(sig.name.clone(), FnSig { effects, input, output });
        }
        self.env.register_module(&md.name, methods);
    }

    fn register_trusted_module_decl(&mut self, tmd: &ast::TrustedModuleDecl) {
        let mut methods = HashMap::new();
        for sig in &tmd.provides {
            let effects = EffectSet::from_names(&sig.effects.effects);
            let input = self.resolve_type_expr(&sig.input_type, &[]);
            let output = self.resolve_type_expr(&sig.output_type, &[]);
            methods.insert(sig.name.clone(), FnSig { effects, input, output });
        }
        self.env.register_module(&tmd.name, methods);
    }

    fn register_effect_decl(&mut self, ed: &ast::EffectDecl) {
        let methods: Vec<FnSig> = ed.methods.iter().map(|sig| {
            let effects = EffectSet::from_names(&sig.effects.effects);
            let input = self.resolve_type_expr(&sig.input_type, &[]);
            let output = self.resolve_type_expr(&sig.output_type, &[]);
            FnSig { effects, input, output }
        }).collect();
        self.env.known_effects.insert(ed.name.clone(), methods);
    }

    // =========================================================================
    // Type expression resolution — AST TypeExpr -> internal Type
    // =========================================================================

    pub fn resolve_type_expr(&self, te: &ast::TypeExpr, type_params: &[String]) -> Type {
        match te {
            ast::TypeExpr::Primitive(p, _) => match p {
                ast::PrimitiveType::Int => Type::Int,
                ast::PrimitiveType::Float => Type::Float,
                ast::PrimitiveType::Bool => Type::Bool,
                ast::PrimitiveType::String => Type::String,
                ast::PrimitiveType::Bytes => Type::Bytes,
                ast::PrimitiveType::Unit => Type::Unit,
            },
            ast::TypeExpr::Never(_) => Type::Never,
            ast::TypeExpr::Named(name, _) => {
                if type_params.contains(name) {
                    Type::TypeVar(name.clone())
                } else {
                    Type::Named(name.clone())
                }
            }
            ast::TypeExpr::Generic { name, args, .. } => {
                let resolved_args: Vec<Type> = args.iter()
                    .map(|a| self.resolve_type_expr(a, type_params))
                    .collect();
                // Recognize List<T> and Channel<T> specially
                match name.as_str() {
                    "List" if resolved_args.len() == 1 => Type::List(Box::new(resolved_args[0].clone())),
                    "Channel" if resolved_args.len() == 1 => Type::Channel(Box::new(resolved_args[0].clone())),
                    "Task" if resolved_args.len() == 1 => Type::Task(Box::new(resolved_args[0].clone())),
                    _ => Type::Generic { name: name.clone(), args: resolved_args }
                }
            }
            ast::TypeExpr::Product(fields, _) => {
                let flds = self.resolve_field_list(fields, type_params);
                Type::Record(flds)
            }
            ast::TypeExpr::FunctionRef { effect, input, output, .. } => {
                let eff = EffectSet::from_names(&effect.effects);
                let inp = self.resolve_type_expr(input, type_params);
                let out = self.resolve_type_expr(output, type_params);
                Type::FunctionRef { effects: eff, input: Box::new(inp), output: Box::new(out) }
            }
        }
    }

    fn resolve_field_list(&self, fields: &[ast::FieldTypeDecl], type_params: &[String]) -> Vec<(String, Type)> {
        fields.iter().map(|f| {
            (f.name.clone(), self.resolve_type_expr(&f.type_expr, type_params))
        }).collect()
    }

    fn resolve_variant_payload(&self, payload: &ast::VariantPayload, type_params: &[String]) -> VariantPayload {
        match payload {
            ast::VariantPayload::Unit => VariantPayload::Unit,
            ast::VariantPayload::Tuple(te) => {
                VariantPayload::Tuple(self.resolve_type_expr(te, type_params))
            }
            ast::VariantPayload::Record(fields) => {
                VariantPayload::Record(self.resolve_field_list(fields, type_params))
            }
        }
    }

    // =========================================================================
    // Checking pass — validate function bodies
    // =========================================================================

    fn check_decl(&mut self, decl: &ast::TopLevelDecl) {
        match decl {
            ast::TopLevelDecl::FnDecl(fd) => self.check_fn_decl(fd),
            ast::TopLevelDecl::LetBinding(lb) => { self.check_let_binding(lb); }
            _ => {} // type/module/effect decls already registered
        }
    }

    fn check_fn_decl(&mut self, fd: &ast::FnDecl) {
        let sig = match self.env.lookup_fn(&fd.name) {
            Some(s) => s.clone(),
            None => return, // shouldn't happen
        };

        // Set current effects context
        let prev_effects = self.current_effects.clone();
        self.current_effects = sig.effects.clone();

        self.env.push_scope();

        // Bind input pattern to input type
        self.bind_pattern(&fd.in_clause, &sig.input);

        // Check output expression and verify type matches signature
        let out_type = self.infer_expr(&fd.out_clause);
        self.check_assignable(&out_type, &sig.output, &fd.span);

        // If the function declares Never return type, validate that the out expression
        // is a tail call (direct recursion) or a do block ending in a tail call (spec §3.1)
        if sig.output == Type::Never {
            if !self.is_valid_never_expr(&fd.out_clause, &fd.name) {
                self.err(
                    "function returning Never must end with a tail call to itself (directly or as the final expression of a do block)",
                    &fd.span,
                );
            }
        }

        // Check verify block if present
        if let Some(verify) = &fd.verify {
            for stmt in verify {
                self.check_verify_stmt(stmt, &sig);
            }
        }

        // Check helpers if present
        if let Some(helpers) = &fd.helpers {
            for helper in helpers {
                self.check_helper_decl(helper);
            }
        }

        self.env.pop_scope();
        self.current_effects = prev_effects;
    }

    fn check_helper_decl(&mut self, helper: &ast::HelperDecl) {
        match helper {
            ast::HelperDecl::Full(fd) => {
                // Register and check the full fn decl
                self.register_fn_decl(fd);
                self.check_fn_decl(fd);
            }
            ast::HelperDecl::Compact { name, effects, input_type, output_type, body, span, .. } => {
                let eff = EffectSet::from_names(&effects.effects);
                let inp = self.resolve_type_expr(input_type, &[]);
                let out = self.resolve_type_expr(output_type, &[]);
                self.env.register_fn(name, FnSig { effects: eff.clone(), input: inp.clone(), output: out.clone() });

                let prev_effects = self.current_effects.clone();
                self.current_effects = eff;
                self.env.push_scope();
                // Compact helpers have the input bound implicitly
                self.env.bind("_input", inp);
                let body_type = self.infer_expr(body);
                self.check_assignable(&body_type, &out, span);
                self.env.pop_scope();
                self.current_effects = prev_effects;
            }
        }
    }

    fn check_let_binding(&mut self, lb: &ast::LetBinding) -> Type {
        let val_type = self.infer_expr(&lb.value);
        if let Some(ann) = &lb.type_annotation {
            let ann_type = self.resolve_type_expr(ann, &[]);
            self.check_assignable(&val_type, &ann_type, &lb.span);
            self.bind_pattern(&lb.pattern, &ann_type);
            ann_type
        } else {
            self.bind_pattern(&lb.pattern, &val_type);
            val_type
        }
    }

    fn check_verify_stmt(&mut self, stmt: &ast::VerifyStmt, sig: &FnSig) {
        match stmt {
            ast::VerifyStmt::Given(gc) => {
                self.env.push_scope();
                let input_ty = self.infer_expr(&gc.input);
                self.check_assignable(&input_ty, &sig.input, &gc.span);
                let expected_ty = self.infer_expr(&gc.expected);
                self.check_assignable(&expected_ty, &sig.output, &gc.span);
                self.env.pop_scope();
            }
            ast::VerifyStmt::Mock(_) => {
                // Mock declarations set up test context; type-checking mock shapes
                // is deferred to the verify executor phase
            }
            ast::VerifyStmt::ForAll(fa) => {
                self.env.push_scope();
                for binding in &fa.bindings {
                    let ty = self.resolve_type_expr(&binding.type_expr, &[]);
                    self.env.bind(&binding.name, ty);
                }
                self.check_logic_expr(&fa.body);
                self.env.pop_scope();
            }
        }
    }

    fn check_logic_expr(&mut self, le: &ast::LogicExpr) {
        match le {
            ast::LogicExpr::Comparison { left, op: _, right } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                // Comparisons require same type or both numeric
                if lt != rt && !(lt.is_numeric() && rt.is_numeric()) {
                    // Allow TypeVar mismatches silently (generics not instantiated)
                    if !matches!((&lt, &rt), (Type::TypeVar(_), _) | (_, Type::TypeVar(_))) {
                        let span = self.expr_span(left);
                        self.err(format!("comparison between incompatible types: {} vs {}", lt, rt), &span);
                    }
                }
            }
            ast::LogicExpr::DoesNotCrash(e) => { self.infer_expr(e); }
            ast::LogicExpr::Not(inner) => self.check_logic_expr(inner),
            ast::LogicExpr::And(l, r) | ast::LogicExpr::Or(l, r) | ast::LogicExpr::Implies(l, r) => {
                self.check_logic_expr(l);
                self.check_logic_expr(r);
            }
        }
    }

    // =========================================================================
    // Pattern binding — introduce variables into scope
    // =========================================================================

    fn bind_pattern(&mut self, pat: &ast::Pattern, ty: &Type) {
        match pat {
            ast::Pattern::Wildcard(_) => {} // discards the value
            ast::Pattern::Binding(name, _) => {
                self.env.bind(name, ty.clone());
            }
            ast::Pattern::Literal(_) => {} // no bindings introduced
            ast::Pattern::UnitVariant(_, _) => {} // no bindings
            ast::Pattern::TupleVariant { name, inner, span } => {
                // Look up what type the variant's payload is
                if let Some((_type_name, vdef)) = self.env.lookup_variant(name) {
                    match &vdef.payload {
                        VariantPayload::Tuple(inner_ty) => {
                            let resolved = self.substitute_type_vars(inner_ty, ty);
                            self.bind_pattern(inner, &resolved);
                        }
                        _ => self.err(format!("variant {} does not have a tuple payload", name), span),
                    }
                } else {
                    // Unknown variant — could be a type we haven't resolved yet; be lenient
                    self.bind_pattern(inner, &Type::TypeVar("_unknown".to_string()));
                }
            }
            ast::Pattern::RecordVariant { name, fields, span } => {
                if let Some((_type_name, vdef)) = self.env.lookup_variant(name) {
                    match &vdef.payload {
                        VariantPayload::Record(field_types) => {
                            self.bind_field_patterns(fields, field_types, span);
                        }
                        _ => self.err(format!("variant {} does not have record fields", name), span),
                    }
                }
            }
            ast::Pattern::Record { fields, span } => {
                if let Type::Record(field_types) = ty {
                    self.bind_field_patterns(fields, field_types, span);
                }
                // If not a record type, the mismatch will be caught by type checking
            }
            ast::Pattern::List(pats, _) => {
                let elem_ty = match ty {
                    Type::List(inner) => inner.as_ref().clone(),
                    _ => Type::TypeVar("_unknown".to_string()),
                };
                for p in pats {
                    self.bind_pattern(p, &elem_ty);
                }
            }
            ast::Pattern::Typed { name, type_expr, span } => {
                let ann_ty = self.resolve_type_expr(type_expr, &[]);
                self.check_assignable(ty, &ann_ty, span);
                self.env.bind(name, ann_ty);
            }
        }
    }

    fn bind_field_patterns(&mut self, fields: &[ast::FieldPattern], field_types: &[(String, Type)], _span: &Span) {
        for fp in fields {
            match fp {
                ast::FieldPattern::Named(name, pat) => {
                    let fty = field_types.iter().find(|(n, _)| n == name)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(Type::TypeVar("_unknown".to_string()));
                    self.bind_pattern(pat, &fty);
                }
                ast::FieldPattern::Shorthand(name) => {
                    let fty = field_types.iter().find(|(n, _)| n == name)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(Type::TypeVar("_unknown".to_string()));
                    self.env.bind(name, fty);
                }
                ast::FieldPattern::Wildcard => {} // ignore
            }
        }
    }

    // =========================================================================
    // Expression type inference
    // =========================================================================

    pub fn infer_expr(&mut self, expr: &ast::Expr) -> Type {
        match expr {
            ast::Expr::IntLiteral(_, _) => Type::Int,
            ast::Expr::FloatLiteral(_, _) => Type::Float,
            ast::Expr::StringLiteral(_, _) => Type::String,
            ast::Expr::BoolLiteral(_, _) => Type::Bool,
            ast::Expr::UnitLiteral(_) => Type::Unit,

            ast::Expr::Var(name, span) => {
                if let Some(ty) = self.env.lookup(name) {
                    ty.clone()
                } else if let Some(sig) = self.env.lookup_fn(name).cloned() {
                    // A function name used as a value => FunctionRef
                    Type::FunctionRef {
                        effects: sig.effects,
                        input: Box::new(sig.input),
                        output: Box::new(sig.output),
                    }
                } else {
                    self.err(format!("undefined variable '{}'", name), span);
                    Type::TypeVar("_error".to_string())
                }
            }

            ast::Expr::UpperVar(name, _) => {
                // Could be a unit variant constructor or a type reference
                if let Some((type_name, vdef)) = self.env.lookup_variant(name) {
                    match &vdef.payload {
                        VariantPayload::Unit => Type::Named(type_name),
                        _ => Type::Named(type_name), // constructor will be called
                    }
                } else {
                    Type::Named(name.clone())
                }
            }

            ast::Expr::QualifiedName(parts, span) => {
                self.infer_qualified_name(parts, span)
            }

            ast::Expr::Wildcard(_) => Type::TypeVar("_wildcard".to_string()),

            ast::Expr::Call { function, args, span } => {
                self.infer_call(function, args, span)
            }

            ast::Expr::Pipeline { left, steps, span } => {
                let mut ty = self.infer_expr(left);
                for step in steps {
                    let step_ty = self.infer_expr(step);
                    match &step_ty {
                        Type::FunctionRef { effects, output, .. } => {
                            // Check effect compatibility
                            if !effects.is_subset_of(&self.current_effects) {
                                self.err(
                                    format!("pipeline step requires effects {} but context only has {}", effects, self.current_effects),
                                    span,
                                );
                            }
                            ty = *output.clone();
                        }
                        _ => {
                            // Could be a named function — look it up
                            if let ast::Expr::Var(name, _) = step {
                                if let Some(sig) = self.env.lookup_fn(name).cloned() {
                                    if !sig.effects.is_subset_of(&self.current_effects) {
                                        self.err(
                                            format!("pipeline step '{}' requires effects {} but context only has {}", name, sig.effects, self.current_effects),
                                            span,
                                        );
                                    }
                                    ty = sig.output;
                                    continue;
                                }
                            }
                            ty = Type::TypeVar("_unknown".to_string());
                        }
                    }
                }
                ty
            }

            ast::Expr::Match { scrutinee, arms, span } => {
                let scrut_ty = self.infer_expr(scrutinee);
                let mut arm_types: Vec<Type> = Vec::new();

                for arm in arms {
                    self.env.push_scope();
                    self.bind_pattern(&arm.pattern, &scrut_ty);
                    let arm_ty = self.infer_expr(&arm.body);
                    arm_types.push(arm_ty);
                    self.env.pop_scope();
                }

                // All arms must produce the same type (or Never)
                if let Some(_first) = arm_types.first() {
                    let result_ty = arm_types.iter().find(|t| !t.is_never())
                        .cloned()
                        .unwrap_or(Type::Never);

                    for (i, arm_ty) in arm_types.iter().enumerate() {
                        if !arm_ty.is_never() && *arm_ty != result_ty {
                            if !self.types_compatible(arm_ty, &result_ty) {
                                self.err(
                                    format!("match arm {} has type {} but expected {}", i, arm_ty, result_ty),
                                    span,
                                );
                            }
                        }
                    }
                    result_ty
                } else {
                    self.err("match expression has no arms", span);
                    Type::TypeVar("_error".to_string())
                }
            }

            ast::Expr::Record { name, fields, span } => {
                let mut field_types = Vec::new();
                for fv in fields {
                    let ft = self.infer_expr(&fv.value);
                    field_types.push((fv.name.clone(), ft));
                }

                if let Some(name_expr) = name {
                    // Named record construction: Name { ... }
                    if let ast::Expr::UpperVar(type_name, _) = name_expr.as_ref() {
                        // Check against the type definition
                        self.check_record_fields(type_name, &field_types, span);
                        Type::Named(type_name.clone())
                    } else {
                        Type::Record(field_types)
                    }
                } else {
                    Type::Record(field_types)
                }
            }

            ast::Expr::List(items, _) => {
                if items.is_empty() {
                    Type::List(Box::new(Type::TypeVar("_empty".to_string())))
                } else {
                    let first_ty = self.infer_expr(&items[0]);
                    for item in items.iter().skip(1) {
                        let item_ty = self.infer_expr(item);
                        if item_ty != first_ty && !self.types_compatible(&item_ty, &first_ty) {
                            let span = self.expr_span(item);
                            self.err(
                                format!("list element has type {} but expected {}", item_ty, first_ty),
                                &span,
                            );
                        }
                    }
                    Type::List(Box::new(first_ty))
                }
            }

            ast::Expr::DoBlock { stmts, final_expr, span: _ } => {
                self.env.push_scope();
                for stmt in stmts {
                    match stmt {
                        ast::DoStmt::Expr(e) => {
                            let ty = self.infer_expr(e);
                            // Non-final statements in do block must be Unit (or Never)
                            if ty != Type::Unit && !ty.is_never() {
                                let s = self.expr_span(e);
                                self.err(
                                    format!("non-final do block statement has type {} but expected Unit", ty),
                                    &s,
                                );
                            }
                        }
                        ast::DoStmt::Let(lb) => { self.check_let_binding(lb); }
                        ast::DoStmt::ChannelSend { channel, value, .. } => {
                            self.infer_expr(channel);
                            self.infer_expr(value);
                        }
                    }
                }
                let result = self.infer_expr(final_expr);
                self.env.pop_scope();
                result
            }

            ast::Expr::Select { arms, timeout, span } => {
                // select requires IO
                if !self.current_effects.effects.contains("IO") {
                    self.err("select requires IO effect", span);
                }
                let mut arm_types: Vec<Type> = Vec::new();
                for arm in arms {
                    self.env.push_scope();
                    let chan_ty = self.infer_expr(&arm.channel);
                    let elem_ty = match &chan_ty {
                        Type::Channel(inner) => inner.as_ref().clone(),
                        _ => Type::TypeVar("_unknown".to_string()),
                    };
                    if arm.binding != "_" {
                        self.env.bind(&arm.binding, elem_ty);
                    }
                    let body_ty = self.infer_expr(&arm.body);
                    arm_types.push(body_ty);
                    self.env.pop_scope();
                }
                if let Some(ta) = timeout {
                    self.infer_expr(&ta.duration);
                    let body_ty = self.infer_expr(&ta.body);
                    arm_types.push(body_ty);
                }
                // All arms should have compatible types; return first non-Never
                arm_types.iter().find(|t| !t.is_never()).cloned()
                    .unwrap_or(Type::Unit)
            }

            ast::Expr::ChannelSend { channel, value, span } => {
                if !self.current_effects.effects.contains("IO") {
                    self.err("channel send requires IO effect", span);
                }
                self.infer_expr(channel);
                self.infer_expr(value);
                Type::Unit
            }

            ast::Expr::ChannelRecv(channel, span) => {
                if !self.current_effects.effects.contains("IO") {
                    self.err("channel receive requires IO effect", span);
                }
                let chan_ty = self.infer_expr(channel);
                match &chan_ty {
                    Type::Channel(inner) => inner.as_ref().clone(),
                    _ => {
                        self.err(format!("expected Channel type, got {}", chan_ty), span);
                        Type::TypeVar("_error".to_string())
                    }
                }
            }

            ast::Expr::ChannelNew { element_type, span } => {
                if !self.current_effects.effects.contains("IO") {
                    self.err("Channel.new requires IO effect", span);
                }
                let elem_ty = self.resolve_type_expr(element_type, &[]);
                Type::Channel(Box::new(elem_ty))
            }

            ast::Expr::Clone(inner, _) => {
                self.infer_expr(inner)
                // Clone returns the same type
            }

            ast::Expr::With { function, binding, span } => {
                let fn_ty = self.infer_expr(function);
                match &fn_ty {
                    Type::FunctionRef { effects: _, input: _, output: _ } => {
                        // .with produces a new FunctionRef with some params bound
                        // For now, return a FunctionRef with same effects/output
                        match binding {
                            ast::WithBinding::Named(_, val) => { self.infer_expr(val); }
                            ast::WithBinding::Record(fields) => {
                                for fv in fields { self.infer_expr(&fv.value); }
                            }
                        }
                        fn_ty.clone()
                    }
                    _ => {
                        self.err(format!("cannot use .with on non-FunctionRef type {}", fn_ty), span);
                        fn_ty
                    }
                }
            }

            ast::Expr::Let(lb) => {
                self.check_let_binding(lb);
                Type::Unit // let bindings in expression context are Unit
            }

            ast::Expr::BinaryOp { left, op, right, span } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                self.check_binary_op(&lt, op, &rt, span)
            }

            ast::Expr::FieldAccess { object, field, span } => {
                let obj_ty = self.infer_expr(object);
                self.infer_field_access(&obj_ty, field, span)
            }

            ast::Expr::Paren(inner, _) => self.infer_expr(inner),
        }
    }

    // =========================================================================
    // Call inference
    // =========================================================================

    fn infer_call(&mut self, function: &ast::Expr, args: &[ast::Arg], span: &Span) -> Type {
        // Infer arg types
        for arg in args {
            match arg {
                ast::Arg::Positional(e) => { self.infer_expr(e); }
                ast::Arg::Named(_, e) => { self.infer_expr(e); }
            }
        }

        // If the function is a qualified name like Result.ok(...), handle it
        if let ast::Expr::QualifiedName(parts, qspan) = function {
            let sig_ty = self.infer_qualified_name(parts, qspan);
            return match sig_ty {
                Type::FunctionRef { effects, output, .. } => {
                    if !effects.is_subset_of(&self.current_effects) {
                        self.err(
                            format!("call requires effects {} but context only has {}", effects, self.current_effects),
                            span,
                        );
                    }
                    *output
                }
                _ => sig_ty, // best-effort
            };
        }

        let fn_ty = self.infer_expr(function);

        match &fn_ty {
            Type::FunctionRef { effects, output, .. } => {
                if !effects.is_subset_of(&self.current_effects) {
                    self.err(
                        format!("call requires effects {} but context only has {}", effects, self.current_effects),
                        span,
                    );
                }
                *output.clone()
            }
            Type::Named(name) => {
                // Could be a variant constructor call: Ok(value), Some(value)
                if let Some((type_name, vdef)) = self.env.lookup_variant(name) {
                    match &vdef.payload {
                        VariantPayload::Tuple(_) => Type::Named(type_name),
                        VariantPayload::Unit => Type::Named(type_name),
                        VariantPayload::Record(_) => Type::Named(type_name),
                    }
                } else if let Some(sig) = self.env.lookup_fn(name).cloned() {
                    if !sig.effects.is_subset_of(&self.current_effects) {
                        self.err(
                            format!("call to '{}' requires effects {} but context only has {}", name, sig.effects, self.current_effects),
                            span,
                        );
                    }
                    sig.output
                } else {
                    // Unknown — could be a generic constructor
                    Type::TypeVar("_unknown".to_string())
                }
            }
            _ => {
                // Try looking up as a named function
                if let ast::Expr::Var(name, _) = function {
                    if let Some(sig) = self.env.lookup_fn(name).cloned() {
                        if !sig.effects.is_subset_of(&self.current_effects) {
                            self.err(
                                format!("call to '{}' requires effects {} but context only has {}", name, sig.effects, self.current_effects),
                                span,
                            );
                        }
                        return sig.output;
                    }
                }
                Type::TypeVar("_unknown".to_string())
            }
        }
    }

    // =========================================================================
    // Qualified name inference
    // =========================================================================

    fn infer_qualified_name(&mut self, parts: &[String], _span: &Span) -> Type {
        if parts.len() == 2 {
            let module = &parts[0];
            let method = &parts[1];

            // Check module methods
            if let Some(sig) = self.env.lookup_module_method(module, method).cloned() {
                return Type::FunctionRef {
                    effects: sig.effects,
                    input: Box::new(sig.input),
                    output: Box::new(sig.output),
                };
            }

            // Check if it's a variant constructor: Type.Variant
            if let Some(td) = self.env.lookup_type(module).cloned() {
                if let TypeDef::Sum { variants, .. } = &td {
                    if variants.iter().any(|v| v.name == *method) {
                        return Type::Named(module.clone());
                    }
                }
            }
        }

        // General qualified name — return TypeVar for now
        Type::TypeVar("_qualified".to_string())
    }

    // =========================================================================
    // Binary operations
    // =========================================================================

    fn check_binary_op(&mut self, lt: &Type, op: &ast::BinaryOp, rt: &Type, span: &Span) -> Type {
        match op {
            ast::BinaryOp::Add | ast::BinaryOp::Sub | ast::BinaryOp::Mul | ast::BinaryOp::Div | ast::BinaryOp::Mod => {
                // Arithmetic: both sides must be numeric
                if !lt.is_numeric() && !matches!(lt, Type::TypeVar(_)) {
                    self.err(format!("left operand of arithmetic op has type {} but expected Int or Float", lt), span);
                }
                if !rt.is_numeric() && !matches!(rt, Type::TypeVar(_)) {
                    self.err(format!("right operand of arithmetic op has type {} but expected Int or Float", rt), span);
                }
                // Result type: if both Int => Int, if either Float => Float
                if lt == &Type::Float || rt == &Type::Float {
                    Type::Float
                } else {
                    Type::Int
                }
            }
            ast::BinaryOp::Eq | ast::BinaryOp::Ne => {
                // Equality: both sides must be same type (structural equality)
                if !self.types_compatible(lt, rt) {
                    self.err(format!("equality comparison between incompatible types: {} vs {}", lt, rt), span);
                }
                Type::Bool
            }
            ast::BinaryOp::Lt | ast::BinaryOp::Le | ast::BinaryOp::Gt | ast::BinaryOp::Ge => {
                // Ordering: both sides must be same numeric type
                if !lt.is_numeric() && !matches!(lt, Type::TypeVar(_)) {
                    self.err(format!("left operand of comparison has type {} but expected numeric", lt), span);
                }
                if !rt.is_numeric() && !matches!(rt, Type::TypeVar(_)) {
                    self.err(format!("right operand of comparison has type {} but expected numeric", rt), span);
                }
                Type::Bool
            }
        }
    }

    // =========================================================================
    // Field access
    // =========================================================================

    fn infer_field_access(&mut self, obj_ty: &Type, field: &str, span: &Span) -> Type {
        match obj_ty {
            Type::Record(fields) => {
                if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                    ty.clone()
                } else {
                    self.err(format!("record has no field '{}'", field), span);
                    Type::TypeVar("_error".to_string())
                }
            }
            Type::Named(type_name) => {
                // Look up the type definition
                if let Some(td) = self.env.lookup_type(type_name).cloned() {
                    match &td {
                        TypeDef::Product { fields, .. } => {
                            if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                                return ty.clone();
                            }
                            self.err(format!("type {} has no field '{}'", type_name, field), span);
                            Type::TypeVar("_error".to_string())
                        }
                        TypeDef::Sum { .. } => {
                            // Cannot dot-access on sum type directly (spec §3.2)
                            self.err(
                                format!("cannot access field '{}' on sum type '{}' directly — use match", field, type_name),
                                span,
                            );
                            Type::TypeVar("_error".to_string())
                        }
                        _ => {
                            Type::TypeVar("_unknown".to_string())
                        }
                    }
                } else {
                    // Unknown type — be lenient
                    Type::TypeVar("_unknown".to_string())
                }
            }
            _ => {
                // TypeVar or unknown — be lenient
                Type::TypeVar("_unknown".to_string())
            }
        }
    }

    // =========================================================================
    // Record field checking
    // =========================================================================

    fn check_record_fields(&mut self, type_name: &str, fields: &[(String, Type)], span: &Span) {
        if let Some(td) = self.env.lookup_type(type_name).cloned() {
            match &td {
                TypeDef::Product { fields: expected, .. } => {
                    // Check all expected fields are present
                    for (ename, _ety) in expected {
                        if !fields.iter().any(|(n, _)| n == ename) {
                            self.err(format!("missing field '{}' in record of type {}", ename, type_name), span);
                        }
                    }
                    // Check no extra fields
                    for (fname, _) in fields {
                        if !expected.iter().any(|(n, _)| n == fname) {
                            self.err(format!("unexpected field '{}' in record of type {}", fname, type_name), span);
                        }
                    }
                }
                TypeDef::Sum { variants: _, .. } => {
                    // Could be a variant record construction
                    // The variant name is already resolved by the caller
                }
                _ => {}
            }
        }
    }

    // =========================================================================
    // Type compatibility / assignability
    // =========================================================================

    /// Check if `actual` is assignable to `expected`. Reports error if not.
    fn check_assignable(&mut self, actual: &Type, expected: &Type, span: &Span) {
        if !self.types_compatible(actual, expected) {
            self.err(
                format!("type mismatch: expected {}, got {}", expected, actual),
                span,
            );
        }
    }

    /// Check if two types are compatible (allowing TypeVars, Never subtyping, etc.)
    fn types_compatible(&self, a: &Type, b: &Type) -> bool {
        // TypeVars are compatible with anything (generics not fully instantiated)
        if matches!(a, Type::TypeVar(_)) || matches!(b, Type::TypeVar(_)) {
            return true;
        }
        // Never is a subtype of everything
        if a.is_never() || b.is_never() {
            return true;
        }
        // Exact equality
        if a == b {
            return true;
        }
        // Named type could be an alias
        if let (Type::Named(a_name), _) = (a, b) {
            if let Some(td) = self.env.lookup_type(a_name) {
                if let TypeDef::Alias { target, .. } = td {
                    return self.types_compatible(target, b);
                }
                if let TypeDef::Refinement { base, .. } = td {
                    return self.types_compatible(base, b);
                }
            }
        }
        if let (_, Type::Named(b_name)) = (a, b) {
            if let Some(td) = self.env.lookup_type(b_name) {
                if let TypeDef::Alias { target, .. } = td {
                    return self.types_compatible(a, target);
                }
                if let TypeDef::Refinement { base, .. } = td {
                    return self.types_compatible(a, base);
                }
            }
        }
        // Generic types — check name and args
        if let (Type::Generic { name: a_name, args: a_args }, Type::Generic { name: b_name, args: b_args }) = (a, b) {
            if a_name == b_name && a_args.len() == b_args.len() {
                return a_args.iter().zip(b_args.iter()).all(|(a, b)| self.types_compatible(a, b));
            }
        }
        // Record types — structural compatibility
        if let (Type::Record(a_fields), Type::Record(b_fields)) = (a, b) {
            if a_fields.len() == b_fields.len() {
                return a_fields.iter().all(|(name, ty)| {
                    b_fields.iter().any(|(bn, bt)| bn == name && self.types_compatible(ty, bt))
                });
            }
        }
        // List types
        if let (Type::List(a_elem), Type::List(b_elem)) = (a, b) {
            return self.types_compatible(a_elem, b_elem);
        }
        // Channel types
        if let (Type::Channel(a_elem), Type::Channel(b_elem)) = (a, b) {
            return self.types_compatible(a_elem, b_elem);
        }
        false
    }

    // =========================================================================
    // Never return type validation (spec §3.1)
    // =========================================================================

    /// Check that a Never-returning function's out expression is a valid tail call
    /// (direct self-recursion, or a do block whose final expr is a tail call).
    fn is_valid_never_expr(&self, expr: &ast::Expr, fn_name: &str) -> bool {
        match expr {
            ast::Expr::Call { function, .. } => {
                self.is_self_call(function, fn_name)
            }
            ast::Expr::DoBlock { final_expr, .. } => {
                self.is_valid_never_expr(final_expr, fn_name)
            }
            ast::Expr::Paren(inner, _) => {
                self.is_valid_never_expr(inner, fn_name)
            }
            _ => false,
        }
    }

    fn is_self_call(&self, function: &ast::Expr, fn_name: &str) -> bool {
        match function {
            ast::Expr::Var(name, _) => name == fn_name,
            ast::Expr::Paren(inner, _) => self.is_self_call(inner, fn_name),
            _ => false,
        }
    }

    // =========================================================================
    // Type variable substitution (for generic instantiation)
    // =========================================================================

    fn substitute_type_vars(&self, ty: &Type, context: &Type) -> Type {
        // Simple: if ty is a TypeVar, return context. For real generic instantiation
        // we'd build a substitution map. This is a simplified version.
        match ty {
            Type::TypeVar(_) => context.clone(),
            _ => ty.clone(),
        }
    }

    // =========================================================================
    // Utility
    // =========================================================================

    fn expr_span(&self, expr: &ast::Expr) -> Span {
        match expr {
            ast::Expr::IntLiteral(_, s) | ast::Expr::FloatLiteral(_, s) |
            ast::Expr::StringLiteral(_, s) | ast::Expr::BoolLiteral(_, s) |
            ast::Expr::UnitLiteral(s) | ast::Expr::Var(_, s) |
            ast::Expr::UpperVar(_, s) | ast::Expr::QualifiedName(_, s) |
            ast::Expr::Wildcard(s) | ast::Expr::Paren(_, s) |
            ast::Expr::Clone(_, s) | ast::Expr::ChannelRecv(_, s) => s.clone(),
            ast::Expr::Call { span, .. } | ast::Expr::Pipeline { span, .. } |
            ast::Expr::Match { span, .. } | ast::Expr::Record { span, .. } |
            ast::Expr::DoBlock { span, .. } | ast::Expr::Select { span, .. } |
            ast::Expr::ChannelSend { span, .. } | ast::Expr::ChannelNew { span, .. } |
            ast::Expr::With { span, .. } | ast::Expr::BinaryOp { span, .. } |
            ast::Expr::FieldAccess { span, .. } => span.clone(),
            ast::Expr::List(_, s) => s.clone(),
            ast::Expr::Let(lb) => lb.span.clone(),
        }
    }
}
