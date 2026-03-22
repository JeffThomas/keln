#[cfg(test)]
mod tests {
    use crate::eval::{eval_fn, load_source, Value, VariantPayload};

    fn ok(v: Value) -> Value {
        Value::Variant {
            name: "Ok".to_string(),
            payload: VariantPayload::Tuple(Box::new(v)),
        }
    }

    fn err(v: Value) -> Value {
        Value::Variant {
            name: "Err".to_string(),
            payload: VariantPayload::Tuple(Box::new(v)),
        }
    }

    fn some(v: Value) -> Value {
        Value::Variant {
            name: "Some".to_string(),
            payload: VariantPayload::Tuple(Box::new(v)),
        }
    }

    fn none() -> Value {
        Value::Variant { name: "None".to_string(), payload: VariantPayload::Unit }
    }

    // =========================================================================
    // Arithmetic and literals
    // =========================================================================

    #[test]
    fn test_eval_int_literal() {
        let result = eval_fn(
            "fn answer { Pure Unit -> Int\n    in: _\n    out: 42\n}",
            "answer",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_arithmetic() {
        let result = eval_fn(
            "fn add { Pure Int -> Int\n    in: n\n    out: n + 1\n}",
            "add",
            Value::Int(5),
        );
        assert_eq!(result, Ok(Value::Int(6)));
    }

    #[test]
    fn test_eval_string_literal() {
        let result = eval_fn(
            r#"fn greet { Pure Unit -> String
    in: _
    out: "hello"
}"#,
            "greet",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Str("hello".to_string())));
    }

    #[test]
    fn test_eval_bool_eq() {
        let result = eval_fn(
            "fn isEven { Pure Int -> Bool\n    in: n\n    out: n == 0\n}",
            "isEven",
            Value::Int(0),
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    // =========================================================================
    // Record construction and field access
    // =========================================================================

    #[test]
    fn test_eval_record_construction() {
        let result = eval_fn(
            r#"fn makePoint { Pure Unit -> { x: Int, y: Int }
    in: _
    out: { x: 3, y: 4 }
}"#,
            "makePoint",
            Value::Unit,
        );
        assert_eq!(
            result,
            Ok(Value::Record(vec![
                ("x".to_string(), Value::Int(3)),
                ("y".to_string(), Value::Int(4)),
            ]))
        );
    }

    #[test]
    fn test_eval_field_access() {
        let result = eval_fn(
            r#"fn getX { Pure { x: Int, y: Int } -> Int
    in: p
    out: p.x
}"#,
            "getX",
            Value::Record(vec![
                ("x".to_string(), Value::Int(7)),
                ("y".to_string(), Value::Int(2)),
            ]),
        );
        assert_eq!(result, Ok(Value::Int(7)));
    }

    // =========================================================================
    // Match expressions
    // =========================================================================

