pub mod result;
mod sample;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use crate::ast;
use crate::eval::{Evaluator, RuntimeError, Value, VariantPayload};
use result::{FnVerifyResult, ForAllOutcome, FuzzMethodResult, FuzzModuleResult, GivenOutcome, ProofStatus};

// =============================================================================
// VerifyExecutor
// =============================================================================

pub struct VerifyExecutor {
    pub evaluator: Evaluator,
    /// Trusted module declarations extracted from the program for fuzz harness.
    pub trusted_modules: Vec<ast::TrustedModuleDecl>,
}

impl VerifyExecutor {
    /// Load a Keln source string and prepare the executor.
    pub fn from_source(source: &str) -> Result<Self, String> {
        let program = crate::parser::parse(source).map_err(|e| format!("{}", e))?;
        let mut evaluator = Evaluator::new();
        evaluator.load_program(&program);
        let trusted_modules = program
            .declarations
            .iter()
            .filter_map(|d| match d {
                ast::TopLevelDecl::TrustedModuleDecl(tm) => Some(tm.clone()),
                _ => None,
            })
            .collect();
        Ok(VerifyExecutor { evaluator, trusted_modules })
    }

    /// Run all verify blocks in the loaded program.
    pub fn verify_all(&mut self) -> Vec<FnVerifyResult> {
        let fn_names: Vec<String> = self
            .evaluator
            .fns
            .keys()
            .cloned()
            .collect();
        let mut results = Vec::new();
        for name in fn_names {
            let fd = self.evaluator.fns.get(&name).cloned();
            if let Some(fd) = fd && fd.verify.is_some() {
                results.push(self.verify_fn(&fd));
            }
        }
        results
    }

    /// Run the verify block for one function declaration.
    pub fn verify_fn(&mut self, fd: &ast::FnDecl) -> FnVerifyResult {
        let stmts = match &fd.verify {
            Some(s) => s.clone(),
            None => {
                return FnVerifyResult {
                    fn_name: fd.name.clone(),
                    given: vec![],
                    forall: vec![],
                }
            }
        };
        self.run_verify_stmts(&fd.name, &stmts)
    }

    fn run_verify_stmts(&mut self, fn_name: &str, stmts: &[ast::VerifyStmt]) -> FnVerifyResult {
        let mut given_results = Vec::new();
        let mut forall_results = Vec::new();
        // Track active FunctionRef mock names (for input substitution)
        let mut active_fn_mocks: HashSet<String> = HashSet::new();

        for stmt in stmts {
            match stmt {
                ast::VerifyStmt::Mock(mock) => {
                    self.register_mock(mock, &mut active_fn_mocks);
                }
                ast::VerifyStmt::Given(gc) => {
                    let r = self.run_given(fn_name, gc, &active_fn_mocks);
                    given_results.push(r);
                }
                ast::VerifyStmt::ForAll(prop) => {
                    let r = self.run_forall(fn_name, prop);
                    forall_results.push(r);
                }
            }
        }

        // Clean up all mocks registered during this verify block
        self.evaluator.mock_fns.clear();

        FnVerifyResult { fn_name: fn_name.to_string(), given: given_results, forall: forall_results }
    }

    // =========================================================================
    // Mock registration
    // =========================================================================

    fn register_mock(&mut self, mock: &ast::MockDecl, active_fn_mocks: &mut HashSet<String>) {
        let mut clauses: Vec<(ast::Pattern, ast::Expr)> = Vec::new();
        let mut is_fn_mock = false;

        for clause in &mock.clauses {
            match clause {
                ast::MockClause::Call { pattern, result } => {
                    is_fn_mock = true;
                    clauses.push((pattern.clone(), result.clone()));
                }
                ast::MockClause::Method { method, patterns, result } => {
                    let key = format!("{}.{}", mock.name, method);
                    let pat = if patterns.is_empty() {
                        ast::Pattern::Wildcard(ast::Span { line: 0, column: 0 })
                    } else if patterns.len() == 1 {
                        patterns[0].clone()
                    } else {
                        // Multiple patterns: match as positional record
                        ast::Pattern::Wildcard(ast::Span { line: 0, column: 0 })
                    };
                    self.evaluator.mock_fns.insert(key, vec![(pat, result.clone())]);
                }
            }
        }

        if is_fn_mock {
            let key = format!("__mock_{}", mock.name);
            self.evaluator.mock_fns.insert(key, clauses);
            active_fn_mocks.insert(mock.name.clone());
        }
    }

    // =========================================================================
    // Given case execution
    // =========================================================================

