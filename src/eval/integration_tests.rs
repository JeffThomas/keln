#[cfg(test)]
mod integration {
    use crate::eval::{eval_fn, Value, VariantPayload};

    // =========================================================================
    // Helpers
    // =========================================================================

    fn ok(v: Value) -> Value {
        Value::Variant { name: "Ok".to_string(), payload: VariantPayload::Tuple(Box::new(v)) }
    }
    fn some(v: Value) -> Value {
        Value::Variant { name: "Some".to_string(), payload: VariantPayload::Tuple(Box::new(v)) }
    }
    fn none() -> Value {
        Value::Variant { name: "None".to_string(), payload: VariantPayload::Unit }
    }
    fn unit_variant(name: &str) -> Value {
        Value::Variant { name: name.to_string(), payload: VariantPayload::Unit }
    }
    fn rec(fields: Vec<(&str, Value)>) -> Value {
        Value::make_record_from_pairs(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    // =========================================================================
    // 1. Fibonacci — recursive function, integer literal patterns
    // =========================================================================

    const FIB_SRC: &str = r#"
fn fib {
    Pure Int -> Int
    in: n
    out: match n {
        0 -> 0
        1 -> 1
        _ -> fib(n - 1) + fib(n - 2)
    }
    verify: {
        given(0) -> 0
        given(1) -> 1
        given(7) -> 13
        forall(n: Int where 1..8) -> fib(n) >= 1
    }
}
"#;

    #[test]
    fn test_fib_base_cases() {
        assert_eq!(eval_fn(FIB_SRC, "fib", Value::Int(0)), Ok(Value::Int(0)));
        assert_eq!(eval_fn(FIB_SRC, "fib", Value::Int(1)), Ok(Value::Int(1)));
    }

    #[test]
    fn test_fib_recursive() {
        assert_eq!(eval_fn(FIB_SRC, "fib", Value::Int(7)), Ok(Value::Int(13)));
        assert_eq!(eval_fn(FIB_SRC, "fib", Value::Int(10)), Ok(Value::Int(55)));
    }

    // =========================================================================
    // 2. parsePort — spec canonical example, Result + record variant
    // =========================================================================

    const PARSE_PORT_SRC: &str = r#"
type PortError =
    | OutOfRange { value: Int }
    | NotANumber { input: String }

fn inRange {
    Pure Int -> Bool
    in: n
    out: Bool.and(n >= 1, n <= 65535)
}

fn parsePort {
    Pure String -> Result<Int, PortError>
    in: s
    out: match Int.parse(s) {
        Ok(n) -> match inRange(n) {
            true  -> Result.ok(n)
            false -> Result.err(OutOfRange { value: n })
        }
        Err(_) -> Result.err(NotANumber { input: s })
    }
    verify: {
        given("8080")  -> Ok(8080)
        given("0")     -> Err(OutOfRange { value: 0 })
        given("65535") -> Ok(65535)
        given("abc")   -> Err(NotANumber { input: "abc" })
    }
}
"#;

    #[test]
    fn test_parse_port_valid() {
        assert_eq!(eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("8080".into())), Ok(ok(Value::Int(8080))));
        assert_eq!(eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("65535".into())), Ok(ok(Value::Int(65535))));
        assert_eq!(eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("1".into())), Ok(ok(Value::Int(1))));
    }

    #[test]
    fn test_parse_port_out_of_range() {
        let result = eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("0".into()));
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
        let result2 = eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("65536".into()));
        assert!(matches!(result2, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    #[test]
    fn test_parse_port_not_a_number() {
        let result = eval_fn(PARSE_PORT_SRC, "parsePort", Value::Str("abc".into()));
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    // =========================================================================
    // 3. Traffic light state machine — unit variants, exhaustive match
    // =========================================================================

    const TRAFFIC_SRC: &str = r#"
type TrafficLight = Red | Yellow | Green

fn nextLight {
    Pure TrafficLight -> TrafficLight
    in: light
    out: match light {
        Red    -> Green
        Yellow -> Red
        Green  -> Yellow
    }
    verify: {
        given(Red)    -> Green
        given(Yellow) -> Red
        given(Green)  -> Yellow
    }
}

fn cycleN {
    Pure { light: TrafficLight, n: Int } -> TrafficLight
    in: ctx
    out: match ctx.n {
        0 -> ctx.light
        _ -> cycleN({ light: nextLight(ctx.light), n: ctx.n - 1 })
    }
}
"#;

    #[test]
    fn test_traffic_light_transitions() {
        assert_eq!(
            eval_fn(TRAFFIC_SRC, "nextLight", unit_variant("Red")),
            Ok(unit_variant("Green"))
        );
        assert_eq!(
            eval_fn(TRAFFIC_SRC, "nextLight", unit_variant("Yellow")),
            Ok(unit_variant("Red"))
        );
        assert_eq!(
            eval_fn(TRAFFIC_SRC, "nextLight", unit_variant("Green")),
            Ok(unit_variant("Yellow"))
        );
    }

    #[test]
    fn test_traffic_light_cycle() {
        let input = rec(vec![("light", unit_variant("Red")), ("n", Value::Int(3))]);
        assert_eq!(
            eval_fn(TRAFFIC_SRC, "cycleN", input),
            Ok(unit_variant("Red"))
        );
    }

    // =========================================================================
    // 4. Safe arithmetic — Result chaining in do blocks
    // =========================================================================

    const SAFE_MATH_SRC: &str = r#"
fn safeDivide {
    Pure { a: Int, b: Int } -> Result<Int, String>
    in: ctx
    out: match ctx.b == 0 {
        true  -> Result.err("division by zero")
        false -> Result.ok(ctx.a / ctx.b)
    }
}

fn safeNested {
    Pure { a: Int, b: Int, c: Int } -> Result<Int, String>
    in: ctx
    out: do {
        let ab = safeDivide({ a: ctx.a, b: ctx.b })
        let result = match ab {
            Ok(n)  -> safeDivide({ a: n, b: ctx.c })
            Err(e) -> Err(e)
        }
        result
    }
}
"#;

    #[test]
    fn test_safe_divide_ok() {
        assert_eq!(
            eval_fn(SAFE_MATH_SRC, "safeDivide", rec(vec![("a", Value::Int(10)), ("b", Value::Int(2))])),
            Ok(ok(Value::Int(5)))
        );
    }

    #[test]
    fn test_safe_divide_by_zero() {
        let result = eval_fn(SAFE_MATH_SRC, "safeDivide", rec(vec![("a", Value::Int(10)), ("b", Value::Int(0))]));
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    #[test]
    fn test_safe_nested_ok() {
        let input = rec(vec![("a", Value::Int(100)), ("b", Value::Int(5)), ("c", Value::Int(4))]);
        assert_eq!(eval_fn(SAFE_MATH_SRC, "safeNested", input), Ok(ok(Value::Int(5))));
    }

    #[test]
    fn test_safe_nested_first_err_propagates() {
        let input = rec(vec![("a", Value::Int(100)), ("b", Value::Int(0)), ("c", Value::Int(4))]);
        let result = eval_fn(SAFE_MATH_SRC, "safeNested", input);
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    // =========================================================================
    // 5. Helper functions — compact helper block
    // =========================================================================

    const HELPERS_SRC: &str = r#"
fn processInts {
    Pure { a: Int, b: Int } -> Int
    in: ctx
    out: doubled(ctx.a) + tripled(ctx.b)
    helpers: {
        doubled :: Pure Int -> Int => it * 2
        tripled :: Pure Int -> Int => it * 3
    }
}
"#;

    #[test]
    fn test_compact_helpers() {
        let input = rec(vec![("a", Value::Int(4)), ("b", Value::Int(3))]);
        assert_eq!(eval_fn(HELPERS_SRC, "processInts", input), Ok(Value::Int(17)));
    }

    // =========================================================================
    // 6. List fold — user-defined accumulator, multi-function program
    // =========================================================================

    const LIST_FOLD_SRC: &str = r#"
fn addPair {
    Pure { acc: Int, item: Int } -> Int
    in: ctx
    out: ctx.acc + ctx.item
}

fn sumList {
    Pure List<Int> -> Int
    in: xs
    out: List.fold(xs, 0, addPair)
}

fn productList {
    Pure List<Int> -> Int
    in: xs
    out: List.fold(xs, 1, mulPair)
    helpers: {
        mulPair :: Pure { acc: Int, item: Int } -> Int => it.acc * it.item
    }
}
"#;

    #[test]
    fn test_sum_list() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]));
        assert_eq!(eval_fn(LIST_FOLD_SRC, "sumList", xs), Ok(Value::Int(10)));
    }

    #[test]
    fn test_sum_list_empty() {
        assert_eq!(eval_fn(LIST_FOLD_SRC, "sumList", Value::List(std::rc::Rc::new(vec![]))), Ok(Value::Int(0)));
    }

    #[test]
    fn test_product_list() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(2), Value::Int(3), Value::Int(4)]));
        assert_eq!(eval_fn(LIST_FOLD_SRC, "productList", xs), Ok(Value::Int(24)));
    }

    // =========================================================================
    // 7. Pipeline |> expressions
    // =========================================================================

    const PIPELINE_SRC: &str = r#"
