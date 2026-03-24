#[cfg(test)]
mod tests {
    use crate::verify::{result::ProofStatus, VerifyExecutor};

    // =========================================================================
    // Given — pure functions
    // =========================================================================

    #[test]
    fn test_verify_given_pass() {
        let src = r#"fn double { Pure Int -> Int
    in: n
    out: n + n
    verify: {
        given(3) -> 6
        given(0) -> 0
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("double").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert_eq!(r.given.len(), 2);
        assert!(r.given[0].passed, "given(3)->6 failed: {:?}", r.given[0]);
        assert!(r.given[1].passed, "given(0)->0 failed: {:?}", r.given[1]);
        assert!(r.is_clean());
    }

    #[test]
    fn test_verify_given_fail() {
        let src = r#"fn double { Pure Int -> Int
    in: n
    out: n + n
    verify: {
        given(3) -> 7
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("double").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(!r.given[0].passed, "should have failed");
        assert!(!r.is_clean());
    }

    #[test]
    fn test_verify_given_string() {
        let src = r#"fn greet { Pure String -> String
    in: name
    out: String.concat("Hello, ", name)
    verify: {
        given("World") -> "Hello, World"
        given("")       -> "Hello, "
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("greet").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(r.given[0].passed);
        assert!(r.given[1].passed);
    }

    #[test]
    fn test_verify_given_with_match() {
        let src = r#"fn abs { Pure Int -> Int
    in: n
    out: match n > 0 {
        true  -> n
        false -> 0 - n
    }
    verify: {
        given(5) -> 5
        given(0) -> 0
        given(3) -> 3
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("abs").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(r.given.iter().all(|g| g.passed), "failures: {:?}", r.given);
    }

    // =========================================================================
    // ForAll — simple properties
    // =========================================================================

    #[test]
    fn test_verify_forall_double_nonneg() {
        let src = r#"fn double { Pure Int -> Int
    in: n
    out: n + n
    verify: {
        forall(n: Int where 0..100) ->
            double(n) >= 0
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("double").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert_eq!(r.forall.len(), 1);
        assert!(
            matches!(r.forall[0].status, ProofStatus::Passed { .. }),
            "forall failed: {:?}",
            r.forall[0]
        );
    }

    #[test]
    fn test_verify_forall_counterexample() {
        // double(n) >= 10 is false for n < 5
        let src = r#"fn double { Pure Int -> Int
    in: n
    out: n + n
    verify: {
        forall(n: Int where 0..20) ->
            double(n) >= 10
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("double").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(
            matches!(r.forall[0].status, ProofStatus::Failed),
            "expected failure, got: {:?}",
            r.forall[0]
        );
        assert!(r.forall[0].counterexample.is_some());
    }

    #[test]
    fn test_verify_forall_bool() {
        let src = r#"fn negate { Pure Bool -> Bool
    in: b
    out: match b {
        true  -> false
        false -> true
    }
    verify: {
        forall(b: Bool) ->
            negate(negate(b)) == b
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("negate").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(matches!(r.forall[0].status, ProofStatus::Passed { .. }));
    }

    // =========================================================================
    // Mock — FunctionRef mocking
    // =========================================================================

    #[test]
    fn test_verify_mock_fn_ref() {
        let src = r#"fn applyFn { Pure { n: Int, f: FunctionRef<Pure, Int, Int> } -> Int
    in: { n, f }
    out: f(n)
    verify: {
        mock f {
            call(_) -> 42
        }
        given({ n: 1, f: _ }) -> 42
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("applyFn").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(r.given[0].passed, "mock fn ref test failed: {:?}", r.given[0]);
    }

    // =========================================================================
    // Mock — module method mocking (Clock.now pattern)
    // =========================================================================

    #[test]
    fn test_verify_module_mock() {
        let src = r#"fn getEpoch { IO Unit -> Int
    in: _
    out: Clock.now().epoch_ms
    verify: {
        mock Clock {
            now() -> Timestamp { epoch_ms: 1000 }
        }
        given(Unit) -> 1000
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("getEpoch").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(r.given[0].passed, "module mock test failed: {:?}", r.given[0]);
    }

    #[test]
    fn test_verify_module_mock_persists_across_givens() {
        let src = r#"fn getTwice { IO Unit -> Int
    in: _
    out: Clock.now().epoch_ms + Clock.now().epoch_ms
    verify: {
        mock Clock {
            now() -> Timestamp { epoch_ms: 5 }
        }
        given(Unit) -> 10
        given(Unit) -> 10
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("getTwice").cloned().unwrap();
        let r = ex.verify_fn(&fd);
        assert!(r.given[0].passed, "first given failed: {:?}", r.given[0]);
        assert!(r.given[1].passed, "second given failed: {:?}", r.given[1]);
    }

    // =========================================================================
    // VerificationResult JSON
    // =========================================================================

    #[test]
    fn test_verification_result_json_clean() {
        use crate::verify::result::VerificationResult;
        let src = r#"fn inc { Pure Int -> Int
    in: n
    out: n + 1
    verify: {
        given(0) -> 1
        given(9) -> 10
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("inc").cloned().unwrap();
        let fn_r = ex.verify_fn(&fd);
        let vr = VerificationResult::from_fn_results(&[fn_r]);
        assert!(vr.is_clean);
        let json = vr.to_json();
        assert!(json.contains("\"is_clean\": true"));
    }

    #[test]
    fn test_verification_result_json_failure() {
        use crate::verify::result::VerificationResult;
        let src = r#"fn inc { Pure Int -> Int
    in: n
    out: n + 1
    verify: {
        given(0) -> 99
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fd = ex.evaluator.fns.get("inc").cloned().unwrap();
        let fn_r = ex.verify_fn(&fd);
        let vr = VerificationResult::from_fn_results(&[fn_r]);
        assert!(!vr.is_clean);
        let json = vr.to_json();
        assert!(json.contains("\"is_clean\": false"));
    }

    // =========================================================================
    // verify_all
    // =========================================================================

    #[test]
    fn test_verify_all() {
        let src = r#"fn inc { Pure Int -> Int
    in: n
    out: n + 1
    verify: {
        given(5) -> 6
    }
}
fn dec { Pure Int -> Int
    in: n
    out: n - 1
    verify: {
        given(5) -> 4
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let results = ex.verify_all();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_clean()), "failures: {:?}", results);
    }

    // =========================================================================
    // Fuzz harness tests
    // =========================================================================

    #[test]
    fn test_fuzz_trusted_module_no_fuzz_block() {
        let src = r#"trusted module HttpServer {
    provides: {
        start: IO Unit -> Unit
    }
    reason: "HTTP stack; correctness guaranteed by integration tests"
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let results = ex.fuzz_trusted_modules();
        assert_eq!(results.len(), 1);
        assert!(!results[0].has_fuzz_block, "HttpServer has no fuzz block");
        assert!(results[0].methods.is_empty());
    }

    #[test]
    fn test_fuzz_json_parse_returns_result() {
        let src = r#"trusted module Json {
    provides: {
        parse:     Pure String -> Unit
        serialize: Pure String -> Unit
    }
    reason: "correctness guaranteed by external test suite"
    fuzz: {
        parse: inputs(String) -> returns_result
        serialize: inputs(String) -> crashes_never
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let results = ex.fuzz_trusted_modules();
        assert_eq!(results.len(), 1);
        assert!(results[0].has_fuzz_block);
        assert_eq!(results[0].methods.len(), 2);

        let parse_result = &results[0].methods[0];
        assert_eq!(parse_result.fn_name, "parse");
        assert!(parse_result.passed, "Json.parse fuzz failed: {:?}", parse_result.failure);

        let ser_result = &results[0].methods[1];
        assert_eq!(ser_result.fn_name, "serialize");
        assert!(ser_result.passed, "Json.serialize fuzz failed: {:?}", ser_result.failure);
    }

    #[test]
    fn test_fuzz_result_in_verification_result_json() {
        let src = r#"trusted module Json {
    provides: {
        parse: Pure String -> Unit
    }
    reason: "test"
    fuzz: {
        parse: inputs(String) -> returns_result
    }
}"#;
        let mut ex = VerifyExecutor::from_source(src).unwrap();
        let fn_results = ex.verify_all();
        let mut vr = crate::verify::result::VerificationResult::from_fn_results(&fn_results);
        vr.fuzz_status = ex.fuzz_trusted_modules();
        let json = vr.to_json();
        assert!(json.contains("fuzz_status"), "fuzz_status should appear in JSON");
        assert!(json.contains("Json"), "module name should appear in JSON");
    }
}
