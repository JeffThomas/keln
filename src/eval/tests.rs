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
    // Compact helper function scoping
    // =========================================================================

    #[test]
    fn test_compact_helper_same_input_type() {
        let result = eval_fn(
            r#"fn doubled { Pure Int -> Int
    in: n
    out: twice(n)
    helpers: {
        twice :: Pure Int -> Int => it + it
    }
}"#,
            "doubled",
            Value::Int(7),
        );
        assert_eq!(result, Ok(Value::Int(14)));
    }

    #[test]
    fn test_compact_helper_different_input_type() {
        let result = eval_fn(
            r#"fn summarize { Pure String -> Int
    in: s
    out: doubled(String.len(s))
    helpers: {
        doubled :: Pure Int -> Int => it + it
    }
}"#,
            "summarize",
            Value::Str("hello".to_string()),
        );
        assert_eq!(result, Ok(Value::Int(10)));
    }

    #[test]
    fn test_compact_helper_returns_bool() {
        let result = eval_fn(
            r#"fn check { Pure Int -> Bool
    in: n
    out: isPositive(n)
    helpers: {
        isPositive :: Pure Int -> Bool => it > 0
    }
}"#,
            "check",
            Value::Int(5),
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_compact_helper_chained() {
        let result = eval_fn(
            r#"fn process { Pure Int -> Int
    in: n
    out: addTwo(triple(n))
    helpers: {
        triple :: Pure Int -> Int => it * 3
        addTwo :: Pure Int -> Int => it + 2
    }
}"#,
            "process",
            Value::Int(4),
        );
        assert_eq!(result, Ok(Value::Int(14)));
    }

    // =========================================================================
    // Refinement constraint checks (item 10)
    // =========================================================================

    #[test]
    fn test_refinement_range_pass() {
        let result = eval_fn(
            r#"type Packet = | Frame { port: Int where 1..65535 }
fn mkFrame { Pure Int -> Packet
    in: p
    out: Frame { port: p }
}"#,
            "mkFrame",
            Value::Int(8080),
        );
        assert!(result.is_ok(), "8080 should be in 1..65535");
    }

    #[test]
    fn test_refinement_range_fail_low() {
        let result = eval_fn(
            r#"type Packet = | Frame { port: Int where 1..65535 }
fn mkFrame { Pure Int -> Packet
    in: p
    out: Frame { port: p }
}"#,
            "mkFrame",
            Value::Int(0),
        );
        assert!(result.is_err(), "0 should violate 1..65535");
        assert!(result.unwrap_err().contains("out of range"), "error should mention out of range");
    }

    #[test]
    fn test_refinement_comparison_pass() {
        let result = eval_fn(
            r#"type Job = | Running { attempt: Int where >= 1 }
fn mkRunning { Pure Int -> Job
    in: n
    out: Running { attempt: n }
}"#,
            "mkRunning",
            Value::Int(3),
        );
        assert!(result.is_ok(), "3 >= 1 should pass");
    }

    #[test]
    fn test_refinement_comparison_fail() {
        let result = eval_fn(
            r#"type Job = | Running { attempt: Int where >= 1 }
fn mkRunning { Pure Int -> Job
    in: n
    out: Running { attempt: n }
}"#,
            "mkRunning",
            Value::Int(0),
        );
        assert!(result.is_err(), "0 >= 1 should fail");
    }

    #[test]
    fn test_refinement_length_pass() {
        let result = eval_fn(
            r#"type Record = | Named { label: String where len > 0 }
fn mkNamed { Pure String -> Record
    in: s
    out: Named { label: s }
}"#,
            "mkNamed",
            Value::Str("hello".to_string()),
        );
        assert!(result.is_ok(), "non-empty string should pass len > 0");
    }

    #[test]
    fn test_refinement_length_fail() {
        let result = eval_fn(
            r#"type Record = | Named { label: String where len > 0 }
fn mkNamed { Pure String -> Record
    in: s
    out: Named { label: s }
}"#,
            "mkNamed",
            Value::Str("".to_string()),
        );
        assert!(result.is_err(), "empty string should fail len > 0");
    }

    #[test]
    fn test_refinement_product_type_pass() {
        let result = eval_fn(
            r#"type Config = { delay: Int where >= 1, attempts: Int where 1..10 }
fn mkConfig { Pure Unit -> Config
    in: _
    out: Config { delay: 30, attempts: 3 }
}"#,
            "mkConfig",
            Value::Unit,
        );
        assert!(result.is_ok(), "valid Config should construct fine: got {:?}", result);
    }

    #[test]
    fn test_refinement_product_type_fail() {
        let result = eval_fn(
            r#"type Config = { delay: Int where >= 1, attempts: Int where 1..10 }
fn mkConfig { Pure Unit -> Config
    in: _
    out: Config { delay: 30, attempts: 11 }
}"#,
            "mkConfig",
            Value::Unit,
        );
        assert!(result.is_err(), "attempts: 11 should violate 1..10");
    }

    // =========================================================================
    // Task spawn + await (item 9)
    // =========================================================================

    #[test]
    fn test_task_spawn_fn_ref() {
        let result = eval_fn(
            r#"fn compute { IO Unit -> Int
    in: _
    out: 42
}
fn runIt { IO Unit -> Int
    in: _
    out: do {
        let t = Task.spawn(compute)
        Task.await(t)
    }
}"#,
            "runIt",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Int(42)));
    }

    #[test]
    fn test_task_await_all() {
        let result = eval_fn(
            r#"fn computeA { IO Unit -> Int
    in: _
    out: 11
}
fn computeB { IO Unit -> Int
    in: _
    out: 12
}
fn runAll { IO Unit -> List<Int>
    in: _
    out: do {
        let t1 = Task.spawn(computeA)
        let t2 = Task.spawn(computeB)
        Task.awaitAll([t1, t2])
    }
}"#,
            "runAll",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::List(vec![Value::Int(11), Value::Int(12)])));
    }

    // =========================================================================
    // Clone operation
    // =========================================================================

    #[test]
    fn test_clone_returns_value() {
        let result = eval_fn(
            r#"fn identity { Pure Int -> Int
    in: n
    out: clone(n)
}"#,
            "identity",
            Value::Int(42),
        );
        assert_eq!(result, Ok(Value::Int(42)));
    }

    #[test]
    fn test_clone_in_do_block() {
        let result = eval_fn(
            r#"fn doubled { Pure Int -> Int
    in: n
    out: do {
        let c = clone(n)
        c + n
    }
}"#,
            "doubled",
            Value::Int(5),
        );
        assert_eq!(result, Ok(Value::Int(10)));
    }

    // =========================================================================
    // Log module
    // =========================================================================

    #[test]
    fn test_log_info_returns_unit() {
        let result = eval_fn(
            r#"fn logIt { Log String -> Unit
    in: msg
    out: Log.info(msg)
}"#,
            "logIt",
            Value::Str("hello".to_string()),
        );
        assert_eq!(result, Ok(Value::Unit));
    }

    #[test]
    fn test_log_error_returns_unit() {
        let result = eval_fn(
            r#"fn logErr { Log String -> Unit
    in: msg
    out: Log.error(msg)
}"#,
            "logErr",
            Value::Str("boom".to_string()),
        );
        assert_eq!(result, Ok(Value::Unit));
    }

    // =========================================================================
    // Float complete arithmetic
    // =========================================================================

    #[test]
    fn test_float_add() {
        let result = eval_fn(
            r#"fn addFloats { Pure Unit -> Float
    in: _
    out: Float.add(1.5, 2.5)
}"#,
            "addFloats",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(4.0)));
    }

    #[test]
    fn test_float_multiply() {
        let result = eval_fn(
            r#"fn mulFloats { Pure Unit -> Float
    in: _
    out: Float.multiply(3.0, 4.0)
}"#,
            "mulFloats",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(12.0)));
    }

    #[test]
    fn test_float_divide() {
        let result = eval_fn(
            r#"fn divFloats { Pure Unit -> Float
    in: _
    out: Float.divide(10.0, 4.0)
}"#,
            "divFloats",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(2.5)));
    }

    #[test]
    fn test_float_round() {
        let result = eval_fn(
            r#"fn roundIt { Pure Unit -> Float
    in: _
    out: Float.round(2.7)
}"#,
            "roundIt",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(3.0)));
    }

    #[test]
    fn test_float_to_int() {
        let result = eval_fn(
            r#"fn truncate { Pure Unit -> Int
    in: _
    out: Float.toInt(9.9)
}"#,
            "truncate",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Int(9)));
    }

    #[test]
    fn test_float_from_int() {
        let result = eval_fn(
            r#"fn convertToFloat { Pure Unit -> Float
    in: _
    out: Float.fromInt(7)
}"#,
            "convertToFloat",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(7.0)));
    }

    #[test]
    fn test_float_compare_less() {
        let result = eval_fn(
            r#"fn cmp { Pure Unit -> Bool
    in: _
    out: match Float.compare(1.0, 2.0) {
        LessThan    -> true
        Equal       -> false
        GreaterThan -> false
    }
}"#,
            "cmp",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    // =========================================================================
    // Int additions
    // =========================================================================

    #[test]
    fn test_int_to_float() {
        let result = eval_fn(
            r#"fn intToFloat { Pure Unit -> Float
    in: _
    out: Int.toFloat(3)
}"#,
            "intToFloat",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Float(3.0)));
    }

    #[test]
    fn test_int_pow() {
        let result = eval_fn(
            r#"fn square { Pure Unit -> Int
    in: _
    out: Int.pow(3, 4)
}"#,
            "square",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Int(81)));
    }

    // =========================================================================
    // Duration and Timestamp
    // =========================================================================

    #[test]
    fn test_duration_ms() {
        let result = eval_fn(
            r#"fn makeDur { Pure Unit -> Bool
    in: _
    out: match Duration.ms(500) {
        _ -> true
    }
}"#,
            "makeDur",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_duration_seconds_and_add() {
        let result = eval_fn(
            r#"fn totalMs { Pure Unit -> Bool
    in: _
    out: do {
        let d1 = Duration.seconds(2)
        let d2 = Duration.ms(500)
        let total = Duration.add(d1, d2)
        match total {
            _ -> true
        }
    }
}"#,
            "totalMs",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_duration_multiply() {
        let result = eval_fn(
            r#"fn tripled { Pure Unit -> Bool
    in: _
    out: match Duration.multiply(Duration.seconds(1), 3) {
        _ -> true
    }
}"#,
            "tripled",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_timestamp_gte() {
        let result = eval_fn(
            r#"fn checkOrder { Clock Unit -> Bool
    in: _
    out: do {
        let t1 = Clock.now()
        let t2 = Clock.now()
        Timestamp.gte(t2, t1)
    }
}"#,
            "checkOrder",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_timestamp_add_sub_roundtrip() {
        let result = eval_fn(
            r#"fn roundtrip { Clock Unit -> Bool
    in: _
    out: do {
        let base = Clock.now()
        let offset = Duration.seconds(10)
        let later = Timestamp.add(base, offset)
        let diff = Timestamp.sub(later, base)
        match diff {
            _ -> true
        }
    }
}"#,
            "roundtrip",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
    }

    #[test]
    fn test_timestamp_compare() {
        let result = eval_fn(
            r#"fn cmpTs { Clock Unit -> Bool
    in: _
    out: do {
        let t1 = Clock.now()
        let t2 = Timestamp.add(t1, Duration.ms(1000))
        match Timestamp.compare(t1, t2) {
            LessThan    -> true
            Equal       -> false
            GreaterThan -> false
        }
    }
}"#,
            "cmpTs",
            Value::Unit,
        );
        assert_eq!(result, Ok(Value::Bool(true)));
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