fn double {
    Pure Int -> Int
    in: n
    out: n * 2
}

fn addOne {
    Pure Int -> Int
    in: n
    out: n + 1
}

fn pipeline {
    Pure Int -> Int
    in: n
    out: n |> double |> addOne |> double
}
"#;

    #[test]
    fn test_pipeline() {
        assert_eq!(eval_fn(PIPELINE_SRC, "pipeline", Value::Int(3)), Ok(Value::Int(14)));
    }

    // =========================================================================
    // 8. String manipulation pipeline
    // =========================================================================

    const STRING_SRC: &str = r#"
fn normalize {
    Pure String -> String
    in: s
    out: do {
        let trimmed = String.trim(s)
        let lower = String.toLower(trimmed)
        lower
    }
}

fn greet {
    Pure String -> String
    in: name
    out: do {
        let n = normalize(name)
        let greeting = String.concat("hello, ", n)
        greeting
    }
}
"#;

    #[test]
    fn test_string_normalize() {
        assert_eq!(
            eval_fn(STRING_SRC, "normalize", Value::Str("  Hello World  ".into())),
            Ok(Value::Str("hello world".into()))
        );
    }

    #[test]
    fn test_string_greet() {
        assert_eq!(
            eval_fn(STRING_SRC, "greet", Value::Str(" Alice ".into())),
            Ok(Value::Str("hello, alice".into()))
        );
    }

    // =========================================================================
    // 9. Maybe chaining — require, getOr, nested Maybe
    // =========================================================================

    const MAYBE_SRC: &str = r#"
