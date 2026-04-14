use crate::ast::{self, ForAllBinding, PrimitiveType, RefinementConstraint, TypeExpr};
use crate::ast::ComparisonOp;
use crate::ast::Number as AstNumber;
use crate::eval::{Value, VariantPayload};

// =============================================================================
// Per-binding sample generation
// =============================================================================

pub fn sample_for_binding(binding: &ForAllBinding) -> Vec<Value> {
    sample_for_type(&binding.type_expr, binding.refinement.as_ref())
}

fn sample_for_type(ty: &TypeExpr, refinement: Option<&RefinementConstraint>) -> Vec<Value> {
    match ty {
        TypeExpr::Primitive(prim, _) => match prim {
            PrimitiveType::Int => sample_int(refinement),
            PrimitiveType::Float => sample_float(refinement),
            PrimitiveType::Bool => vec![Value::Bool(false), Value::Bool(true)],
            PrimitiveType::String => sample_string(refinement),
            PrimitiveType::Bytes => {
                vec![Value::Bytes(vec![]), Value::Bytes(vec![0x61, 0x62])]
            }
            PrimitiveType::Unit => vec![Value::Unit],
        },
        TypeExpr::Named(name, _) => sample_named(name),
        TypeExpr::Generic { name, args, .. } => sample_generic(name, args),
        TypeExpr::Product(fields, _) => sample_product(fields),
        TypeExpr::Never(_) => vec![],
        TypeExpr::FunctionRef { .. } => vec![],
    }
}

// =========================================================================
// Int sampling
// =========================================================================

fn sample_int(refinement: Option<&RefinementConstraint>) -> Vec<Value> {
    let (lo, hi) = int_bounds(refinement);
    let mut samples = vec![];
    samples.push(lo);
    samples.push(hi);
    if lo < hi {
        samples.push(lo + (hi - lo) / 2);
    }
    if lo + 1 < hi {
        samples.push(lo + 1);
        samples.push(hi - 1);
    }
    if lo <= 0 && 0 <= hi {
        samples.push(0);
    }
    if lo <= 1 && 1 <= hi {
        samples.push(1);
    }
    // Deterministic pseudo-random values using LCG
    let range = hi - lo;
    if range > 0 {
        let mut state: u64 = 12_345_678_901_234_567;
        for _ in 0..8 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let v = lo + ((state >> 33) as i64).unsigned_abs() as i64 % (range + 1);
            samples.push(v);
        }
    }
    samples.sort_unstable();
    samples.dedup();
    samples.into_iter().map(Value::Int).collect()
}

fn int_bounds(refinement: Option<&RefinementConstraint>) -> (i64, i64) {
    match refinement {
        None => (-10, 10),
        Some(RefinementConstraint::Range(lo, hi)) => {
            (ast_number_to_i64(lo), ast_number_to_i64(hi))
        }
        Some(RefinementConstraint::Comparison(op, n)) => {
            let n = ast_number_to_i64(n);
            match op {
                ComparisonOp::Ge => (n, n + 20),
                ComparisonOp::Gt => (n + 1, n + 21),
                ComparisonOp::Le => (n - 20, n),
                ComparisonOp::Lt => (n - 21, n - 1),
                ComparisonOp::Eq => (n, n),
                ComparisonOp::Ne => (-10, 10),
            }
        }
        _ => (-10, 10),
    }
}

fn ast_number_to_i64(n: &AstNumber) -> i64 {
    match n {
        AstNumber::Int(i) => *i,
        AstNumber::Float(f) => *f as i64,
    }
}

// =========================================================================
// Float sampling
// =========================================================================

fn sample_float(refinement: Option<&RefinementConstraint>) -> Vec<Value> {
    let mut samples = vec![0.0_f64, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0];
    if let Some(RefinementConstraint::Range(lo, hi)) = refinement {
        let lo = ast_number_to_f64(lo);
        let hi = ast_number_to_f64(hi);
        samples.push(lo);
        samples.push(hi);
        samples.push(lo + (hi - lo) / 2.0);
        samples.push(lo + (hi - lo) / 4.0);
        samples.push(lo + 3.0 * (hi - lo) / 4.0);
    }
    samples.into_iter().map(Value::Float).collect()
}

fn ast_number_to_f64(n: &AstNumber) -> f64 {
    match n {
        AstNumber::Int(i) => *i as f64,
        AstNumber::Float(f) => *f,
    }
}

// =========================================================================
// String sampling
// =========================================================================

