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
}