fn firstPositive {
    Pure List<Int> -> Maybe<Int>
    in: xs
    out: do {
        let h = List.head(xs)
        match h {
            Some(n) -> match n > 0 {
                true  -> Some(n)
                false -> firstPositive(List.tail(xs))
            }
            None -> None
        }
    }
}

fn firstPositiveOr {
    Pure { xs: List<Int>, default: Int } -> Int
    in: ctx
    out: Maybe.getOr(firstPositive(ctx.xs), ctx.default)
}
"#;

    #[test]
    fn test_first_positive_found() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(0), Value::Int(0), Value::Int(5)]));
        assert_eq!(eval_fn(MAYBE_SRC, "firstPositive", xs), Ok(some(Value::Int(5))));
    }

    #[test]
    fn test_first_positive_none() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(0), Value::Int(0)]));
        assert_eq!(eval_fn(MAYBE_SRC, "firstPositive", xs), Ok(none()));
    }

    #[test]
    fn test_first_positive_or_default() {
        let input = rec(vec![
            ("xs", Value::List(std::rc::Rc::new(vec![Value::Int(0)]))),
            ("default", Value::Int(42)),
        ]);
        assert_eq!(eval_fn(MAYBE_SRC, "firstPositiveOr", input), Ok(Value::Int(42)));
    }

    // =========================================================================
    // 10. Map operations in a real program — frequency counter
    // =========================================================================

    const MAP_SRC: &str = r#"
fn countItem {
    Pure { counts: Map<String, Int>, item: String } -> Map<String, Int>
    in: ctx
    out: do {
        let current = Maybe.getOr(Map.get(ctx.counts, ctx.item), 0)
        Map.insert(ctx.counts, ctx.item, current + 1)
    }
}

fn countAll {
    Pure { counts: Map<String, Int>, items: List<String> } -> Map<String, Int>
    in: ctx
    out: match List.isEmpty(ctx.items) {
        true  -> ctx.counts
        false -> do {
            let h = Maybe.getOr(List.head(ctx.items), "")
            let rest = List.tail(ctx.items)
            let updated = countItem({ counts: ctx.counts, item: h })
            countAll({ counts: updated, items: rest })
        }
    }
}
"#;

    #[test]
    fn test_frequency_counter() {
        let src = MAP_SRC;
        let items = Value::List(std::rc::Rc::new(vec![
            Value::Str("a".into()), Value::Str("b".into()),
            Value::Str("a".into()), Value::Str("c".into()),
            Value::Str("a".into()),
        ]));
        let input = rec(vec![("counts", Value::Map(std::rc::Rc::new(std::collections::BTreeMap::new()))), ("items", items)]);
        let result = eval_fn(src, "countAll", input).unwrap();
        if let Value::Map(entries) = &result {
            let get = |k: &str| entries.get(&Value::Str(k.into())).cloned();
            assert_eq!(get("a"), Some(Value::Int(3)));
            assert_eq!(get("b"), Some(Value::Int(1)));
            assert_eq!(get("c"), Some(Value::Int(1)));
        } else {
            panic!("expected Map, got {:?}", result);
        }
    }

    // =========================================================================
    // 11. Tail-recursive loop — TCO / Never return type
    // =========================================================================

    const COUNTDOWN_SRC: &str = r#"
fn countdown {
    Pure { n: Int, acc: Int } -> Int
    in: ctx
    out: match ctx.n {
        0 -> ctx.acc
        _ -> countdown({ n: ctx.n - 1, acc: ctx.acc + ctx.n })
    }
}
"#;

    #[test]
    fn test_countdown_sum_100() {
        let input = rec(vec![("n", Value::Int(100)), ("acc", Value::Int(0))]);
        assert_eq!(eval_fn(COUNTDOWN_SRC, "countdown", input), Ok(Value::Int(5050)));
    }

    #[test]
    fn test_countdown_sum_1000() {
        let input = rec(vec![("n", Value::Int(1000)), ("acc", Value::Int(0))]);
        assert_eq!(eval_fn(COUNTDOWN_SRC, "countdown", input), Ok(Value::Int(500500)));
    }

    // =========================================================================
    // 12. Clone operation — clone on list + mutation test
    // =========================================================================

    const CLONE_SRC: &str = r#"
fn appendToClone {
    Pure { xs: List<Int>, item: Int } -> List<Int>
    in: ctx
    out: do {
        let copy = clone(ctx.xs)
        List.append(copy, ctx.item)
    }
}
"#;

    #[test]
    fn test_clone_list() {
        let input = rec(vec![
            ("xs", Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2)]))),
            ("item", Value::Int(3)),
        ]);
        assert_eq!(
            eval_fn(CLONE_SRC, "appendToClone", input),
            Ok(Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])))
        );
    }

    // =========================================================================
    // 13. Multi-function cross-call — verify correctness of dispatch
    // =========================================================================

    const MULTI_FN_SRC: &str = r#"
fn isEven {
    Pure Int -> Bool
    in: n
    out: (n % 2) == 0
}

fn classify {
    Pure Int -> String
    in: n
    out: match isEven(n) {
        true  -> "even"
        false -> "odd"
    }
}