    #[test]
    fn test_eval_match_bool() {
        let result = eval_fn(
            r#"fn describe { Pure Bool -> String
    in: b
    out: match b {
        true  -> "yes"
        false -> "no"
    }
}"#,
            "describe",
            Value::Bool(true),
        );
        assert_eq!(result, Ok(Value::Str("yes".to_string())));
    }

    #[test]
    fn test_eval_match_variant() {
        let result = eval_fn(
            r#"fn unwrapOr { Pure Result<Int, String> -> Int
    in: r
    out: match r {
        Ok(n)  -> n
        Err(_) -> 0
    }
}"#,
            "unwrapOr",
            ok(Value::Int(42)),
        );
        assert_eq!(result, Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_match_wildcard_arm() {
        let result = eval_fn(
            r#"fn unwrapOr { Pure Result<Int, String> -> Int
    in: r
    out: match r {
        Ok(n) -> n
        _     -> 0
    }
}"#,
            "unwrapOr",
            err(Value::Str("oops".to_string())),
        );
        assert_eq!(result, Ok(Value::Int(0)));
    }

    // =========================================================================
    // Do block
    // =========================================================================

    #[test]
    fn test_eval_do_block_let() {
        let result = eval_fn(
            r#"fn compute { Pure Int -> Int
    in: n
    out: do {
        let doubled = n + n
        doubled + 1
    }
}"#,
            "compute",
            Value::Int(3),
        );
        assert_eq!(result, Ok(Value::Int(7)));
    }

    // =========================================================================
    // Function calls
    // =========================================================================

    #[test]
    fn test_eval_fn_call() {
        let result = eval_fn(
            r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}
fn quadruple { Pure Int -> Int
    in: n
    out: double(double(n))
}"#,
            "quadruple",
            Value::Int(3),
        );
        assert_eq!(result, Ok(Value::Int(12)));
    }

    // =========================================================================
    // Stdlib: Result
    // =========================================================================

    #[test]
    fn test_eval_result_ok() {
        let result = eval_fn(
            r#"fn wrap { Pure Int -> Result<Int, String>
    in: n
    out: Result.ok(n)
}"#,
            "wrap",
            Value::Int(5),
        );
        assert_eq!(result, Ok(ok(Value::Int(5))));
    }

    #[test]
    fn test_eval_result_map() {
        let result = eval_fn(
            r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}
fn mapResult { Pure Result<Int, String> -> Result<Int, String>
    in: r
    out: Result.map(r, double)
}"#,
            "mapResult",
            ok(Value::Int(4)),
        );
        assert_eq!(result, Ok(ok(Value::Int(8))));
    }

    #[test]
    fn test_eval_result_map_err_passthrough() {
        let result = eval_fn(
            r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}
fn mapResult { Pure Result<Int, String> -> Result<Int, String>
    in: r
    out: Result.map(r, double)
}"#,
            "mapResult",
            err(Value::Str("nope".to_string())),
        );
        assert_eq!(result, Ok(err(Value::Str("nope".to_string()))));
    }

    // =========================================================================
    // Stdlib: List
    // =========================================================================

    #[test]
    fn test_eval_list_map() {
        let result = eval_fn(
            r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}
fn doMap { Pure List<Int> -> List<Int>
    in: xs
    out: List.map(xs, double)
}"#,
            "doMap",
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        );
        assert_eq!(
            result,
            Ok(Value::List(vec![Value::Int(2), Value::Int(4), Value::Int(6)]))
        );
    }

    #[test]
    fn test_eval_list_filter() {
        let result = eval_fn(
            r#"fn isPositive { Pure Int -> Bool
    in: n
    out: n > 0
}
fn doFilter { Pure List<Int> -> List<Int>
    in: xs
    out: List.filter(xs, isPositive)
}"#,
            "doFilter",
            Value::List(vec![
                Value::Int(-1),
                Value::Int(2),
                Value::Int(-3),
                Value::Int(4),
            ]),
        );
        assert_eq!(
            result,
            Ok(Value::List(vec![Value::Int(2), Value::Int(4)]))
        );
    }

    #[test]
    fn test_eval_list_len() {
        let result = eval_fn(
            r#"fn count { Pure List<Int> -> Int
    in: xs
    out: List.len(xs)
}"#,
            "count",
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        );
        assert_eq!(result, Ok(Value::Int(3)));
    }

    // =========================================================================
    // Stdlib: String
    // =========================================================================

    #[test]
    fn test_eval_string_concat() {
        let result = eval_fn(
            r#"fn greet { Pure String -> String
    in: name
    out: String.concat("Hello, ", name)
}"#,
            "greet",
            Value::Str("World".to_string()),
        );
        assert_eq!(result, Ok(Value::Str("Hello, World".to_string())));
    }

    #[test]
    fn test_eval_int_to_string() {
        let result = eval_fn(
            r#"fn show { Pure Int -> String
    in: n
    out: Int.toString(n)
}"#,
            "show",
            Value::Int(42),
        );
        assert_eq!(result, Ok(Value::Str("42".to_string())));
    }

    // =========================================================================
    // Recursion with TCO (tail-call test)
    // =========================================================================

    #[test]
    fn test_eval_recursive_sum() {
        let result = eval_fn(
            r#"fn sumTo { Pure Int -> Int
    in: n
    out: match n {
        0 -> 0
        _ -> n + sumTo(n - 1)
    }
}"#,
            "sumTo",
            Value::Int(10),
        );
        assert_eq!(result, Ok(Value::Int(55)));
    }

    #[test]
    fn test_eval_tail_recursive_countdown() {
        // Deep tail-recursive call — exercises the trampoline
        let result = eval_fn(
            r#"fn countdown { Pure Int -> Int
    in: n
    out: match n {
        0 -> 0
        _ -> countdown(n - 1)
    }
}"#,
            "countdown",
            Value::Int(50_000),
        );
        assert_eq!(result, Ok(Value::Int(0)));
    }

    // =========================================================================
    // Pipeline
    // =========================================================================

    #[test]
    fn test_eval_pipeline() {
        let result = eval_fn(
            r#"fn double { Pure Int -> Int
    in: n
    out: n + n
}
fn addOne { Pure Int -> Int
    in: n
    out: n + 1
}
fn process { Pure Int -> Int
    in: n
    out: n |> double |> addOne
}"#,
            "process",
            Value::Int(5),
        );
        assert_eq!(result, Ok(Value::Int(11)));
    }

    // =========================================================================
    // Channel operations (synchronous)
    // =========================================================================

    #[test]
    fn test_eval_channel_send_recv() {
        let result = eval_fn(
            r#"fn roundtrip { IO Unit -> Int
    in: _
    out: do {
        let ch = Channel.new<Int>()
        ch <- 99
        let v = <-ch
        v
    }
}"#,
            "roundtrip",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Int(99)));
    }
}