    fn run_given(
        &mut self,
        fn_name: &str,
        gc: &ast::GivenCase,
        active_fn_mocks: &HashSet<String>,
    ) -> GivenOutcome {
        // Evaluate input, substituting `_` wildcards for mocked FunctionRef params
        let input = match self.eval_given_expr(&gc.input, active_fn_mocks) {
            Ok(v) => v,
            Err(e) => {
                return GivenOutcome {
                    input: "?".to_string(),
                    expected: "?".to_string(),
                    actual: Err(format!("input eval error: {}", e.message)),
                    passed: false,
                }
            }
        };

        let expected = match self.evaluator.eval_expr(&gc.expected) {
            Ok(v) => v,
            Err(e) => {
                return GivenOutcome {
                    input: format!("{}", input),
                    expected: "?".to_string(),
                    actual: Err(format!("expected eval error: {}", e.message)),
                    passed: false,
                }
            }
        };

        let actual = self.evaluator.call_fn(fn_name, input.clone());

        let passed = match &actual {
            Ok(v) => v == &expected,
            Err(_) => false,
        };

        GivenOutcome {
            input: format!("{}", input),
            expected: format!("{}", expected),
            actual: actual.map(|v| format!("{}", v)).map_err(|e| e.message),
            passed,
        }
    }