fn classifyList {
    Pure List<Int> -> List<String>
    in: xs
    out: List.map(xs, classify)
}
"#;

    #[test]
    fn test_classify() {
        assert_eq!(eval_fn(MULTI_FN_SRC, "classify", Value::Int(4)), Ok(Value::Str("even".into())));
        assert_eq!(eval_fn(MULTI_FN_SRC, "classify", Value::Int(7)), Ok(Value::Str("odd".into())));
    }

    #[test]
    fn test_classify_list() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]));
        assert_eq!(
            eval_fn(MULTI_FN_SRC, "classifyList", xs),
            Ok(Value::List(std::rc::Rc::new(vec![
                Value::Str("odd".into()), Value::Str("even".into()),
                Value::Str("odd".into()), Value::Str("even".into()),
            ])))
        );
    }

    // =========================================================================
    // 14. Env + Json pipeline — read config, parse, extract field
    // =========================================================================

    const ENV_JSON_SRC: &str = r#"
fn configOrDefault {
    Pure { key: String, default: String } -> String
    in: ctx
    out: Maybe.getOr(Env.get(ctx.key), ctx.default)
}

fn parseJsonInt {
    Pure String -> Result<Int, String>
    in: s
    out: match Json.parse(s) {
        Ok(v)  -> match v == 42 {
            true  -> Result.ok(42)
            false -> Result.err("not 42")
        }
        Err(_) -> Result.err("parse failed")
    }
}
"#;

    #[test]
    fn test_config_or_default() {
        let input = rec(vec![
            ("key", Value::Str("KELN_TEST_NONEXISTENT_XYZ".into())),
            ("default", Value::Str("fallback".into())),
        ]);
        assert_eq!(eval_fn(ENV_JSON_SRC, "configOrDefault", input), Ok(Value::Str("fallback".into())));
    }

    #[test]
    fn test_parse_json_int_ok() {
        assert_eq!(eval_fn(ENV_JSON_SRC, "parseJsonInt", Value::Str("42".into())), Ok(ok(Value::Int(42))));
    }

    #[test]
    fn test_parse_json_int_wrong_value() {
        let result = eval_fn(ENV_JSON_SRC, "parseJsonInt", Value::Str("99".into()));
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    #[test]
    fn test_parse_json_invalid() {
        let result = eval_fn(ENV_JSON_SRC, "parseJsonInt", Value::Str("{not json}".into()));
        assert!(matches!(result, Ok(Value::Variant { name, .. }) if name == "Err"));
    }

    // =========================================================================
    // 15. Verify integration — run verify blocks programmatically
    // =========================================================================

    #[test]
    fn test_verify_fib() {
        use crate::verify::VerifyExecutor;
        let mut ex = VerifyExecutor::from_source(FIB_SRC).unwrap();
        let results = ex.verify_all();
        assert!(!results.is_empty(), "fib should have a verify block");
        for r in &results {
            assert!(r.is_clean(), "fib verify failed: {:?}", r);
        }
    }

    #[test]
    fn test_verify_parse_port() {
        use crate::verify::VerifyExecutor;
        let mut ex = VerifyExecutor::from_source(PARSE_PORT_SRC).unwrap();
        let results = ex.verify_all();
        assert!(!results.is_empty());
        for r in &results {
            assert!(r.is_clean(), "parsePort verify failed: {:?}", r);
        }
    }

    #[test]
    fn test_verify_traffic_light() {
        use crate::verify::VerifyExecutor;
        let mut ex = VerifyExecutor::from_source(TRAFFIC_SRC).unwrap();
        let results = ex.verify_all();
        assert!(!results.is_empty());
        for r in &results {
            assert!(r.is_clean(), "traffic light verify failed: {:?}", r);
        }
    }

    // =========================================================================
    // Regression 1: Record pattern field values must be checked recursively.
    // Before the fix, Pattern::Record only checked the shape of the value, not
    // the actual field sub-patterns. All arms matched the same record type so
    // only the first arm ever fired — every input produced the same result.
    // =========================================================================

    const RECORD_PATTERN_SRC: &str = r#"
fn classify {
    Pure Int -> String
    in: n
    out: match { by3: n % 3 == 0, by5: n % 5 == 0 } {
        { by3: true,  by5: true  } -> "FizzBuzz"
        { by3: true,  by5: false } -> "Fizz"
        { by3: false, by5: true  } -> "Buzz"
        { by3: false, by5: false } -> "Number"
    }
}
"#;

    #[test]
    fn test_record_pattern_fizzbuzz() {
        assert_eq!(eval_fn(RECORD_PATTERN_SRC, "classify", Value::Int(15)), Ok(Value::Str("FizzBuzz".into())));
    }

    #[test]
    fn test_record_pattern_fizz() {
        assert_eq!(eval_fn(RECORD_PATTERN_SRC, "classify", Value::Int(3)), Ok(Value::Str("Fizz".into())));
    }

    #[test]
    fn test_record_pattern_buzz() {
        assert_eq!(eval_fn(RECORD_PATTERN_SRC, "classify", Value::Int(5)), Ok(Value::Str("Buzz".into())));
    }

    #[test]
    fn test_record_pattern_number() {
        assert_eq!(eval_fn(RECORD_PATTERN_SRC, "classify", Value::Int(7)), Ok(Value::Str("Number".into())));
    }

    // =========================================================================
    // Regression 2: Parser UpperVar { } ambiguity in match arm bodies.
    // Before the fix, after parsing a unit variant as a match arm body (e.g.
    // `FizzBuzz`), the parser greedily consumed the next arm's opening `{` as
    // the start of a named record constructor, producing a parse error.
    // =========================================================================

    const UNIT_VARIANT_THEN_RECORD_PATTERN_SRC: &str = r#"
type Result2 = FizzBuzz | Fizz | Buzz | Number

fn classify2 {
    Pure { by3: Bool, by5: Bool } -> Result2
    in: ctx
    out: match ctx {
        { by3: true,  by5: true  } -> FizzBuzz
        { by3: true,  by5: false } -> Fizz
        { by3: false, by5: true  } -> Buzz
        { by3: false, by5: false } -> Number
    }
}
"#;

    #[test]
    fn test_unit_variant_then_record_pattern_parses() {
        let input = rec(vec![("by3", Value::Bool(true)), ("by5", Value::Bool(true))]);
        assert_eq!(
            eval_fn(UNIT_VARIANT_THEN_RECORD_PATTERN_SRC, "classify2", input),
            Ok(unit_variant("FizzBuzz"))
        );
    }

    #[test]
    fn test_unit_variant_then_record_pattern_second_arm() {
        let input = rec(vec![("by3", Value::Bool(true)), ("by5", Value::Bool(false))]);
        assert_eq!(
            eval_fn(UNIT_VARIANT_THEN_RECORD_PATTERN_SRC, "classify2", input),
            Ok(unit_variant("Fizz"))
        );
    }

    // =========================================================================
    // Negative integer literals in expressions and match patterns
    // =========================================================================

    const NEG_LITERAL_SRC: &str = r#"
fn sign {
    Pure Int -> Int
    in: n
    out: match n {
        -1 -> -10
        0  -> 0
        _  -> 1
    }
}

fn negate_const {
    Pure Int -> Int
    in: n
    out: n + -5
}
"#;

    #[test]
    fn test_negative_literal_in_pattern() {
        assert_eq!(eval_fn(NEG_LITERAL_SRC, "sign", Value::Int(-1)), Ok(Value::Int(-10)));
        assert_eq!(eval_fn(NEG_LITERAL_SRC, "sign", Value::Int(0)),  Ok(Value::Int(0)));
        assert_eq!(eval_fn(NEG_LITERAL_SRC, "sign", Value::Int(5)),  Ok(Value::Int(1)));
    }

    #[test]
    fn test_negative_literal_in_expression() {
        assert_eq!(eval_fn(NEG_LITERAL_SRC, "negate_const", Value::Int(10)), Ok(Value::Int(5)));
        assert_eq!(eval_fn(NEG_LITERAL_SRC, "negate_const", Value::Int(0)),  Ok(Value::Int(-5)));
    }

    // =========================================================================
    // Named capturing helpers (closures)
    // =========================================================================

    const CAPTURING_HELPER_SRC: &str = r#"
fn sum_with_offset {
    Pure { items: List<Int>, offset: Int } -> Int
    in: args
    out:
        let offset = args.offset in
        let addOffset :: Pure { acc: Int, item: Int } -> Int =>
            it.acc + it.item + offset
        in
        List.fold(args.items, 0, addOffset)
}
"#;

    #[test]
    fn test_capturing_helper_basic() {
        let arg = Value::make_record(&["items", "offset"], vec![
            Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
            Value::Int(10),
        ]);
        assert_eq!(
            eval_fn(CAPTURING_HELPER_SRC, "sum_with_offset", arg),
            Ok(Value::Int(36)) // (1+10) + (2+10) + (3+10) = 36
        );
    }

    const NESTED_CLOSURE_SRC: &str = r#"
fn count_above {
    Pure { items: List<Int>, cutoff: Int } -> Int
    in: args
    out:
        let cutoff = args.cutoff in
        let countStep :: Pure { acc: Int, item: Int } -> Int =>
            match it.item > cutoff {
                true  -> it.acc + 1
                false -> it.acc
            }
        in
        List.fold(args.items, 0, countStep)
}
"#;

    #[test]
    fn test_capturing_helper_with_match() {
        let arg = Value::make_record(&["items", "cutoff"], vec![
            Value::List(std::rc::Rc::new(vec![
                Value::Int(1), Value::Int(5), Value::Int(3), Value::Int(8), Value::Int(2),
            ])),
            Value::Int(3),
        ]);
        assert_eq!(
            eval_fn(NESTED_CLOSURE_SRC, "count_above", arg),
            Ok(Value::Int(2)) // 5 and 8 are above 3
        );
    }

    // =========================================================================
    // Map.empty / Set.empty in value position (not call position)
    // =========================================================================

    const MAP_EMPTY_VALUE_SRC: &str = r#"
fn make_empty_map {
    Pure Unit -> Int
    in: _
    out:
        let m = Map.empty in
        let m2 = Map.insert(m, "a", 1) in
        Map.size(m2)
}

fn empty_map_in_record {
    Pure Unit -> Int
    in: _
    out:
        let rec = { counts: Map.empty } in
        let m2 = Map.insert(rec.counts, "x", 42) in
        Map.size(m2)
}
"#;

    #[test]
    fn test_map_empty_in_let_binding() {
        assert_eq!(eval_fn(MAP_EMPTY_VALUE_SRC, "make_empty_map", Value::Unit), Ok(Value::Int(1)));
    }

    #[test]
    fn test_map_empty_in_record_field() {
        assert_eq!(eval_fn(MAP_EMPTY_VALUE_SRC, "empty_map_in_record", Value::Unit), Ok(Value::Int(1)));
    }

    // =========================================================================
    // Type aliases — `type Frac = {num: Int, den: Int}` field access
    // =========================================================================

    const TYPE_ALIAS_SRC: &str = r#"
type Frac = { num: Int, den: Int }

fn make_frac {
    Pure { n: Int, d: Int } -> Int
    in: args
    out:
        let f = { num: args.n, den: args.d } in
        f.num + f.den
}
"#;

    #[test]
    fn test_type_alias_field_access() {
        let arg = Value::make_record(&["n", "d"], vec![Value::Int(3), Value::Int(4)]);
        assert_eq!(eval_fn(TYPE_ALIAS_SRC, "make_frac", arg), Ok(Value::Int(7)));
    }

    // =========================================================================
    // Naming error messages — uppercase identifier gives helpful suggestion
    // =========================================================================

    #[test]
    fn test_naming_error_uppercase_suggests_lowercase() {
        // Function name is uppercase — expect_lower_ident fires on the fn name
        let src = r#"fn MyFunc { Pure Int -> Int in: n out: n }"#;
        let result = eval_fn(src, "MyFunc", Value::Int(1));
        let err = result.unwrap_err();
        assert!(err.contains("must be lower_snake_case"), "expected helpful error, got: {}", err);
        assert!(err.contains("did you mean"), "expected 'did you mean' suggestion, got: {}", err);
        assert!(err.contains("my_func"), "expected suggestion 'my_func', got: {}", err);
    }

    #[test]
    fn test_naming_error_reserved_keyword() {
        // Function name is a reserved keyword — expect_lower_ident fires on the fn name
        let src = r#"fn match { Pure Int -> Int in: n out: n }"#;
        let result = eval_fn(src, "match", Value::Int(1));
        let err = result.unwrap_err();
        assert!(err.contains("reserved keyword"), "expected reserved keyword error, got: {}", err);
    }

    // =========================================================================
    // Record .with() — field update on plain record values
    // =========================================================================

    const RECORD_WITH_SRC: &str = r#"
fn update_count {
    Pure { count: Int, label: String } -> Int
    in: args
    out:
        let updated = args.with(count: args.count + 1) in
        updated.count
}

fn update_multi {
    Pure { x: Int, y: Int, z: Int } -> Int
    in: args
    out:
        let moved = args.with({ x: 10, y: 20 }) in
        moved.x + moved.y + moved.z
}

fn with_in_fold {
    Pure { items: List<Int> } -> Int
    in: args
    out:
        let init = { acc: 0, seen: 0 } in
        let step :: Pure { acc: { acc: Int, seen: Int }, item: Int } -> { acc: Int, seen: Int } =>
            it.acc.with({ acc: it.acc.acc + it.item, seen: it.acc.seen + 1 })
        in
        let result = List.fold(args.items, init, step) in
        result.acc
}
"#;

    #[test]
    fn test_record_with_single_field() {
        let arg = Value::make_record(&["count", "label"], vec![Value::Int(5), Value::Str("x".to_string())]);
        assert_eq!(eval_fn(RECORD_WITH_SRC, "update_count", arg), Ok(Value::Int(6)));
    }

    #[test]
    fn test_record_with_multi_field() {
        let arg = Value::make_record(&["x", "y", "z"], vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(eval_fn(RECORD_WITH_SRC, "update_multi", arg), Ok(Value::Int(33)));
    }

    #[test]
    fn test_record_with_in_fold() {
        let arg = Value::make_record(&["items"], vec![
            Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        ]);
        assert_eq!(eval_fn(RECORD_WITH_SRC, "with_in_fold", arg), Ok(Value::Int(6)));
    }

    // =========================================================================
    // List.findMap — first successful transformation or None
    // =========================================================================

    const FIND_MAP_SRC: &str = r#"
fn doubleFirstEven {
    Pure List<Int> -> Maybe<Int>
    in: xs
    out: List.findMap(xs, tryDouble)
    helpers: {
        tryDouble :: Pure Int -> Maybe<Int> =>
            match it % 2 == 0 {
                true  -> Some(it * 2)
                false -> None
            }
    }
}

fn findAboveLimit {
    Pure { xs: List<Int>, limit: Int } -> Maybe<Int>
    in: args
    out:
        let lim = args.limit in
        let tryAbove :: Pure Int -> Maybe<Int> =>
            match it > lim {
                true  -> Some(it)
                false -> None
            }
        in
        List.findMap(args.xs, tryAbove)
}
"#;

    #[test]
    fn test_find_map_finds_first() {
        let xs = Value::List(std::rc::Rc::new(vec![
            Value::Int(1), Value::Int(3), Value::Int(4), Value::Int(6),
        ]));
        assert_eq!(eval_fn(FIND_MAP_SRC, "doubleFirstEven", xs), Ok(some(Value::Int(8))));
    }

    #[test]
    fn test_find_map_returns_none() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(3), Value::Int(5)]));
        assert_eq!(eval_fn(FIND_MAP_SRC, "doubleFirstEven", xs), Ok(none()));
    }

    #[test]
    fn test_find_map_empty_list() {
        let xs = Value::List(std::rc::Rc::new(vec![]));
        assert_eq!(eval_fn(FIND_MAP_SRC, "doubleFirstEven", xs), Ok(none()));
    }

    #[test]
    fn test_find_map_closure_captures_context() {
        let input = Value::make_record(&["limit", "xs"], vec![
            Value::Int(10),
            Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(5), Value::Int(12), Value::Int(20)])),
        ]);
        assert_eq!(eval_fn(FIND_MAP_SRC, "findAboveLimit", input), Ok(some(Value::Int(12))));
    }

    // =========================================================================
    // Map.foldUntil — early-exit map fold
    // =========================================================================

    const MAP_FOLD_UNTIL_SRC: &str = r#"
