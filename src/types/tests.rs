#[cfg(test)]
mod tests {
    use crate::types::check_source;

    fn assert_no_errors(source: &str) {
        let errors = check_source(source).expect("should parse");
        assert!(errors.is_empty(), "expected no type errors, got: {:?}", errors);
    }

    fn assert_has_error(source: &str, substring: &str) {
        let errors = check_source(source).expect("should parse");
        assert!(
            errors.iter().any(|e| e.message.contains(substring)),
            "expected error containing '{}', got: {:?}", substring, errors
        );
    }

    // =========================================================================
    // Basic function type checking
    // =========================================================================

    #[test]
    fn test_simple_fn_types_ok() {
        assert_no_errors(r#"fn identity {
    Pure Int -> Int
    in: n
    out: n
}"#);
    }

    #[test]
    fn test_fn_wrong_output_type() {
        assert_has_error(r#"fn bad {
    Pure Int -> String
    in: n
    out: n
}"#, "type mismatch");
    }

    #[test]
    fn test_fn_arithmetic_output_ok() {
        assert_no_errors(r#"fn addOne {
    Pure Int -> Int
    in: n
    out: n + 1
}"#);
    }

    #[test]
    fn test_fn_arithmetic_type_mismatch() {
        assert_has_error(r#"fn bad {
    Pure Int -> String
    in: n
    out: n + 1
}"#, "type mismatch");
    }

    // =========================================================================
    // Boolean and comparison
    // =========================================================================

    #[test]
    fn test_comparison_produces_bool() {
        assert_no_errors(r#"fn isPositive {
    Pure Int -> Bool
    in: n
    out: n > 0
}"#);
    }

    // =========================================================================
    // Type declarations
    // =========================================================================

