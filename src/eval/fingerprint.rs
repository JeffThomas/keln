use crate::ast::{self, Expr};

/// Structural fingerprint of a function declaration.
///
/// Used by PatternDB for clustering and resilient pattern matching.
/// - `effect_signature`: canonical string of effects + input type + output type
/// - `ast_shape`: recursive shape of the `out:` expression (structure, not values)
/// - `call_graph`: ordered list of function/module names called in the body
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub effect_signature: String,
    pub ast_shape: String,
    pub call_graph: Vec<String>,
}

/// Compute the structural fingerprint of a function declaration.
pub fn fingerprint(decl: &ast::FnDecl) -> Fingerprint {
    Fingerprint {
        effect_signature: effect_signature(&decl.signature),
        ast_shape: shape_expr(&decl.out_clause),
        call_graph: call_graph_expr(&decl.out_clause),
    }
}

// =============================================================================
// Effect signature
// =============================================================================

fn effect_signature(sig: &ast::FnSignature) -> String {
    let effects = format_effects(&sig.effects);
    let input = format_type(&sig.input_type);
    let output = format_type(&sig.output_type);
    format!("{} {} -> {}", effects, input, output)
}

fn format_effects(e: &ast::EffectSet) -> String {
    if e.effects.is_empty() {
        "Pure".to_string()
    } else {
        e.effects.join("+")
    }
}

fn format_type(t: &ast::TypeExpr) -> String {
    match t {
        ast::TypeExpr::Primitive(p, _) => format!("{:?}", p),
        ast::TypeExpr::Never(_) => "Never".to_string(),
        ast::TypeExpr::Named(n, _) => n.clone(),
        ast::TypeExpr::Generic { name, args, .. } => {
            let args: Vec<_> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, args.join(","))
        }
        ast::TypeExpr::Product(fields, _) => {
            let fs: Vec<_> = fields.iter().map(|f| format!("{}:{}", f.name, format_type(&f.type_expr))).collect();
            format!("{{{}}}", fs.join(","))
        }
        ast::TypeExpr::FunctionRef { effect, input, output, .. } => {
            format!("FnRef<{},{},{}>", format_effects(effect), format_type(input), format_type(output))
        }
    }
}

// =============================================================================
// AST shape — structural description of the expression (no literals)
// =============================================================================

fn shape_expr(e: &Expr) -> String {
    match e {
        Expr::IntLiteral(_, _) => "IntLit".to_string(),
        Expr::FloatLiteral(_, _) => "FloatLit".to_string(),
        Expr::StringLiteral(_, _) => "StrLit".to_string(),
        Expr::BoolLiteral(_, _) => "BoolLit".to_string(),
        Expr::UnitLiteral(_) => "Unit".to_string(),
        Expr::Wildcard(_) => "Wildcard".to_string(),
        Expr::Var(_, _) => "Var".to_string(),
        Expr::UpperVar(_, _) => "Variant".to_string(),
        Expr::QualifiedName(parts, _) => format!("Ref({})", parts.join(".")),
        Expr::BinaryOp { op, left, right, .. } => {
            format!("BinOp({:?},{},{})", op, shape_expr(left), shape_expr(right))
        }
        Expr::Call { function, args, .. } => {
            let fn_shape = shape_expr(function);
            let arg_shapes: Vec<_> = args.iter().map(|a| match a {
                ast::Arg::Positional(e) => shape_expr(e),
                ast::Arg::Named(k, e) => format!("{}:{}", k, shape_expr(e)),
            }).collect();
            format!("Call({},{})", fn_shape, arg_shapes.join(","))
        }
        Expr::Pipeline { left, steps, .. } => {
            let step_shapes: Vec<_> = steps.iter().map(shape_expr).collect();
            format!("Pipeline({},{})", shape_expr(left), step_shapes.join("|>"))
        }
        Expr::Match { scrutinee, arms, .. } => {
            let arm_shapes: Vec<_> = arms.iter().map(|a| shape_expr(&a.body)).collect();
            format!("Match({};[{}])", shape_expr(scrutinee), arm_shapes.join(","))
        }
        Expr::DoBlock { stmts, final_expr, .. } => {
            let stmt_shapes: Vec<_> = stmts.iter().map(shape_do_stmt).collect();
            format!("Do([{}],{})", stmt_shapes.join(";"), shape_expr(final_expr))
        }
        Expr::Let(lb) => format!("Let({})", shape_expr(&lb.value)),
        Expr::Record { name, fields, .. } => {
            let ns = name.as_ref().map(|n| shape_expr(n)).unwrap_or_default();
            let fs: Vec<_> = fields.iter().map(|f| format!("{}:{}", f.name, shape_expr(&f.value))).collect();
            format!("Record({},{{{} }})", ns, fs.join(","))
        }
        Expr::List(items, _) => {
            let shapes: Vec<_> = items.iter().map(shape_expr).collect();
            format!("List([{}])", shapes.join(","))
        }
        Expr::FieldAccess { object, field, .. } => {
            format!("Field({}.{})", shape_expr(object), field)
        }
        Expr::Select { arms, timeout, .. } => {
            let n = arms.len();
            let has_timeout = timeout.is_some();
            format!("Select({},{:?})", n, has_timeout)
        }
        Expr::ChannelSend { .. } => "ChSend".to_string(),
        Expr::ChannelRecv(_, _) => "ChRecv".to_string(),
        Expr::ChannelNew { .. } => "ChNew".to_string(),
        Expr::Clone(inner, _) => format!("Clone({})", shape_expr(inner)),
        Expr::With { function, binding, .. } => {
            let b = match binding {
                ast::WithBinding::Named(k, v) => format!("{}:{}", k, shape_expr(v)),
                ast::WithBinding::Record(fields) => {
                    let fs: Vec<_> = fields.iter().map(|f| format!("{}:{}", f.name, shape_expr(&f.value))).collect();
                    format!("{{{}}}", fs.join(","))
                }
            };
            format!("With({},{})", shape_expr(function), b)
        }
        Expr::Paren(inner, _) => shape_expr(inner),
    }
}