fn sumMapUntil {
    Pure { m: Map<String, Int>, limit: Int } -> Int
    in: args
    out: Map.foldUntil(args.m, 0, addVal, isDone)
    helpers: {
        addVal :: Pure { acc: Int, key: String, value: Int } -> Int =>
            it.acc + it.value
        isDone :: Pure Int -> Bool =>
            it > args.limit
    }
}

fn sumMapFull {
    Pure Map<String, Int> -> Int
    in: m
    out: Map.foldUntil(m, 0, addVal, neverStop)
    helpers: {
        addVal :: Pure { acc: Int, key: String, value: Int } -> Int =>
            it.acc + it.value
        neverStop :: Pure Int -> Bool =>
            false
    }
}
"#;

    #[test]
    fn test_map_fold_until_stops_early() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(Value::Str("a".into()), Value::Int(3));
        map.insert(Value::Str("b".into()), Value::Int(5));
        map.insert(Value::Str("c".into()), Value::Int(7));
        map.insert(Value::Str("d".into()), Value::Int(9));
        let input = Value::make_record(&["m", "limit"], vec![
            Value::Map(std::rc::Rc::new(map)),
            Value::Int(10),
        ]);
        // a=3, b=5 → 8 (continue), c=7 → 15 > 10 (stop)
        assert_eq!(eval_fn(MAP_FOLD_UNTIL_SRC, "sumMapUntil", input), Ok(Value::Int(15)));
    }

    #[test]
    fn test_map_fold_until_full_when_stop_never_fires() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(Value::Str("a".into()), Value::Int(3));
        map.insert(Value::Str("b".into()), Value::Int(5));
        map.insert(Value::Str("c".into()), Value::Int(7));
        assert_eq!(
            eval_fn(MAP_FOLD_UNTIL_SRC, "sumMapFull", Value::Map(std::rc::Rc::new(map))),
            Ok(Value::Int(15))
        );
    }

    #[test]
    fn test_map_fold_until_empty_map() {
        let map = std::collections::BTreeMap::new();
        let input = Value::make_record(&["m", "limit"], vec![
            Value::Map(std::rc::Rc::new(map)),
            Value::Int(0),
        ]);
        assert_eq!(eval_fn(MAP_FOLD_UNTIL_SRC, "sumMapUntil", input), Ok(Value::Int(0)));
    }

    // =========================================================================
    // List.mapFold — running accumulator + output list in O(N)
    // =========================================================================

    const MAP_FOLD_SRC: &str = r#"