    /// Evaluate an expression for use as a `given` input, substituting
    /// wildcard fields with mock FnRef values for active FunctionRef mocks.
    fn eval_given_expr(
        &mut self,
        expr: &ast::Expr,
        active_fn_mocks: &HashSet<String>,
    ) -> Result<Value, RuntimeError> {
        match expr {
            ast::Expr::Record { name, fields, .. } => {
                let mut fvs: Vec<(String, Value)> = Vec::new();
                for fv in fields {
                    let v = if matches!(fv.value.as_ref(), ast::Expr::Wildcard(_))
                        && active_fn_mocks.contains(&fv.name)
                    {
                        Value::FnRef(format!("__mock_{}", fv.name))
                    } else {
                        self.evaluator.eval_expr(&fv.value)?
                    };
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
            other => self.evaluator.eval_expr(other),
        }
    }

    // =========================================================================
    // ForAll property execution
    // =========================================================================

    fn run_forall(&mut self, _fn_name: &str, prop: &ast::ForAllProperty) -> ForAllOutcome {
        const BUDGET: usize = 1000;
        let samples = sample::cartesian_samples(&prop.bindings, BUDGET);
        let mut iterations = 0;

        for binding_vals in samples {
            iterations += 1;
            self.evaluator.env.push_scope();
            for (name, val) in &binding_vals {
                self.evaluator.env.bind(name, val.clone());
            }

            let result = self.eval_logic(&prop.body);

            self.evaluator.env.pop_scope();

            match result {
                Ok(true) => {} // property holds for this sample
                Ok(false) => {
                    return ForAllOutcome {
                        status: ProofStatus::Failed,
                        counterexample: Some(
                            binding_vals
                                .into_iter()
                                .map(|(n, v)| (n, format!("{}", v)))
                                .collect(),
                        ),
                        iterations,
                    };
                }
                Err(e) => {
                    return ForAllOutcome {
                        status: ProofStatus::Error(e.message),
                        counterexample: Some(
                            binding_vals
                                .into_iter()
                                .map(|(n, v)| (n, format!("{}", v)))
                                .collect(),
                        ),
                        iterations,
                    };
                }
            }
        }

        ForAllOutcome {
            status: ProofStatus::Passed { iterations },
            counterexample: None,
            iterations,
        }
    }

    fn eval_logic(&mut self, logic: &ast::LogicExpr) -> Result<bool, RuntimeError> {
        match logic {
            ast::LogicExpr::Comparison { left, op, right } => {
                let lv = self.evaluator.eval_expr(left)?;
                let rv = self.evaluator.eval_expr(right)?;
                Ok(compare_values(&lv, op, &rv))
            }
            ast::LogicExpr::DoesNotCrash(expr) => {
                match self.evaluator.eval_expr(expr)? {
                    Value::Bool(b) => Ok(b),
                    _ => Ok(true),
                }
            }
            ast::LogicExpr::Not(inner) => {
                let b = self.eval_logic(inner)?;
                Ok(!b)
            }
            ast::LogicExpr::And(a, b) => {
                if !self.eval_logic(a)? {
                    return Ok(false);
                }
                self.eval_logic(b)
            }
            ast::LogicExpr::Or(a, b) => {
                if self.eval_logic(a)? {
                    return Ok(true);
                }
                self.eval_logic(b)
            }
            ast::LogicExpr::Implies(p, q) => {
                if !self.eval_logic(p)? {
                    return Ok(true); // vacuously true
                }
                self.eval_logic(q)
            }
        }
    }

    // =========================================================================
    // Fuzz harness for trusted modules
    // =========================================================================

    /// Run the fuzz harness against all trusted module declarations in the program.
    /// For each method with a fuzz block, samples inputs and checks the declared invariant.
    pub fn fuzz_trusted_modules(&mut self) -> Vec<FuzzModuleResult> {
        let trusted = self.trusted_modules.clone();
        let mut results = Vec::new();

        for tm in &trusted {
            let has_fuzz = tm.fuzz.is_some();
            let fuzz_decls = tm.fuzz.clone().unwrap_or_default();

            if !has_fuzz {
                results.push(FuzzModuleResult {
                    module_name: tm.name.clone(),
                    methods: vec![],
                    has_fuzz_block: false,
                });
                continue;
            }

            let mut method_results = Vec::new();
            for fuzz_decl in &fuzz_decls {
                let fn_qualified = format!("{}.{}", tm.name, fuzz_decl.fn_name);
                let invariant_name = match &fuzz_decl.invariant {
                    ast::FuzzInvariant::CrashesNever => "crashes_never",
                    ast::FuzzInvariant::ReturnsResult => "returns_result",
                    ast::FuzzInvariant::Deterministic => "deterministic",
                };

                // Generate sample inputs for each declared input type
                let input_samples: Vec<Vec<Value>> = fuzz_decl
                    .input_types
                    .iter()
                    .map(|ty| {
                        let binding = ast::ForAllBinding {
                            name: "_fuzz".to_string(),
                            type_expr: ty.clone(),
                            refinement: None,
                            span: fuzz_decl.span.clone(),
                        };
                        sample::sample_for_binding(&binding)
                    })
                    .collect();

                // Build cross-product of input combinations (up to 20)
                let combos = cross_product(&input_samples, 20);
                let iter_count = combos.len();

                let mut passed = true;
                let mut failure: Option<String> = None;

                for combo in &combos {
                    let args = combo.clone();
                    let call_result = crate::eval::stdlib::dispatch(
                        &fn_qualified,
                        args.clone(),
                        &mut self.evaluator,
                    );
                    match &fuzz_decl.invariant {
                        ast::FuzzInvariant::CrashesNever => {
                            if let Err(e) = call_result {
                                passed = false;
                                failure = Some(format!(
                                    "crashed on input {:?}: {}",
                                    args,
                                    e
                                ));
                                break;
                            }
                        }
                        ast::FuzzInvariant::ReturnsResult => {
                            match &call_result {
                                Err(e) => {
                                    passed = false;
                                    failure = Some(format!("crashed: {}", e));
                                    break;
                                }
                                Ok(v) => {
                                    if !is_result_value(v) {
                                        passed = false;
                                        failure = Some(format!("returned non-Result: {}", v));
                                        break;
                                    }
                                }
                            }
                        }
                        ast::FuzzInvariant::Deterministic => {
                            let first = call_result.ok();
                            let second = crate::eval::stdlib::dispatch(
                                &fn_qualified,
                                args.clone(),
                                &mut self.evaluator,
                            )
                            .ok();
                            if first != second {
                                passed = false;
                                failure = Some(format!(
                                    "non-deterministic on input {:?}",
                                    args
                                ));
                                break;
                            }
                        }
                    }
                }

                method_results.push(FuzzMethodResult {
                    fn_name: fuzz_decl.fn_name.clone(),
                    invariant: invariant_name.to_string(),
                    iterations: iter_count,
                    passed,
                    failure,
                });
            }

            results.push(FuzzModuleResult {
                module_name: tm.name.clone(),
                methods: method_results,
                has_fuzz_block: true,
            });
        }

        results
    }
}

// =========================================================================
// Value comparison for logic expressions
// =========================================================================

fn compare_values(left: &Value, op: &ast::ComparisonOp, right: &Value) -> bool {
    match op {
        ast::ComparisonOp::Eq => left == right,
        ast::ComparisonOp::Ne => left != right,
        ast::ComparisonOp::Lt => value_lt(left, right),
        ast::ComparisonOp::Le => value_lt(left, right) || left == right,
        ast::ComparisonOp::Gt => value_lt(right, left),
        ast::ComparisonOp::Ge => value_lt(right, left) || left == right,
    }
}

fn value_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x < y,
        (Value::Float(x), Value::Float(y)) => x < y,
        (Value::Str(x), Value::Str(y)) => x < y,
        _ => false,
    }
}

// =========================================================================
// Fuzz harness helpers
// =========================================================================

/// Build the cross product of per-argument sample lists, capped at `max` combinations.
fn cross_product(per_arg: &[Vec<Value>], max: usize) -> Vec<Vec<Value>> {
    if per_arg.is_empty() {
        return vec![vec![]];
    }
    let mut result: Vec<Vec<Value>> = vec![vec![]];
    for samples in per_arg {
        let mut next = Vec::new();
        'outer: for prefix in &result {
            for s in samples {
                if next.len() >= max {
                    break 'outer;
                }
                let mut row = prefix.clone();
                row.push(s.clone());
                next.push(row);
            }
        }
        result = next;
        if result.len() >= max {
            result.truncate(max);
            break;
        }
    }
    result
}

/// Returns true if `v` is an Ok or Err variant (i.e. a Result value).
fn is_result_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Variant { name, .. } if name == "Ok" || name == "Err"
    )
}