fn sample_string(refinement: Option<&RefinementConstraint>) -> Vec<Value> {
    let mut samples = vec![
        String::new(),
        "a".to_string(),
        "hello".to_string(),
        "hello world".to_string(),
        "123".to_string(),
    ];
    if let Some(RefinementConstraint::Length(ComparisonOp::Gt, n)) = refinement {
        samples.push("x".repeat((*n + 1) as usize));
    }
    samples.into_iter().map(Value::Str).collect()
}

// =========================================================================
// Named type sampling
// =========================================================================

fn sample_named(name: &str) -> Vec<Value> {
    match name {
        "Bool" => vec![Value::Bool(false), Value::Bool(true)],
        "Int" => sample_int(None),
        "Float" => sample_float(None),
        "String" => sample_string(None),
        _ => vec![Value::Variant {
            name: name.to_string(),
            payload: VariantPayload::Unit,
        }],
    }
}

// =========================================================================
// Generic type sampling
// =========================================================================

fn sample_generic(name: &str, args: &[TypeExpr]) -> Vec<Value> {
    match name {
        "Channel" | "Task" => vec![], // non-samplable; verified by type checker
        "List" => {
            let inner: Vec<Value> = args
                .first()
                .map(|t| sample_for_type(t, None))
                .unwrap_or_default();
            let mut v = vec![Value::List(std::rc::Rc::new(vec![]))];
            if !inner.is_empty() {
                v.push(Value::List(std::rc::Rc::new(vec![inner[0].clone()])));
                if inner.len() > 1 {
                    v.push(Value::List(std::rc::Rc::new(inner[..2].to_vec())));
                }
            }
            v
        }
        "Maybe" => {
            let inner: Vec<Value> = args
                .first()
                .map(|t| sample_for_type(t, None))
                .unwrap_or_default();
            let mut v = vec![Value::Variant {
                name: "None".to_string(),
                payload: VariantPayload::Unit,
            }];
            for item in inner.into_iter().take(2) {
                v.push(Value::Variant {
                    name: "Some".to_string(),
                    payload: VariantPayload::Tuple(Box::new(item)),
                });
            }
            v
        }
        "Result" => {
            let ok_inner: Vec<Value> = args
                .first()
                .map(|t| sample_for_type(t, None))
                .unwrap_or_default();
            let err_inner: Vec<Value> = args
                .get(1)
                .map(|t| sample_for_type(t, None))
                .unwrap_or_default();
            let mut v = vec![];
            for item in ok_inner.into_iter().take(2) {
                v.push(Value::Variant {
                    name: "Ok".to_string(),
                    payload: VariantPayload::Tuple(Box::new(item)),
                });
            }
            for item in err_inner.into_iter().take(2) {
                v.push(Value::Variant {
                    name: "Err".to_string(),
                    payload: VariantPayload::Tuple(Box::new(item)),
                });
            }
            v
        }
        _ => vec![Value::Unit],
    }
}

// =========================================================================
// Product type (record) sampling: one sample with the first value per field
// =========================================================================

fn sample_product(fields: &[ast::FieldTypeDecl]) -> Vec<Value> {
    let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    let vals: Vec<Value> = fields
        .iter()
        .map(|f| {
            sample_for_type(&f.type_expr, f.refinement.as_ref())
                .into_iter()
                .next()
                .unwrap_or(Value::Unit)
        })
        .collect();
    let layout = crate::eval::intern_layout(&names);
    vec![Value::Record(layout, vals)]
}

// =============================================================================
// Cartesian product generation
// =============================================================================

/// Generate all combinations of per-binding samples, up to `budget`.
/// Returns a Vec of rows, each row being a Vec of (name, value) pairs.
pub fn cartesian_samples(bindings: &[ForAllBinding], budget: usize) -> Vec<Vec<(String, Value)>> {
    if bindings.is_empty() {
        return vec![vec![]];
    }

    let all_samples: Vec<Vec<Value>> =
        bindings.iter().map(sample_for_binding).collect();

    let mut result: Vec<Vec<(String, Value)>> = vec![vec![]];

    for (binding, samples) in bindings.iter().zip(all_samples.iter()) {
        let mut next: Vec<Vec<(String, Value)>> = Vec::new();
        'outer: for existing in &result {
            for sample in samples {
                let mut row = existing.clone();
                row.push((binding.name.clone(), sample.clone()));
                next.push(row);
                if next.len() >= budget {
                    break 'outer;
                }
            }
        }
        result = next;
        if result.len() >= budget {
            result.truncate(budget);
            break;
        }
    }

    result
}