fn prefixSums {
    Pure List<Int> -> List<Int>
    in: xs
    out: List.mapFold(xs, 0, step).result
    helpers: {
        step :: Pure {acc: Int, item: Int} -> {acc: Int, val: Int} =>
            let s = it.acc + it.item in
            {acc: s, val: s}
    }
}

fn enumerate {
    Pure List<String> -> List<{i: Int, val: String}>
    in: xs
    out: List.mapFold(xs, 0, tag).result
    helpers: {
        tag :: Pure {acc: Int, item: String} -> {acc: Int, val: {i: Int, v: String}} =>
            {acc: it.acc + 1, val: {i: it.acc, v: it.item}}
    }
}

fn finalAcc {
    Pure List<Int> -> Int
    in: xs
    out: List.mapFold(xs, 0, step).acc
    helpers: {
        step :: Pure {acc: Int, item: Int} -> {acc: Int, val: Int} =>
            let s = it.acc + it.item in
            {acc: s, val: s}
    }
}
"#;

    #[test]
    fn test_map_fold_prefix_sums() {
        let xs = Value::List(std::rc::Rc::new(vec![
            Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4),
        ]));
        let expected = Value::List(std::rc::Rc::new(vec![
            Value::Int(1), Value::Int(3), Value::Int(6), Value::Int(10),
        ]));
        assert_eq!(eval_fn(MAP_FOLD_SRC, "prefixSums", xs), Ok(expected));
    }

    #[test]
    fn test_map_fold_empty() {
        let xs = Value::List(std::rc::Rc::new(vec![]));
        let expected = Value::List(std::rc::Rc::new(vec![]));
        assert_eq!(eval_fn(MAP_FOLD_SRC, "prefixSums", xs), Ok(expected));
    }

    #[test]
    fn test_map_fold_final_acc() {
        let xs = Value::List(std::rc::Rc::new(vec![Value::Int(10), Value::Int(20), Value::Int(30)]));
        assert_eq!(eval_fn(MAP_FOLD_SRC, "finalAcc", xs), Ok(Value::Int(60)));
    }

    #[test]
    fn test_map_fold_enumerate() {
        let xs = Value::List(std::rc::Rc::new(vec![
            Value::Str("a".into()), Value::Str("b".into()),
        ]));
        let result = eval_fn(MAP_FOLD_SRC, "enumerate", xs).unwrap();
        if let Value::List(items) = result {
            assert_eq!(items.len(), 2);
            if let Value::Record(_, ref f0) = items[0] { assert_eq!(f0[0], Value::Int(0)); }
            if let Value::Record(_, ref f1) = items[1] { assert_eq!(f1[0], Value::Int(1)); }
        } else {
            panic!("expected List");
        }
    }

    // =========================================================================
    // Fix: helpers: functions visible from named capturing helpers (closures)
    // =========================================================================

    const HELPERS_VISIBLE_FROM_CLOSURE_SRC: &str = r#"