    #[test]
    fn test_sum_type_decl_ok() {
        assert_no_errors(r#"type Color = Red | Green | Blue
fn isRed {
    Pure Color -> Bool
    in: c
    out: match c {
        Red -> true
        Green -> false
        Blue -> false
    }
}"#);
    }

    #[test]
    fn test_match_arms_consistent_types() {
        assert_has_error(r#"type Color = Red | Green | Blue
fn bad {
    Pure Color -> Int
    in: c
    out: match c {
        Red -> 1
        Green -> "two"
        Blue -> 3
    }
}"#, "match arm");
    }

    // =========================================================================
    // List expressions
    // =========================================================================

    #[test]
    fn test_list_homogeneous_ok() {
        assert_no_errors(r#"fn makeList {
    Pure Unit -> List<Int>
    in: _
    out: [1, 2, 3]
}"#);
    }

    #[test]
    fn test_list_heterogeneous_error() {
        assert_has_error(r#"fn bad {
    Pure Unit -> List<Int>
    in: _
    out: [1, "two", 3]
}"#, "list element");
    }

    // =========================================================================
    // Do blocks
    // =========================================================================

    #[test]
    fn test_do_block_ok() {
        assert_no_errors(r#"fn example {
    Pure Int -> Int
    in: n
    out: do {
        let x = n + 1
        x + 2
    }
}"#);
    }

    // =========================================================================
    // Undefined variables
    // =========================================================================

    #[test]
    fn test_undefined_variable_error() {
        assert_has_error(r#"fn bad {
    Pure Int -> Int
    in: n
    out: x + 1
}"#, "undefined variable 'x'");
    }

    // =========================================================================
    // Effect checking
    // =========================================================================

    #[test]
    fn test_pure_fn_no_io_ok() {
        assert_no_errors(r#"fn pureAdd {
    Pure Int -> Int
    in: n
    out: n + 1
}"#);
    }

    #[test]
    fn test_select_requires_io() {
        // select in a Pure function should be an error
        // Note: this requires a syntactically valid select, which our parser handles
        assert_has_error(r#"fn bad {
    Pure Int -> Int
    in: n
    out: select {
        x = <-n -> x
    }
}"#, "select requires IO");
    }

    // =========================================================================
    // Field access
    // =========================================================================

    #[test]
    fn test_record_field_access_ok() {
        assert_no_errors(r#"type Point = { x: Int, y: Int }
fn getX {
    Pure Point -> Int
    in: p
    out: p.x
}"#);
    }

    #[test]
    fn test_sum_type_direct_field_access_error() {
        assert_has_error(r#"type Maybe = Some(Int) | None
fn bad {
    Pure Maybe -> Int
    in: m
    out: m.value
}"#, "cannot access field");
    }

    // =========================================================================
    // Module declarations
    // =========================================================================

    #[test]
    fn test_module_decl_and_usage() {
        assert_no_errors(r#"module Db {
    provides: {
        query: IO String -> String
    }
}"#);
    }

    // =========================================================================
    // Effect declarations
    // =========================================================================

    #[test]
    fn test_effect_decl() {
        assert_no_errors(r#"effect Logging {
    log: Log String -> Unit
}"#);
    }

    // =========================================================================
    // Multiple declarations
    // =========================================================================

    #[test]
    fn test_multiple_decls_cross_reference() {
        assert_no_errors(r#"type Color = Red | Green | Blue
fn identity {
    Pure Color -> Color
    in: c
    out: c
}
fn apply {
    Pure Color -> Color
    in: c
    out: identity(c)
}"#);
    }

    // =========================================================================
    // Pipeline type checking
    // =========================================================================

    #[test]
    fn test_pipeline_with_known_fns() {
        assert_no_errors(r#"fn trim {
    Pure String -> String
    in: s
    out: s
}
fn toUpper {
    Pure String -> String
    in: s
    out: s
}
fn process {
    Pure String -> String
    in: s
    out: s |> trim |> toUpper
}"#);
    }

    // =========================================================================
    // Verify block type checking
    // =========================================================================

    #[test]
    fn test_verify_given_types_ok() {
        assert_no_errors(r#"fn identity {
    Pure Int -> Int
    in: n
    out: n
    verify: {
        given(0) -> 0
        given(42) -> 42
    }
}"#);
    }

    // =========================================================================
    // Record construction
    // =========================================================================

    #[test]
    fn test_record_construction_ok() {
        assert_no_errors(r#"type Point = { x: Int, y: Int }
fn makeOrigin {
    Pure Unit -> Point
    in: _
    out: Point { x: 0, y: 0 }
}"#);
    }

    #[test]
    fn test_record_missing_field_error() {
        assert_has_error(r#"type Point = { x: Int, y: Int }
fn bad {
    Pure Unit -> Point
    in: _
    out: Point { x: 0 }
}"#, "missing field 'y'");
    }

    #[test]
    fn test_record_extra_field_error() {
        assert_has_error(r#"type Point = { x: Int, y: Int }
fn bad {
    Pure Unit -> Point
    in: _
    out: Point { x: 0, y: 0, z: 0 }
}"#, "unexpected field 'z'");
    }

    // =========================================================================
    // Fix 1 regression: EffectSet::from_names normalization
    // =========================================================================

    #[test]
    fn test_effect_set_pure_io_normalizes() {
        use crate::types::EffectSet;
        // from_names(&["Pure", "IO"]) should normalize to just {"IO"}
        let es = EffectSet::from_names(&["Pure".to_string(), "IO".to_string()]);
        assert!(!es.is_pure(), "Pure + IO should not be considered pure");
        assert!(es.effects.contains("IO"));
        assert!(!es.effects.contains("Pure"), "Pure should be dropped when IO is present");
    }

    // =========================================================================
    // Fix 2 regression: select/channel require IO specifically, not just non-Pure
    // =========================================================================

    #[test]
    fn test_select_in_log_only_fn_requires_io() {
        // A function with only Log effect (no IO) should not be able to use select
        assert_has_error(r#"fn bad {
    Log Int -> Int
    in: n
    out: select {
        x = <-n -> x
    }
}"#, "select requires IO");
    }

    #[test]
    fn test_select_in_io_fn_ok() {
        // A function with IO effect should be able to use select
        assert_no_errors(r#"fn good {
    IO Int -> Int
    in: n
    out: select {
        x = <-n -> x
    }
}"#);
    }

    // =========================================================================
    // Fix 4 regression: Never return type validation
    // =========================================================================

    #[test]
    fn test_never_fn_with_tail_call_ok() {
        assert_no_errors(r#"fn loop1 {
    IO Int -> Never
    in: n
    out: loop1(n)
}"#);
    }

    #[test]
    fn test_never_fn_with_do_block_tail_call_ok() {
        assert_no_errors(r#"fn loop2 {
    IO Int -> Never
    in: n
    out: do {
        let x = n + 1
        loop2(x)
    }
}"#);
    }

    #[test]
    fn test_never_fn_without_tail_call_error() {
        assert_has_error(r#"fn bad {
    Pure Int -> Never
    in: n
    out: n + 1
}"#, "function returning Never must end with a tail call");
    }

    // =========================================================================
    // Fix 5 regression: Channel.new<T>() parsing and type checking
    // =========================================================================

    #[test]
    fn test_channel_new_parses_and_types() {
        assert_no_errors(r#"fn makeChannel {
    IO Unit -> Channel<Int>
    in: _
    out: Channel.new<Int>()
}"#);
    }

    #[test]
    fn test_channel_new_requires_io() {
        assert_has_error(r#"fn bad {
    Pure Unit -> Channel<Int>
    in: _
    out: Channel.new<Int>()
}"#, "Channel.new requires IO");
    }

    // =========================================================================
    // Domain error types — built-in pre-registration
    // =========================================================================

    #[test]
    fn test_domain_errors_are_known_types() {
        use crate::types::check_source;
        for type_name in &[
            "DbError", "HttpError", "EnvError", "ParseError",
            "PortError", "JobError", "QueueError", "WorkerError",
            "RetryError", "LeaseError",
        ] {
            let errors = check_source(&format!(
                "fn useError {{ Pure Unit -> {0}\n    in: _\n    out: ConnectionFailed\n}}",
                type_name
            )).unwrap_or_default();
            let has_unknown_type = errors.iter().any(|e| e.message.contains("unknown type"));
            assert!(!has_unknown_type, "{} should be a known builtin type", type_name);
        }
    }

    // =========================================================================
    // Helper function scoping — regression tests
    // =========================================================================

    #[test]
    fn test_compact_helper_it_binding_ok() {
        // 'it' must be visible inside a compact helper body
        assert_no_errors(r#"fn double {
    Pure Int -> Int
    in: n
    out: go(n)
    helpers: {
        go :: Pure Int -> Int => it * 2
    }
}"#);
    }

    #[test]
    fn test_compact_helper_name_visible_in_out() {
        // The helper name must be visible in the out: expression
        assert_no_errors(r#"fn run {
    Pure Int -> Int
    in: n
    out: step(n)
    helpers: {
        step :: Pure Int -> Int => it + 1
    }
}"#);
    }

    #[test]
    fn test_compact_helper_name_visible_in_verify() {
        // The helper name must also be visible inside verify:
        assert_no_errors(r#"fn run {
    Pure Int -> Int
    in: n
    out: step(n)
    verify: {
        given(0) -> 1
    }
    helpers: {
        step :: Pure Int -> Int => it + 1
    }
}"#);
    }

    #[test]
    fn test_full_helper_visible_in_out() {
        // A full helper fn must be visible in the parent out: expression
        assert_no_errors(r#"fn run {
    Pure Int -> Int
    in: n
    out: helper(n)
    helpers: {
        fn helper {
            Pure Int -> Int
            in: x
            out: x + 10
        }
    }
}"#);
    }

    #[test]
    fn test_compact_helper_wrong_body_type_error() {
        // Compact helper body returning wrong type should produce a type error
        assert_has_error(r#"fn run {
    Pure Int -> String
    in: n
    out: step(n)
    helpers: {
        step :: Pure Int -> String => it + 1
    }
}"#, "type mismatch");
    }

    #[test]
    fn test_env_error_variants_resolve() {
        use crate::types::check_source;
        let errors = check_source(r#"fn handleEnv {
    IO Unit -> EnvError
    in: _
    out: Missing
}"#).unwrap_or_default();
        let has_type_error = errors.iter().any(|e| e.message.contains("unknown type"));
        assert!(!has_type_error, "EnvError variants should resolve without unknown-type errors");
    }

    // =========================================================================
    // Fix 1 — Channel.close type checking
    // =========================================================================

    #[test]
    fn test_channel_close_plain_channel_type_error() {
        // Channel.close must require a Closeable<Channel<T>>. Passing a plain
        // Channel<T> should produce a type error mentioning "Closeable".
        assert_has_error(r#"fn closeIt {
    IO Channel<Int> -> Unit
    in: ch
    out: Channel.close(ch)
}"#, "Closeable");
    }

    #[test]
    fn test_channel_close_closeable_channel_ok() {
        assert_no_errors(r#"fn closeIt {
    IO Closeable<Channel<Int>> -> Unit
    in: ch
    out: Channel.close(ch)
}"#);
    }
}
