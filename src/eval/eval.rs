use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::ast;
use super::{RuntimeError, Thunk, Value, VariantPayload};
use super::env::Env;
use super::stdlib;

/// Maximum trampoline iterations before giving up (guards against infinite loops
/// in the synchronous evaluator where channels/tasks are not real).
const MAX_ITER: usize = 100_000;

pub struct Evaluator {
    pub(crate) env: Env,
    /// All user-defined function declarations (by name).
    pub(crate) fns: HashMap<String, ast::FnDecl>,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator { env: Env::new(), fns: HashMap::new() }
    }

    // =========================================================================
    // Program loading
    // =========================================================================

    pub fn load_program(&mut self, program: &crate::ast::Program) {
        for decl in &program.declarations {
            match decl {
                ast::TopLevelDecl::FnDecl(fd) => self.register_fn(fd),
                ast::TopLevelDecl::LetBinding(lb) => {
                    if let Ok(v) = self.eval_expr(&lb.value) {
                        self.bind_pattern_to_env(&lb.pattern, v);
                    }
                }
                _ => {}
            }
        }
    }

    fn register_fn(&mut self, fd: &ast::FnDecl) {
        self.fns.insert(fd.name.clone(), fd.clone());
        if let Some(helpers) = &fd.helpers {
            for h in helpers {
                match h {
                    ast::HelperDecl::Compact { name, body, span, .. } => {
                        let helper_decl = ast::FnDecl {
                            name: name.clone(),
                            signature: fd.signature.clone(),
                            in_clause: ast::Pattern::Binding("_in".to_string(), span.clone()),
                            out_clause: body.clone(),
                            confidence: None,
                            reason: None,
                            proves: None,
                            provenance: None,
                            verify: None,
                            helpers: None,
                            span: span.clone(),
                        };
                        self.fns.insert(name.clone(), helper_decl);
                    }
                    ast::HelperDecl::Full(inner) => self.register_fn(inner),
                }
            }
        }
    }

    // =========================================================================
    // Public call interface — TCO trampoline
    // =========================================================================

    pub fn call_fn(&mut self, fn_name: &str, arg: Value) -> Result<Value, RuntimeError> {
        let mut cur_name = fn_name.to_string();
        let mut cur_arg = arg;
        for _ in 0..MAX_ITER {
            match self.eval_fn_once(&cur_name, cur_arg)? {
                Thunk::Value(v) => return Ok(v),
                Thunk::TailCall { fn_name, arg } => {
                    cur_name = fn_name;
                    cur_arg = arg;
                }
            }
        }
        Err(RuntimeError::new(format!(
            "exceeded {} iterations in '{}'", MAX_ITER, cur_name
        )))
    }

    fn eval_fn_once(&mut self, fn_name: &str, arg: Value) -> Result<Thunk, RuntimeError> {
        if stdlib::is_stdlib(fn_name) {
            let v = stdlib::dispatch(fn_name, vec![arg], self)?;
            return Ok(Thunk::Value(v));
        }
        let decl = self.fns.get(fn_name).cloned()
            .ok_or_else(|| RuntimeError::new(format!("undefined function '{}'", fn_name)))?;
        self.env.push_scope();
        self.bind_pattern(&decl.in_clause, &arg)?;
        let result = self.eval_tail(&decl.out_clause)?;
        self.env.pop_scope();
        Ok(result)
    }

    // =========================================================================
    // Tail-position evaluator (returns Thunk for TCO)
    // =========================================================================

    fn eval_tail(&mut self, expr: &ast::Expr) -> Result<Thunk, RuntimeError> {
        match expr {
            ast::Expr::Call { function, args, span } => {
                let arg_vals = self.eval_args(args)?;
                match function.as_ref() {
                    ast::Expr::Var(name, _) => {
                        if self.fns.contains_key(name.as_str()) {
                            let arg = pack_args(arg_vals);
                            return Ok(Thunk::TailCall { fn_name: name.clone(), arg });
                        }
                        let arg = pack_args(arg_vals);
                        Ok(Thunk::Value(self.dispatch_by_name(name, arg, span)?))
                    }
                    ast::Expr::QualifiedName(parts, _) => {
                        let name = parts.join(".");
                        Ok(Thunk::Value(stdlib::dispatch(&name, arg_vals, self)?))
                    }
                    ast::Expr::UpperVar(name, _) => {
                        let arg = pack_args(arg_vals);
                        Ok(Thunk::Value(Value::Variant {
                            name: name.clone(),
                            payload: VariantPayload::Tuple(Box::new(arg)),
                        }))
                    }
                    _ => {
                        let fn_val = self.eval_expr(function)?;
                        let arg = pack_args(arg_vals);
                        Ok(Thunk::Value(self.call_value(fn_val, arg, span)?))
                    }
                }
            }
            ast::Expr::DoBlock { stmts, final_expr, .. } => {
                self.env.push_scope();
                for stmt in stmts {
                    self.eval_do_stmt(stmt)?;
                }
                let result = self.eval_tail(final_expr)?;
                self.env.pop_scope();
                Ok(result)
            }
            ast::Expr::Paren(inner, _) => self.eval_tail(inner),
            ast::Expr::Match { scrutinee, arms, span } => {
                let scrut = self.eval_expr(scrutinee)?;
                for arm in arms {
                    if self.pattern_matches(&arm.pattern, &scrut) {
                        self.env.push_scope();
                        self.bind_pattern(&arm.pattern, &scrut)?;
                        let result = self.eval_tail(&arm.body)?;
                        self.env.pop_scope();
                        return Ok(result);
                    }
                }
                Err(RuntimeError::at(format!("non-exhaustive match on: {}", scrut), span))
            }
            _ => Ok(Thunk::Value(self.eval_expr(expr)?)),
        }
    }

    // =========================================================================
    // Expression evaluator
    // =========================================================================

    pub fn eval_expr(&mut self, expr: &ast::Expr) -> Result<Value, RuntimeError> {
        match expr {
            ast::Expr::IntLiteral(n, _) => Ok(Value::Int(*n)),
            ast::Expr::FloatLiteral(f, _) => Ok(Value::Float(*f)),
            ast::Expr::StringLiteral(s, _) => Ok(Value::Str(s.clone())),
            ast::Expr::BoolLiteral(b, _) => Ok(Value::Bool(*b)),
            ast::Expr::UnitLiteral(_) => Ok(Value::Unit),
            ast::Expr::Wildcard(_) => Ok(Value::Unit),

            ast::Expr::Var(name, span) => {
                if let Some(v) = self.env.lookup(name) {
                    return Ok(v.clone());
                }
                if self.fns.contains_key(name.as_str()) {
                    return Ok(Value::FnRef(name.clone()));
                }
                Err(RuntimeError::at(format!("undefined variable '{}'", name), span))
            }

            ast::Expr::UpperVar(name, _) => {
                Ok(Value::Variant { name: name.clone(), payload: VariantPayload::Unit })
            }

            ast::Expr::QualifiedName(parts, _) => {
                Ok(Value::FnRef(parts.join(".")))
            }

            ast::Expr::Call { function, args, span } => {
                let arg_vals = self.eval_args(args)?;
                match function.as_ref() {
                    ast::Expr::Var(name, _) => {
                        let arg = pack_args(arg_vals);
                        self.dispatch_by_name(name, arg, span)
                    }
                    ast::Expr::QualifiedName(parts, _) => {
                        let name = parts.join(".");
                        stdlib::dispatch(&name, arg_vals, self)
                    }
                    ast::Expr::UpperVar(name, _) => {
                        let arg = pack_args(arg_vals);
                        Ok(Value::Variant {
                            name: name.clone(),
                            payload: VariantPayload::Tuple(Box::new(arg)),
                        })
                    }
                    _ => {
                        let fn_val = self.eval_expr(function)?;
                        let arg = pack_args(arg_vals);
                        self.call_value(fn_val, arg, span)
                    }
                }
            }

            ast::Expr::Pipeline { left, steps, span } => {
                let mut val = self.eval_expr(left)?;
                for step in steps {
                    val = self.apply_pipeline_step(val, step, span)?;
                }
                Ok(val)
            }

            ast::Expr::Match { scrutinee, arms, span } => {
                let scrut = self.eval_expr(scrutinee)?;
                self.eval_match(scrut, arms, span)
            }

            ast::Expr::Record { name, fields, .. } => {
                let mut fvs: Vec<(String, Value)> = Vec::new();
                for fv in fields {
                    let v = self.eval_expr(&fv.value)?;
                    fvs.push((fv.name.clone(), v));
                }
                match name {
                    Some(name_expr) => {
                        if let ast::Expr::UpperVar(type_name, _) = name_expr.as_ref() {
                            Ok(Value::Variant {
                                name: type_name.clone(),
                                payload: VariantPayload::Record(fvs),
                            })
                        } else {
                            Ok(Value::Record(fvs))
                        }
                    }
                    None => Ok(Value::Record(fvs)),
                }
            }

            ast::Expr::List(items, _) => {
                let vals: Result<Vec<_>, _> = items.iter().map(|e| self.eval_expr(e)).collect();
                Ok(Value::List(vals?))
            }

            ast::Expr::DoBlock { stmts, final_expr, .. } => {
                self.env.push_scope();
                for stmt in stmts {
                    self.eval_do_stmt(stmt)?;
                }
                let result = self.eval_expr(final_expr)?;
                self.env.pop_scope();
                Ok(result)
            }

            ast::Expr::Select { arms, timeout, span } => {
                for arm in arms {
                    let chan_val = self.eval_expr(&arm.channel)?;
                    if let Value::Channel(ch) = chan_val {
                        if let Some(item) = ch.borrow_mut().pop_front() {
                            self.env.push_scope();
                            if arm.binding != "_" {
                                self.env.bind(&arm.binding, item);
                            }
                            let result = self.eval_expr(&arm.body)?;
                            self.env.pop_scope();
                            return Ok(result);
                        }
                    }
                }
                if let Some(ta) = timeout {
                    return self.eval_expr(&ta.body);
                }
                Err(RuntimeError::at("select: no channel ready in synchronous evaluator", span))
            }

            ast::Expr::ChannelSend { channel, value, span } => {
                let chan_val = self.eval_expr(channel)?;
                let val = self.eval_expr(value)?;
                match chan_val {
                    Value::Channel(ch) => {
                        ch.borrow_mut().push_back(val);
                        Ok(Value::Unit)
                    }
                    _ => Err(RuntimeError::at("channel send on non-channel value", span)),
                }
            }

            ast::Expr::ChannelRecv(channel, span) => {
                let chan_val = self.eval_expr(channel)?;
                match chan_val {
                    Value::Channel(ch) => ch
                        .borrow_mut()
                        .pop_front()
                        .ok_or_else(|| RuntimeError::at("channel recv: empty channel", span)),
                    _ => Err(RuntimeError::at("channel recv on non-channel value", span)),
                }
            }

            ast::Expr::ChannelNew { .. } => {
                Ok(Value::Channel(Rc::new(RefCell::new(VecDeque::new()))))
            }

            ast::Expr::Clone(inner, _) => self.eval_expr(inner),

            ast::Expr::With { function, binding, span } => {
                let fn_val = self.eval_expr(function)?;
                let (fn_name, mut bound) = match fn_val {
                    Value::FnRef(n) => (n, Vec::new()),
                    Value::PartialFn { name, bound } => (name, bound),
                    other => {
                        return Err(RuntimeError::at(
                            format!("cannot apply .with to non-function: {}", other),
                            span,
                        ))
                    }
                };
                match binding {
                    ast::WithBinding::Named(field, val_expr) => {
                        let v = self.eval_expr(val_expr)?;
                        bound.push((field.clone(), v));
                    }
                    ast::WithBinding::Record(fields) => {
                        for fv in fields {
                            let v = self.eval_expr(&fv.value)?;
                            bound.push((fv.name.clone(), v));
                        }
                    }
                }
                Ok(Value::PartialFn { name: fn_name, bound })
            }

            ast::Expr::Let(lb) => {
                let v = self.eval_expr(&lb.value)?;
                self.bind_pattern_to_env(&lb.pattern, v);
                Ok(Value::Unit)
            }

            ast::Expr::BinaryOp { left, op, right, span } => {
                let lv = self.eval_expr(left)?;
                let rv = self.eval_expr(right)?;
                eval_binop(&lv, op, &rv, span)
            }

            ast::Expr::FieldAccess { object, field, span } => {
                let obj = self.eval_expr(object)?;
                eval_field_access(obj, field, span)
            }

            ast::Expr::Paren(inner, _) => self.eval_expr(inner),
        }
    }

    // =========================================================================
    // Call dispatch helpers
    // =========================================================================

    fn dispatch_by_name(
        &mut self,
        name: &str,
        arg: Value,
        span: &ast::Span,
    ) -> Result<Value, RuntimeError> {
        if stdlib::is_stdlib(name) {
            return stdlib::dispatch(name, vec![arg], self);
        }
        if self.fns.contains_key(name) {
            return self.call_fn(name, arg);
        }
        Err(RuntimeError::at(format!("undefined function '{}'", name), span))
    }

    pub(crate) fn call_value(
        &mut self,
        fn_val: Value,
        arg: Value,
        span: &ast::Span,
    ) -> Result<Value, RuntimeError> {
        match fn_val {
            Value::FnRef(name) => {
                if stdlib::is_stdlib(&name) {
                    stdlib::dispatch(&name, vec![arg], self)
                } else {
                    self.call_fn(&name, arg)
                }
            }
            Value::PartialFn { name, mut bound } => {
                // Merge the bound fields with the new arg
                let merged = match arg {
                    Value::Record(new_fields) => {
                        bound.extend(new_fields);
                        Value::Record(bound)
                    }
                    single => {
                        if bound.is_empty() {
                            single
                        } else {
                            bound.push(("_input".to_string(), single));
                            Value::Record(bound)
                        }
                    }
                };
                if stdlib::is_stdlib(&name) {
                    stdlib::dispatch(&name, vec![merged], self)
                } else {
                    self.call_fn(&name, merged)
                }
            }
            other => Err(RuntimeError::at(
                format!("cannot call non-function value: {}", other),
                span,
            )),
        }
    }

    fn eval_args(&mut self, args: &[ast::Arg]) -> Result<Vec<Value>, RuntimeError> {
        args.iter()
            .map(|a| match a {
                ast::Arg::Positional(e) => self.eval_expr(e),
                ast::Arg::Named(_, e) => self.eval_expr(e),
            })
            .collect()
    }

    fn apply_pipeline_step(
        &mut self,
        piped: Value,
        step: &ast::Expr,
        span: &ast::Span,
    ) -> Result<Value, RuntimeError> {
        match step {
            ast::Expr::Var(name, _) => self.dispatch_by_name(name, piped, span),
            ast::Expr::QualifiedName(parts, _) => {
                let name = parts.join(".");
                stdlib::dispatch(&name, vec![piped], self)
            }
            ast::Expr::Call { function, args, span: call_span } => {
                // Pipeline with extra args: `list |> List.map(fn)`
                // piped is prepended as first arg
                let mut arg_vals = self.eval_args(args)?;
                arg_vals.insert(0, piped);
                match function.as_ref() {
                    ast::Expr::QualifiedName(parts, _) => {
                        let name = parts.join(".");
                        stdlib::dispatch(&name, arg_vals, self)
                    }
                    ast::Expr::Var(name, _) => {
                        if stdlib::is_stdlib(name) {
                            stdlib::dispatch(name, arg_vals, self)
                        } else {
                            // User fn with extra args: use piped as sole arg
                            self.call_fn(name, arg_vals.remove(0))
                        }
                    }
                    _ => {
                        let fn_val = self.eval_expr(function)?;
                        self.call_value(fn_val, arg_vals.remove(0), call_span)
                    }
                }
            }
            _ => {
                let fn_val = self.eval_expr(step)?;
                self.call_value(fn_val, piped, span)
            }
        }
    }

    // =========================================================================
    // Do block statement evaluator
    // =========================================================================

    fn eval_do_stmt(&mut self, stmt: &ast::DoStmt) -> Result<(), RuntimeError> {
        match stmt {
            ast::DoStmt::Expr(e) => {
                self.eval_expr(e)?;
                Ok(())
            }
            ast::DoStmt::Let(lb) => {
                let v = self.eval_expr(&lb.value)?;
                self.bind_pattern_to_env(&lb.pattern, v);
                Ok(())
            }
            ast::DoStmt::ChannelSend { channel, value } => {
                let chan_val = self.eval_expr(channel)?;
                let val = self.eval_expr(value)?;
                if let Value::Channel(ch) = chan_val {
                    ch.borrow_mut().push_back(val);
                }
                Ok(())
            }
        }
    }

    // =========================================================================
    // Pattern matching
    // =========================================================================

    fn eval_match(
        &mut self,
        scrutinee: Value,
        arms: &[ast::MatchArm],
        span: &ast::Span,
    ) -> Result<Value, RuntimeError> {
        for arm in arms {
            if self.pattern_matches(&arm.pattern, &scrutinee) {
                self.env.push_scope();
                self.bind_pattern(&arm.pattern, &scrutinee)?;
                let result = self.eval_expr(&arm.body)?;
                self.env.pop_scope();
                return Ok(result);
            }
        }
        Err(RuntimeError::at(format!("non-exhaustive match on: {}", scrutinee), span))
    }

    fn pattern_matches(&self, pat: &ast::Pattern, val: &Value) -> bool {
        match pat {
            ast::Pattern::Wildcard(_) => true,
            ast::Pattern::Binding(_, _) => true,
            ast::Pattern::Typed { .. } => true,
            ast::Pattern::Literal(lit_expr) => match (lit_expr.as_ref(), val) {
                (ast::Expr::IntLiteral(n, _), Value::Int(v)) => n == v,
                (ast::Expr::FloatLiteral(f, _), Value::Float(v)) => f == v,
                (ast::Expr::StringLiteral(s, _), Value::Str(v)) => s == v,
                (ast::Expr::BoolLiteral(b, _), Value::Bool(v)) => b == v,
                (ast::Expr::UnitLiteral(_), Value::Unit) => true,
                _ => false,
            },
            ast::Pattern::UnitVariant(name, _) => {
                matches!(val, Value::Variant { name: n, payload: VariantPayload::Unit } if n == name)
            }
            ast::Pattern::TupleVariant { name, inner, .. } => {
                if let Value::Variant { name: n, payload: VariantPayload::Tuple(inner_val) } = val {
                    n == name && self.pattern_matches(inner, inner_val)
                } else {
                    false
                }
            }
            ast::Pattern::RecordVariant { name, .. } => {
                matches!(val, Value::Variant { name: n, .. } if n == name)
            }
            ast::Pattern::Record { .. } => {
                matches!(
                    val,
                    Value::Record(_) | Value::Variant { payload: VariantPayload::Record(_), .. }
                )
            }
            ast::Pattern::List(pats, _) => {
                if let Value::List(items) = val {
                    pats.len() == items.len()
                        && pats.iter().zip(items.iter()).all(|(p, v)| self.pattern_matches(p, v))
                } else {
                    false
                }
            }
        }
    }

    pub(crate) fn bind_pattern(
        &mut self,
        pat: &ast::Pattern,
        val: &Value,
    ) -> Result<(), RuntimeError> {
        match pat {
            ast::Pattern::Wildcard(_) => Ok(()),
            ast::Pattern::Binding(name, _) => {
                self.env.bind(name, val.clone());
                Ok(())
            }
            ast::Pattern::Typed { name, .. } => {
                self.env.bind(name, val.clone());
                Ok(())
            }
            ast::Pattern::Literal(_) => Ok(()),
            ast::Pattern::UnitVariant(_, _) => Ok(()),
            ast::Pattern::TupleVariant { name, inner, .. } => {
                if let Value::Variant { name: n, payload: VariantPayload::Tuple(inner_val) } = val {
                    if n == name {
                        self.bind_pattern(inner, inner_val)?;
                    }
                }
                Ok(())
            }
            ast::Pattern::RecordVariant { name, fields, .. } => {
                if let Value::Variant { name: n, payload: VariantPayload::Record(fvals) } = val {
                    if n == name {
                        self.bind_field_patterns(fields, fvals)?;
                    }
                }
                Ok(())
            }
            ast::Pattern::Record { fields, .. } => {
                let fvals = match val {
                    Value::Record(fvals) => fvals,
                    Value::Variant { payload: VariantPayload::Record(fvals), .. } => fvals,
                    _ => return Ok(()),
                };
                self.bind_field_patterns(fields, fvals)
            }
            ast::Pattern::List(pats, _) => {
                if let Value::List(items) = val {
                    for (p, v) in pats.iter().zip(items.iter()) {
                        self.bind_pattern(p, v)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn bind_field_patterns(
        &mut self,
        fields: &[ast::FieldPattern],
        fvals: &[(String, Value)],
    ) -> Result<(), RuntimeError> {
        for fp in fields {
            match fp {
                ast::FieldPattern::Named(fname, pat) => {
                    if let Some((_, v)) = fvals.iter().find(|(n, _)| n == fname) {
                        self.bind_pattern(pat, v)?;
                    }
                }
                ast::FieldPattern::Shorthand(fname) => {
                    if let Some((_, v)) = fvals.iter().find(|(n, _)| n == fname) {
                        self.env.bind(fname, v.clone());
                    }
                }
                ast::FieldPattern::Wildcard => {}
            }
        }
        Ok(())
    }

    pub(crate) fn bind_pattern_to_env(&mut self, pat: &ast::Pattern, val: Value) {
        let _ = self.bind_pattern(pat, &val);
    }
}

// =========================================================================
// Free functions (no Evaluator state needed)
// =========================================================================

/// Pack a vec of evaluated arg values into a single Value.
/// 0 args → Unit, 1 arg → that value, 2+ args → positional Record.
pub(crate) fn pack_args(mut vals: Vec<Value>) -> Value {
    match vals.len() {
        0 => Value::Unit,
        1 => vals.remove(0),
        _ => Value::Record(
            vals.into_iter()
                .enumerate()
                .map(|(i, v)| (format!("_{}", i), v))
                .collect(),
        ),
    }
}

fn eval_binop(
    left: &Value,
    op: &ast::BinaryOp,
    right: &Value,
    span: &ast::Span,
) -> Result<Value, RuntimeError> {
    match op {
        ast::BinaryOp::Add => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            _ => Err(RuntimeError::at(format!("type error: {} + {}", left, right), span)),
        },
        ast::BinaryOp::Sub => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            _ => Err(RuntimeError::at(format!("type error: {} - {}", left, right), span)),
        },
        ast::BinaryOp::Mul => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            _ => Err(RuntimeError::at(format!("type error: {} * {}", left, right), span)),
        },
        ast::BinaryOp::Div => match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    Err(RuntimeError::at("division by zero", span))
                } else {
                    Ok(Value::Int(a / b))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            _ => Err(RuntimeError::at(format!("type error: {} / {}", left, right), span)),
        },
        ast::BinaryOp::Eq => Ok(Value::Bool(left == right)),
        ast::BinaryOp::Ne => Ok(Value::Bool(left != right)),
        ast::BinaryOp::Lt => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
            (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a < b)),
            _ => Err(RuntimeError::at(format!("type error: {} < {}", left, right), span)),
        },
        ast::BinaryOp::Le => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
            _ => Err(RuntimeError::at(format!("type error: {} <= {}", left, right), span)),
        },
        ast::BinaryOp::Gt => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
            _ => Err(RuntimeError::at(format!("type error: {} > {}", left, right), span)),
        },
        ast::BinaryOp::Ge => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
            _ => Err(RuntimeError::at(format!("type error: {} >= {}", left, right), span)),
        },
    }
}

fn eval_field_access(
    obj: Value,
    field: &str,
    span: &ast::Span,
) -> Result<Value, RuntimeError> {
    match &obj {
        Value::Record(fields) => fields
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| {
                RuntimeError::at(format!("field '{}' not found in record", field), span)
            }),
        Value::Variant { payload: VariantPayload::Record(fields), .. } => fields
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| {
                RuntimeError::at(format!("field '{}' not found in variant record", field), span)
            }),
        _ => Err(RuntimeError::at(
            format!("field access '{}' on non-record: {}", field, obj),
            span,
        )),
    }
}