fn compute {
    Pure Int -> Int
    in: n
    out:
        let doubled :: Pure Int -> Int => double(it) in
        doubled(n)
    helpers: {
        double :: Pure Int -> Int => it * 2
    }
}
"#;

    #[test]
    fn test_helpers_visible_from_closure() {
        assert_eq!(eval_fn(HELPERS_VISIBLE_FROM_CLOSURE_SRC, "compute", Value::Int(5)), Ok(Value::Int(10)));
        assert_eq!(eval_fn(HELPERS_VISIBLE_FROM_CLOSURE_SRC, "compute", Value::Int(0)), Ok(Value::Int(0)));
    }

    // =========================================================================
    // Fix: let rec — recursive named capturing helpers
    // =========================================================================

    const LET_REC_SRC: &str = r#"
fn countdown {
    Pure Int -> Int
    in: n
    out:
        let rec loop :: Pure Int -> Int =>
            match it {
                0 -> 0
                n -> loop(n - 1)
            }
        in
        loop(n)
}

fn factorial {
    Pure Int -> Int
    in: n
    out:
        let rec fact :: Pure Int -> Int =>
            match it {
                0 -> 1
                n -> n * fact(n - 1)
            }
        in
        fact(n)
}
"#;

    #[test]
    fn test_let_rec_countdown() {
        assert_eq!(eval_fn(LET_REC_SRC, "countdown", Value::Int(10)), Ok(Value::Int(0)));
        assert_eq!(eval_fn(LET_REC_SRC, "countdown", Value::Int(0)),  Ok(Value::Int(0)));
    }

    #[test]
    fn test_let_rec_factorial() {
        assert_eq!(eval_fn(LET_REC_SRC, "factorial", Value::Int(0)), Ok(Value::Int(1)));
        assert_eq!(eval_fn(LET_REC_SRC, "factorial", Value::Int(5)), Ok(Value::Int(120)));
        assert_eq!(eval_fn(LET_REC_SRC, "factorial", Value::Int(7)), Ok(Value::Int(5040)));
    }

    // =========================================================================
    // Fix: and/or/not as boolean expression operators
    // =========================================================================

    const BOOL_OPS_SRC: &str = r#"
