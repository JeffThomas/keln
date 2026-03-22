use serde::{Deserialize, Serialize};

// =============================================================================
// Per-function verify result
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnVerifyResult {
    pub fn_name: String,
    pub given: Vec<GivenOutcome>,
    pub forall: Vec<ForAllOutcome>,
}

impl FnVerifyResult {
    pub fn is_clean(&self) -> bool {
        self.given.iter().all(|g| g.passed)
            && self.forall.iter().all(|f| matches!(f.status, ProofStatus::Passed { .. }))
    }

    pub fn failure_count(&self) -> usize {
        self.given.iter().filter(|g| !g.passed).count()
            + self
                .forall
                .iter()
                .filter(|f| !matches!(f.status, ProofStatus::Passed { .. }))
                .count()
    }
}

// =============================================================================
// Given outcome
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GivenOutcome {
    pub input: String,
    pub expected: String,
    /// Ok(display string of actual value) or Err(error message)
    pub actual: Result<String, String>,
    pub passed: bool,
}

// =============================================================================
// ForAll outcome
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForAllOutcome {
    pub status: ProofStatus,
    /// Binding name → display string of counterexample value
    pub counterexample: Option<Vec<(String, String)>>,
    pub iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofStatus {
    Passed { iterations: usize },
    Failed,
    Timeout,
    Error(String),
}

// =============================================================================
// Program-level VerificationResult
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub compile_errors: Vec<String>,
    pub test_failures: Vec<TestFailure>,
    pub coverage_gaps: Vec<String>,
    pub proof_violations: Vec<ProofViolation>,
    pub is_clean: bool,
}

impl VerificationResult {
    pub fn from_fn_results(fn_results: &[FnVerifyResult]) -> Self {
        let mut test_failures = Vec::new();
        let mut proof_violations = Vec::new();

        for r in fn_results {
            for (i, g) in r.given.iter().enumerate() {
                if !g.passed {
                    test_failures.push(TestFailure {
                        fn_name: r.fn_name.clone(),
                        case_index: i,
                        kind: match &g.actual {
                            Ok(actual) => TestFailureKind::Mismatch {
                                input: g.input.clone(),
                                expected: g.expected.clone(),
                                actual: actual.clone(),
                            },
                            Err(e) => TestFailureKind::RuntimeError {
                                input: g.input.clone(),
                                error: e.clone(),
                            },
                        },
                    });
                }
            }
            for (i, f) in r.forall.iter().enumerate() {
                if !matches!(f.status, ProofStatus::Passed { .. }) {
                    proof_violations.push(ProofViolation {
                        fn_name: r.fn_name.clone(),
                        property_index: i,
                        status: f.status.clone(),
                        counterexample: f.counterexample.clone(),
                    });
                }
            }
        }

        let is_clean = test_failures.is_empty() && proof_violations.is_empty();

        VerificationResult {
            compile_errors: vec![],
            test_failures,
            coverage_gaps: vec![],
            proof_violations,
            is_clean,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
    }
}

// =============================================================================
// Test failure detail
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    pub fn_name: String,
    pub case_index: usize,
    pub kind: TestFailureKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestFailureKind {
    Mismatch { input: String, expected: String, actual: String },
    RuntimeError { input: String, error: String },
}

// =============================================================================
// Proof violation detail
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofViolation {
    pub fn_name: String,
    pub property_index: usize,
    pub status: ProofStatus,
    pub counterexample: Option<Vec<(String, String)>>,
}