fn shape_do_stmt(s: &ast::DoStmt) -> String {
    match s {
        ast::DoStmt::Expr(e) => shape_expr(e),
        ast::DoStmt::Let(lb) => format!("Let({})", shape_expr(&lb.value)),
        ast::DoStmt::ChannelSend { channel, value } => {
            format!("ChSend({},{})", shape_expr(channel), shape_expr(value))
        }
    }
}

// =============================================================================
// Call graph — ordered list of called names
// =============================================================================

fn call_graph_expr(e: &Expr) -> Vec<String> {
    let mut calls = Vec::new();
    collect_calls(e, &mut calls);
    calls.dedup();
    calls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn decl(src: &str) -> ast::FnDecl {
        let prog = parse(src).expect("parse failed");
        prog.declarations.into_iter().find_map(|d| {
            if let ast::TopLevelDecl::FnDecl(fd) = d { Some(*fd) } else { None }
        }).expect("no fn decl found")
    }

    #[test]
    fn test_effect_signature_pure() {
        let fd = decl(r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}"#);
        let fp = fingerprint(&fd);
        assert_eq!(fp.effect_signature, "Pure Int -> Int");
    }

    #[test]
    fn test_effect_signature_io() {
        let fd = decl(r#"fn logMsg { IO String -> Unit
    in: msg
    out: Unit
}"#);
        let fp = fingerprint(&fd);
        assert_eq!(fp.effect_signature, "IO String -> Unit");
    }

    #[test]
    fn test_ast_shape_binop() {
        let fd = decl(r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}"#);
        let fp = fingerprint(&fd);
        assert!(fp.ast_shape.starts_with("BinOp(Add,"), "shape was: {}", fp.ast_shape);
    }

    #[test]
    fn test_call_graph_stdlib_call() {
        let fd = decl(r#"fn trimIt { Pure String -> String
    in: s
    out: String.trim(s)
}"#);
        let fp = fingerprint(&fd);
        assert!(fp.call_graph.contains(&"String.trim".to_string()));
    }

    #[test]
    fn test_call_graph_deduped() {
        let fd = decl(r#"fn addTwice { Pure Int -> Int
    in: n
    out: Int.abs(n) + Int.abs(n)
}"#);
        let fp = fingerprint(&fd);
        assert_eq!(fp.call_graph.iter().filter(|s| *s == "Int.abs").count(), 1);
    }
}

fn collect_calls(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Call { function, args, .. } => {
            match function.as_ref() {
                Expr::Var(name, _) => out.push(name.clone()),
                Expr::QualifiedName(parts, _) => out.push(parts.join(".")),
                other => collect_calls(other, out),
            }
            for a in args {
                match a {
                    ast::Arg::Positional(inner) | ast::Arg::Named(_, inner) => collect_calls(inner, out),
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_calls(left, out);
            collect_calls(right, out);
        }
        Expr::Pipeline { left, steps, .. } => {
            collect_calls(left, out);
            for s in steps { collect_calls(s, out); }
        }
        Expr::Match { scrutinee, arms, .. } => {
            collect_calls(scrutinee, out);
            for a in arms { collect_calls(&a.body, out); }
        }
        Expr::DoBlock { stmts, final_expr, .. } => {
            for s in stmts {
                match s {
                    ast::DoStmt::Expr(e) | ast::DoStmt::Let(ast::LetBinding { value: e, .. }) => collect_calls(e, out),
                    ast::DoStmt::ChannelSend { channel, value } => {
                        collect_calls(channel, out);
                        collect_calls(value, out);
                    }
                }
            }
            collect_calls(final_expr, out);
        }
        Expr::Let(lb) => collect_calls(&lb.value, out),
        Expr::Record { fields, .. } => {
            for f in fields { collect_calls(&f.value, out); }
        }
        Expr::FieldAccess { object, .. } => collect_calls(object, out),
        Expr::Clone(inner, _) | Expr::Paren(inner, _) => collect_calls(inner, out),
        Expr::List(items, _) => {
            for i in items { collect_calls(i, out); }
        }
        Expr::With { function, binding, .. } => {
            collect_calls(function, out);
            match binding {
                ast::WithBinding::Named(_, v) => collect_calls(v, out),
                ast::WithBinding::Record(fields) => {
                    for f in fields { collect_calls(&f.value, out); }
                }
            }
        }
        Expr::ChannelSend { channel, value, .. } => {
            collect_calls(channel, out);
            collect_calls(value, out);
        }
        Expr::ChannelRecv(inner, _) => collect_calls(inner, out),
        Expr::Select { arms, timeout, .. } => {
            for a in arms { collect_calls(&a.body, out); }
            if let Some(t) = timeout { collect_calls(&t.body, out); }
        }
        // Terminals — no sub-expressions
        _ => {}
    }
}