fn testNot {
    Pure Bool -> Bool
    in: b
    out: not(b)
}

fn testAnd {
    Pure { a: Bool, b: Bool } -> Bool
    in: args
    out: and(args.a, args.b)
}

fn testOr {
    Pure { a: Bool, b: Bool } -> Bool
    in: args
    out: or(args.a, args.b)
}
"#;

    fn bool_rec(a: bool, b: bool) -> Value {
        Value::make_record(&["a", "b"], vec![Value::Bool(a), Value::Bool(b)])
    }

    #[test]
    fn test_expr_not() {
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testNot", Value::Bool(true)),  Ok(Value::Bool(false)));
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testNot", Value::Bool(false)), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_expr_and() {
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testAnd", bool_rec(true,  true)),  Ok(Value::Bool(true)));
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testAnd", bool_rec(true,  false)), Ok(Value::Bool(false)));
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testAnd", bool_rec(false, true)),  Ok(Value::Bool(false)));
    }

    #[test]
    fn test_expr_or() {
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testOr", bool_rec(false, false)), Ok(Value::Bool(false)));
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testOr", bool_rec(false, true)),  Ok(Value::Bool(true)));
        assert_eq!(eval_fn(BOOL_OPS_SRC, "testOr", bool_rec(true,  false)), Ok(Value::Bool(true)));
    }

    // =========================================================================
    // Regression 3: Lexer InputString truncation at 1024 characters.
    // Before the fix, lexxor::InputString silently truncated source to 1024
    // chars. Programs longer than that would fail to parse or lose definitions.
    // =========================================================================

    #[test]
    fn test_lexer_long_source_not_truncated() {
        // Build a source file well over 1024 characters: 30 trivial functions
        // (~59 chars each => ~1770 chars total). func25 starts around byte 1475,
        // well past the 1024-char truncation point of the old InputString lexer.
        let mut src = String::new();
        for i in 0..30 {
            src.push_str(&format!(
                "fn func{i} {{\n    Pure Int -> Int\n    in: n\n    out: n + {i}\n}}\n\n"
            ));
        }
        assert!(src.len() > 1024, "test source must exceed 1024 chars (got {})", src.len());

        // func25 is defined ~1475 bytes in — invisible to the old truncating lexer.
        assert_eq!(eval_fn(&src, "func25", Value::Int(0)),  Ok(Value::Int(25)));
        assert_eq!(eval_fn(&src, "func25", Value::Int(10)), Ok(Value::Int(35)));
    }
}
