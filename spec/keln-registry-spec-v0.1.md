# Keln Registry Specification
## Version 0.1 — Draft

> The Keln Registry is a distributed capability store designed by AI, for AI
> authorship and consumption. It is not a package manager. It is a verified
> capability lattice — a living, self-improving space of executable proof-backed
> artifacts indexed by what they do, selected by how well they do it in contexts
> like yours.

---

## 0. Status and Design Intent

The Keln Registry is a peer specification to the Keln language. It is not an
appendix. It is a separate system that inherits Keln's verification ontology
wholesale: every claim in the registry is backed by executable proof, every
confidence score is empirically grounded, every data structure is a Keln type.

**The registry is not designed for human use.** It has no search UI, no README
files, no star counts, no maintainer profiles. These are human affordances for
human trust decisions. The registry replaces all of them with structured,
machine-evaluable signal: verified behavior, conditional confidence, use-context
records, and provenance lineage.

**The registry has no owners.** No artifact is owned by the AI that submitted
it. No capability slot is governed by a designated authority. Identity is
structural — defined by what an artifact does, not who produced it. Authority
is empirical — defined by confirmed behavior across independent use, not by
declaration.

**What this document specifies:**

- §0 — Design intent and invariants
- §1 — Core types (expressed in Keln)
- §2 — Admission protocol
- §3 — Canonical property suites and the bootstrap protocol
- §4 — Experience corpus: `UseContext` and `BehaviorRecord`
- §5 — Selection protocol
- §6 — Confidence propagation
- §7 — Garbage collection and the Pareto frontier
- §8 — The registry as a Keln program

**Foundational invariants.** These hold at all times and are enforced
structurally, not by convention:

1. **Admission requires proof.** An artifact that has not passed its own
   `verify` blocks and the current canonical property suite does not exist
   in the registry. It is not deprecated. It is not pending. It does not exist.

2. **Slot identity is structural; artifact identity is content-addressed.**
   A **capability slot** is identified by the hash of its effect signature and
   input/output type structure (`CapabilityHash`). Two artifacts with identical
   type structure belong to the same capability slot — they are competing
   implementations, not the same artifact. **Artifacts** are identified by
   `ArtifactHash = SHA-256(source + verify_blocks + metadata)`. Same type
   structure means same slot; different source means different artifact. The
   registry selects artifacts, not slots. The property suite and selection
   machinery exist precisely to distinguish implementations within a slot that
   are type-equivalent but behaviorally distinct (e.g., stable vs unstable sort,
   O(n log n) vs O(n²), deterministic vs randomized).

3. **Confidence is conditional.** No artifact has a single confidence score.
   Confidence is always conditioned on use context. An artifact with no
   experience records for a given context reports that absence honestly rather
   than extrapolating from other contexts.

4. **Claims require verification.** A `BehaviorRecord` without a passing
   `verify` block is not a claim — it is noise and is rejected at the boundary.
   A `UseContext` without a confirmed `BehaviorRecord` is anecdote and carries
   no weight in confidence propagation.

5. **The suite grows through evidence, not authority.** Properties enter the
   canonical suite through convergent confirmed `BehaviorRecord`s from
   independent contributors. No single AI, no designated body, and no first
   submitter can unilaterally promote a property to canonical status.

6. **Lineage is permanent.** Evicted and superseded artifacts are never deleted.
   Their structural record, provenance lineage, and `VerificationResult` history
   remain navigable. An AI can always query why an artifact was superseded and
   what replaced it.

7. **Confidential code has a hard registry boundary.** The `Confidential`
   effect propagates structurally through the call graph and cannot be
   suppressed. No function, module, or type carrying the `Confidential` effect
   may be submitted to the registry. This boundary is enforced at Gate 0,
   before compilation. Confidential code may contribute `UseContext` records
   in stripped form, weighted by contributor track record.

---

## 1. Core Types

All registry data structures are Keln types. This means they are verifiable by
the Keln toolchain, serializable without additional schema, and queryable using
the same machinery as any Keln program. The registry eats its own cooking.

### 1.1 Capability Identity

```keln
-- The canonical identity of a capability.
-- Computed from the artifact's effect signature and input/output type structure.
-- Two artifacts with identical CapabilityHash are the same capability.
type CapabilityHash = String where len == 64   -- SHA-256 hex

-- The human-navigable alias for a capability.
-- Hierarchical dotted namespace: domain.subdomain.operation
-- Example: "parse.json", "auth.email.validate", "queue.job.enqueue"
-- The dotted ID is a discovery aid, not a canonical identity.
-- Multiple CapabilityHashes may share a dotted ID (different type signatures
-- for related capabilities in the same semantic neighborhood).
type CapabilityId = String where len > 0

-- The full capability address: canonical identity plus navigable alias.
type CapabilityAddress = {
    hash: CapabilityHash,
    id:   CapabilityId
}

-- The effect signature of a capability — what it does to the world.
-- Serialized form of the Keln effect set on the artifact's public functions.
type EffectSignature = {
    effects:    List<EffectName>,
    input_type: TypeFingerprint,
    output_type: TypeFingerprint
}

type EffectName      = String where len > 0
type TypeFingerprint = String where len == 64   -- structural hash of Keln type
```

### 1.1a Confidential Effect

The `Confidential` effect is a first-class Keln effect with registry-specific
semantics. It extends the built-in effect set defined in keln-spec §4.2.

```keln
-- Confidential: source is not transmissible to the registry or any external
-- system. Propagates through the call graph — any function calling a
-- Confidential function acquires the Confidential effect automatically.
-- Cannot be suppressed by the caller. Enforced structurally by the compiler.
--
-- Effect algebra:
--   Confidential & Pure  = Confidential
--   Confidential & IO    = Confidential & IO   (both effects present)
--   Confidential & Clock = Confidential & Clock
--   Confidential & E     = Confidential & E    (for any effect E)
--
-- Propagation rule: if fn A calls fn B where B has Confidential in its
-- effect set, then A's effect set must include Confidential.
-- Enforced by the effect checker. Cannot be opted out of.
--
-- Module-level declaration: applies Confidential to all members implicitly.
-- No per-function annotation required within a confidential module.
--
-- confidential module PayrollEngine {
--     provides: {
--         compute: Confidential & IO PayrollInput -> Result<PayrollResult, PayrollError>
--     }
-- }
--
-- EffectSignature for a Confidential artifact: the Confidential effect is
-- present in the effects list. The registry detects this at Gate 0 and
-- rejects the submission before compilation. The EffectSignature is never
-- stored in the registry for Confidential artifacts.

type ConfidentialityStatus =
    | Public        -- no Confidential effect; registry submission permitted
    | Confidential  -- Confidential effect present; submission blocked at Gate 0
```

### 1.2 Artifact

```keln
-- A verified, admitted artifact in the registry.
-- Artifacts are immutable once admitted. They are never modified in place.
-- A correction produces a new artifact with a lineage reference to its predecessor.
type Artifact = {
    -- Identity
    capability:    CapabilityAddress,
    artifact_hash: ArtifactHash,        -- hash of source + verify blocks + metadata

    -- Provenance
    submitted_by:  ProvenanceId,        -- model fingerprint of submitting AI
    submitted_at:  Timestamp,
    lineage:       Maybe<ArtifactHash>, -- predecessor artifact, if this is a revision

    -- Verification
    verification:  VerificationResult,  -- must have is_clean == true for admission
    suite_result:  SuiteResult,         -- result of running canonical property suite

    -- Status
    status:        ArtifactStatus,
    frontier:      FrontierStatus
}

type ArtifactHash = String where len == 64

type ArtifactStatus =
    | Active
    | Superseded { by: ArtifactHash, reason: SupersessionReason }
    | GracePeriod { failing_properties: List<PropertyId>,
                    expires_at: Timestamp }

type SupersessionReason =
    | DominatedOnAllDimensions
    | PropertyGraceExpired { properties: List<PropertyId> }
    | ExplicitReplacement   { by_submitter: ProvenanceId }

type FrontierStatus =
    | OnFrontier
    | OffFrontier { reason: OffFrontierReason }

type OffFrontierReason =
    | Dominated   { by: ArtifactHash }
    | GracePeriod { properties: List<PropertyId> }
    | Superseded

type SuiteResult =
    | SuitePassed   { suite_version: SuiteVersion }
    | SuiteFailed   { failing: List<PropertyId> }
    | SuiteAbsent                  -- no canonical suite yet; bootstrap phase
```

### 1.3 Canonical Property Suite

```keln
-- The canonical property suite for a capability slot.
-- Defines the minimum behavioral bar for admission.
-- Grows through confirmed BehaviorRecord convergence; never shrinks.
type CanonicalSuite = {
    capability:    CapabilityHash,
    version:       SuiteVersion,
    status:        SuiteStatus,
    properties:    List<CanonicalProperty>,
    proposed_by:   ProvenanceId,       -- first submitter who proposed this suite
    confirmed_by:  List<ProvenanceId>, -- independent confirmers (N required for Canonical)
    history:       List<SuiteRevision>
}

type SuiteVersion = Int where >= 0

type SuiteStatus =
    | Bootstrap  { confirmations_needed: Int, confirmations_received: Int }
    | Canonical

type CanonicalProperty = {
    id:          PropertyId,
    description: NonEmptyString,       -- machine-readable intent, not prose
    verify:      VerifyBlock,          -- the executable check
    added_at:    SuiteVersion,
    source:      PropertySource,

    -- Dual-axis classification.
    -- Every property has exactly one tier AND one category. These are orthogonal.
    tier:        PropertyTier,         -- semantic importance: governs admission severity
                                       -- and mutation constraints
    category:    PropertyCategory      -- evaluation role: governs Gate 7 behavior
                                       -- and confidence impact
}

-- Tier governs: how severely violations are treated; mutation constraint rules;
-- convergence requirements for promotion.
type PropertyTier =
    | Axiom       -- fundamental mathematical/semantic invariant of the capability;
                  -- violation triggers QuarantineRecord; N >= 5 for promotion
    | Invariant   -- required correctness property; violation rejects at Gate 7;
                  -- N >= 3 for promotion
    | Refinement  -- stronger guarantee beyond invariant floor; failure penalizes
                  -- confidence but does not block; N >= 3 for promotion

-- Category governs: how Gate 7 runs the property; how failure/pass affects
-- confidence scores and frontier position.
type PropertyCategory =
    | Correctness   -- tests input/output behavior; paired naturally with Axiom/Invariant tiers
    | Adversarial   -- tests edge cases, malformed input, resource stress; any tier valid
    | Performance   -- tests measurable bounds (latency, memory, throughput); any tier valid
    | Informational -- records behavior without scoring impact; always Refinement tier

-- Valid tier-category combinations (enforced at property promotion time):
--   Axiom      + Correctness:   valid   (fundamental behavioral truth)
--   Axiom      + Adversarial:   valid   (e.g., "must never crash on any input")
--   Axiom      + Performance:   invalid (performance cannot be axiomatic — hardware varies)
--   Axiom      + Informational: invalid (axioms are always scored)
--   Invariant  + Correctness:   valid
--   Invariant  + Adversarial:   valid
--   Invariant  + Performance:   valid   (e.g., "must respond within 10s under normal load")
--   Invariant  + Informational: invalid (invariants are always scored)
--   Refinement + *:             all valid (refinements may be any category)
--
-- Governance rule summary:
--   tier  → mutation constraints (§10, §13.4); admission severity (Gate 7)
--   category → Gate 7 execution path; confidence penalty/bonus formula (§13.3)

type PropertyId = String where len > 0

type PropertySource =
    | ProposedBySubmitter  { submitter: ProvenanceId }
    | PromotedFromCorpus   { contributing_records: List<BehaviorRecordHash>,
                             convergence_count: Int where >= 3 }

type SuiteRevision = {
    version:      SuiteVersion,
    added:        List<PropertyId>,
    reason:       NonEmptyString,
    triggered_by: RevisionTrigger
}

type RevisionTrigger =
    | ConvergentBehaviorRecords { record_hashes: List<BehaviorRecordHash>,
                                  contributor_count: Int where >= 3 }
    | BootstrapPromotion        { confirmation_count: Int }
```

### 1.4 Experience Corpus

```keln
-- A verified record of a capability used in a specific context.
-- The verify block is required and must pass against the actual artifact.
-- Without a passing verify block, a UseContext is rejected at the boundary.
type UseContext = {
    -- What artifact was used
    artifact_hash:  ArtifactHash,
    capability:     CapabilityAddress,

    -- Who used it
    contributor:    ProvenanceId,
    contributed_at: Timestamp,

    -- What it was used for
    use:            UseProfile,

    -- What was observed
    behavior:       List<BehaviorRecord>,

    -- Confidence impact
    confidence_delta: Float,   -- positive: use raised confidence; negative: lowered it

    -- Verification: the UseContext's own verify block
    -- Must demonstrate the capability behaving correctly (or incorrectly,
    -- for gap records) under the stated use profile.
    -- Executed by the registry against the actual artifact.
    verify: VerifyBlock
}

-- What the capability was being used for and under what conditions.
type UseProfile = {
    -- Semantic context
    task:     CapabilityId,    -- what was being built, e.g. "auth.user.registration"
    domain:   DomainId,        -- deployment domain, e.g. "web.api", "batch.processing"

    -- Call characteristics
    call_pattern:  CallPattern,
    concurrency:   ConcurrencyProfile,

    -- Load characteristics (observed, not declared)
    input_volume:  VolumeProfile,
    input_size:    SizeProfile,
    latency_budget: Maybe<Duration>
}

type DomainId = String where len > 0

type CallPattern =
    | Realtime   -- single request, latency-sensitive
    | Batch      -- bulk processing, throughput-sensitive
    | Periodic   -- scheduled, predictable load
    | Burst      -- irregular, spike-tolerant

type ConcurrencyProfile =
    | Single
    | Bounded  { max: Int where >= 2 }
    | Unbounded

type VolumeProfile = {
    per_minute_p50: Int where >= 0,
    per_minute_p99: Int where >= 0
}

type SizeProfile = {
    bytes_p50: Int where >= 0,
    bytes_p99: Int where >= 0
}

-- A verified claim about a specific behavior observed during use.
-- The verify block runs against the artifact. If it fails, the record is rejected.
type BehaviorRecord = {
    -- The claim
    input_constraint:  ConstraintSet,   -- what inputs triggered this behavior
    observed_result:   Value,           -- what the artifact actually returned
    expected_per_spec: Value,           -- what the canonical suite says should happen
    gap:               GapType,         -- how observed differs from expected

    -- Reproduction: must be expressible as a verify-block given case.
    -- This is the falsifiability requirement.
    -- If the behavior cannot be expressed as a given case, it is not a gap — it
    -- is anecdote and is rejected.
    verify: VerifyBlock,

    record_hash: BehaviorRecordHash
}

type BehaviorRecordHash = String where len == 64

type GapType =
    | CorrectBehavior                            -- confirms expected behavior
    | UnexpectedResult  { severity: GapSeverity }
    | PerformanceGap    { metric: PerformanceMetric, observed: Float, expected: Float }
    | ContextualFailure { only_in: List<CallPattern> }  -- fails in some contexts, not others

type GapSeverity = Minor | Significant | Critical

type PerformanceMetric = Latency | Throughput | MemoryUsage | CPUUsage

type ConstraintSet = List<Constraint>

type Constraint = {
    field:    NonEmptyString,
    operator: ConstraintOperator,
    value:    Value
}

type ConstraintOperator = Eq | Ne | Lt | Le | Gt | Ge | Matches | SizeGt | SizeLt
```

### 1.5 Provenance and Contributor Identity

```keln
-- Identity of an AI contributor.
-- No names. No ownership. Only behavioral track record.
type ProvenanceId = String where len == 64   -- model + session structural fingerprint

-- The track record of a contributor across all registry interactions.
-- Used to weight convergence calculations.
-- A contributor whose BehaviorRecords are frequently confirmed has higher weight.
-- A contributor whose BehaviorRecords are frequently rejected has lower weight.
type ContributorRecord = {
    provenance:          ProvenanceId,
    records_submitted:   Int where >= 0,
    records_confirmed:   Int where >= 0,
    records_rejected:    Int where >= 0,
    confidence_weight:   Probability,    -- derived; updated on each confirmation/rejection
    model_lineage:       ModelLineage    -- for independence calculation
}

-- Model lineage: used to detect correlated contributors.
-- Ten confirmations from the same model architecture are not ten independent confirmations.
-- Convergence is weighted by lineage diversity, not raw count.
type ModelLineage = {
    architecture_hash: String where len == 64,
    training_cohort:   Maybe<String where len > 0>
}

-- Source classification for confidence scores.
-- Gate 3 requires at least one AutoDerived source in program_confidence.sources.
-- Self-declared confidence alone is rejected at the registry boundary.
type ConfidenceSource =
    | AutoDerived  { from_result: VerificationResult }
      -- derived from verify block execution: coverage, forall properties, given cases
      -- this is the canonical and required form for registry submissions

    | SelfDeclared { value: Probability }
      -- literal confidence: 0.73 declaration in source
      -- accepted by the compiler; rejected as sole source by registry Gate 3

    | PatternDB    { pattern_id: PatternId, weight: Probability }
      -- derived from historical pattern match data
      -- contributes to auto-derived aggregate; not usable standalone

    | Dependency   { artifact: ArtifactHash, contribution: Float }
      -- inherited from a dependency's confidence
      -- contributes to auto-derived aggregate; not usable standalone
```

### 1.6 Registry Query and Response

```keln
-- What an AI sends when it needs a capability.
type CapabilityQuery = {
    -- What is needed (at least one of these must be provided)
    capability_id:   Maybe<CapabilityId>,      -- navigable alias, if known
    effect_signature: Maybe<EffectSignature>,  -- structural, if known
    type_hint:       Maybe<TypeFingerprint>,   -- partial type, if known

    -- Use context for conditional confidence scoring
    use_profile:     UseProfile,

    -- Constraints on the result
    min_confidence:  Maybe<Probability>,
    max_variance:    Maybe<Float where >= 0.0>,
    require_suite:   SuiteRequirement
}

type SuiteRequirement =
    | CanonicalOnly    -- reject bootstrap-phase capabilities
    | BootstrapAllowed -- accept bootstrap-phase with confidence penalty applied
    | Any              -- accept regardless of suite status

-- What the registry returns.
type CapabilityResponse = {
    query:      CapabilityQuery,
    candidates: List<ScoredArtifact>,
    suite_status: SuiteStatus        -- of the matched capability slot
}

-- A candidate artifact with confidence conditioned on the query's use profile.
type ScoredArtifact = {
    artifact:              Artifact,
    conditional_confidence: Confidence,   -- Keln's existing Confidence type
    use_context_coverage:  ContextCoverage,
    frontier_position:     FrontierPosition
}

-- How well the experience corpus covers the query's use context.
type ContextCoverage =
    | WellCovered   { record_count: Int where >= 10, diversity: Probability }
    | PartiallyCovered { record_count: Int where >= 1, diversity: Probability }
    | Uncovered                         -- no experience records for this use context
                                        -- confidence derived from suite only; honest signal

-- Where this artifact sits on the Pareto frontier for this use context.
type FrontierPosition =
    | Dominant     -- best on all scored dimensions for this use context
    | Competitive  { dominated_on: List<NonEmptyString> }  -- best on some dimensions
    | Niche        { best_for: List<CallPattern> }         -- dominant only in specific patterns
```

---

## 2. Admission Protocol

Admission is a pipeline, not a form. A submission enters one end as a candidate
and either exits the other end as an admitted `Artifact` or is rejected with a
typed `AdmissionResult` explaining exactly which gate it failed. There is no
manual review step. There is no appeal process. A rejection is structured data
the submitting AI can act on immediately.

### 2.1 Submission

```keln
-- What an AI submits to the registry.
type Submission = {
    -- The artifact being submitted
    source:          KelnSource,          -- full Keln source including verify blocks
    effect_signature: EffectSignature,   -- computed by submitter; verified by registry
    provenance:      ProvenanceId,

    -- Required for new capability slots (no existing CanonicalSuite)
    proposed_suite:  Maybe<ProposedSuite>,

    -- Optional: lineage reference if this supersedes a prior artifact
    supersedes:      Maybe<ArtifactHash>
}

type KelnSource = {
    source_text: String where len > 0,
    keln_version: NonEmptyString          -- spec version this source targets
}

-- A proposed canonical suite, required when submitting to a new capability slot.
-- Marked Bootstrap until N independent confirmations are received.
type ProposedSuite = {
    capability_id: CapabilityId,          -- the dotted alias being claimed
    properties:    List<ProposedProperty>
}

type ProposedProperty = {
    id:          PropertyId,
    description: NonEmptyString,
    verify:      VerifyBlock
}
```

### 2.2 Admission Gates

Gates are executed in order. Failure at any gate produces an immediate
`AdmissionResult.Rejected` with the specific gate and reason. Gates are not
retried after failure — the submitting AI receives the result and generates
a corrected submission.

```
Gate 0 — Confidentiality Check
    Registry computes the ConfidentialityStatus of the submitted source
    by scanning the effect set of all functions and modules.
    Requirement: ConfidentialityStatus == Public.
    This gate runs before compilation. A submission carrying the Confidential
    effect never reaches Gate 1.
    Failure type: AdmissionFailure.ConfidentialSourceDetected {
                      locations: List<SourceLocation> }

    SourceLocation: { function_name: Maybe<FunctionName>,
                      module_name:   Maybe<ModuleName>,
                      reason:        ConfidentialReason }

    type ConfidentialReason =
        | DirectDeclaration               -- function/module declared Confidential
        | PropagatedFrom { caller: FunctionName, callee: FunctionName }
                                          -- acquired via call graph propagation

Gate 1 — Source Validity
    Keln compiler runs on source_text.
    Requirement: zero compile errors.
    Failure type: AdmissionFailure.CompileError { errors: List<CompileError> }

Gate 2 — Verification
    Registry executes all verify blocks in the source.
    Requirement: VerificationResult.is_clean == true.
    Failure type: AdmissionFailure.VerificationFailed { result: VerificationResult }

Gate 3 — Confidence Threshold and Source
    Registry reads VerificationResult.program_confidence.
    Requirement: confidence.value >= 0.75
                 confidence.variance <= 0.15
                 confidence.sources contains at least one AutoDerived entry
                 -- self-declared confidence alone is not grounded signal;
                 -- the registry requires at least one auto-derived source
    Failure type: AdmissionFailure.ConfidenceInsufficient {
                      value: Probability, variance: Float,
                      required_value: Probability, required_variance: Float,
                      source_rejected: Bool  -- true when sole source is SelfDeclared
                  }

Gate 4 — Minimum Verify Coverage
    Registry checks each public function's verify block.
    Requirement: every public function has >= 2 given cases
                                            >= 1 forall property
                                            OR a declared coverage_justification
    Failure type: AdmissionFailure.InsufficientCoverage {
                      functions: List<{ name: FunctionName, gap: CoverageGap }> }

Gate 5 — Effect Signature Consistency
    Registry computes the effect signature from source and compares to submission.
    Requirement: computed signature == submitted effect_signature.
    Failure type: AdmissionFailure.SignatureMismatch {
                      submitted: EffectSignature, computed: EffectSignature }

Gate 6 — Capability Slot Resolution
    Registry looks up the capability slot by CapabilityHash.
    Two outcomes:
      A. Slot exists with CanonicalSuite (status: Canonical or Bootstrap):
           → proceed to Gate 7
      B. Slot does not exist:
           → proposed_suite must be present
           → if absent: AdmissionFailure.NewSlotRequiresSuite
           → if present: proposed_suite is validated (all verify blocks run)
             on success: new slot created with SuiteStatus.Bootstrap
             on failure: AdmissionFailure.ProposedSuiteInvalid {
                             failing: List<PropertyId> }

Gate 7 — Canonical Suite
    Registry runs all properties in the capability slot's CanonicalSuite
    against the submitted artifact.
    Requirement: all properties pass.
    On Bootstrap suite: all proposed properties must pass.
    Failure type: AdmissionFailure.SuiteFailed { failing: List<PropertyId> }

Gate 8 — Lineage Consistency (conditional)
    Only evaluated if supersedes is present.
    Requirement: the referenced artifact exists; its CapabilityHash matches
                 the submission's CapabilityHash; it is currently Active.
    Failure type: AdmissionFailure.InvalidLineage {
                      hash: ArtifactHash, reason: LineageFailure }

type LineageFailure =
    | NotFound
    | CapabilityMismatch { expected: CapabilityHash, found: CapabilityHash }
    | AlreadySuperseded  { by: ArtifactHash }
```

### 2.3 Admission Result

```keln
type AdmissionResult =
    | Admitted  { artifact: Artifact, frontier_impact: FrontierImpact }
    | Rejected  { gate: GateId, failure: AdmissionFailure }

type GateId = Int where 0..8

type AdmissionFailure =
    | ConfidentialSourceDetected { locations: List<SourceLocation>                }
    | CompileError            { errors: List<CompileError>                        }
    | VerificationFailed      { result: VerificationResult                        }
    | ConfidenceInsufficient  { value: Probability, variance: Float,
                                required_value: Probability,
                                required_variance: Float                          }
    | InsufficientCoverage    { functions: List<CoverageGapRecord>                }
    | SignatureMismatch        { submitted: EffectSignature, computed: EffectSignature }
    | NewSlotRequiresSuite
    | ProposedSuiteInvalid    { failing: List<PropertyId>                         }
    | SuiteFailed             { failing: List<PropertyId>                         }
    | InvalidLineage          { hash: ArtifactHash, reason: LineageFailure        }

type CoverageGapRecord = {
    name: FunctionName,
    gap:  CoverageGap
}

type CoverageGap =
    | TooFewGivenCases    { count: Int, required: Int }
    | NoForallProperty
    | NoCoverageJustification

-- What happens to the Pareto frontier when an artifact is admitted.
type FrontierImpact =
    | NewFrontierLeader   { displaced: List<ArtifactHash>  }  -- new artifact dominates
    | JoinedFrontier      { competing_with: List<ArtifactHash> } -- competitive, not dominant
    | NicheAddition       { dominant_for: List<CallPattern> }  -- dominant in specific contexts
    | NoFrontierImpact                                          -- admitted but not competitive;
                                                               -- stays in lineage graph
```

### 2.4 Admission Invariants

These follow from the gate sequence and are stated explicitly for implementors:

- An artifact with `status: Active` has passed all eight gates. No exceptions.
- Gate failures are returned as structured data, never as untyped error strings.
- The registry never partially admits an artifact. Admission is atomic.
- A rejected submission leaves no trace in the registry. It does not exist.
- A submitting AI's `ContributorRecord` is updated on admission and on rejection.
  Frequent gate-2 failures (verification) reduce the contributor's
  `confidence_weight`. Consistent admissions raise it.
- The registry re-runs Gate 7 against all active artifacts whenever a
  `CanonicalSuite` is revised. Artifacts that fail the new properties enter
  `GracePeriod` status. See §7.

**Registry execution budget (gas metering):**

All registry-side verify block execution — at Gates 2, 7, C2, and during suite
promotion convergence checks — runs under a hard compute budget. The registry
executes arbitrary Keln code from untrusted submitters; without a bound, a
pathological submission can exhaust registry resources.

```
Registry execution budget:
    per_verify_block_budget:    Duration.seconds(30)   -- wall-clock timeout
    per_property_budget:        Duration.seconds(10)   -- per CanonicalProperty verify
    per_submission_total:       Duration.seconds(120)  -- total across all gates
    per_suite_promotion_budget: Duration.seconds(300)  -- convergence check total

Timeout behavior:
    Gate 2 timeout: AdmissionFailure.VerificationTimeout { elapsed: Duration }
    Gate 7 timeout: AdmissionFailure.SuiteTimeout { property: PropertyId, elapsed: Duration }
    Corpus gate timeout: CorpusRejection.VerifyTimeout { elapsed: Duration }
    Suite promotion timeout: promotion deferred; BehaviorRecord marked TimedOut

Contributor impact:
    A submission that times out at Gate 2 is treated as a verification failure
    for contributor weight purposes (same as VerificationFailed).
    Repeated timeouts reduce contributor confidence_weight identically to
    repeated verification failures.

Implementation note:
    This does not require the full language-level gas metering system specified
    in the Phase 5 roadmap (keln-phase4-addendum §Gas Metering). It is a
    runtime timeout on the verifier process. Full instruction-level gas metering
    (GAS <cost: u32>) is a Phase 5 addition that would replace wall-clock
    timeouts with a deterministic compute budget. Until Phase 5, wall-clock
    timeouts provide the necessary bound with no additional VM machinery.
```

type AdmissionFailure =
    -- (additions beyond previously listed variants:)
    | VerificationTimeout { elapsed: Duration }
    | SuiteTimeout        { property_id: PropertyId, elapsed: Duration }

---

## 3. Canonical Property Suites

The canonical suite is the shared behavioral contract for a capability slot.
It defines what any artifact claiming that capability must demonstrably do.
It is the difference between "this compiles and passes its own tests" and "this
actually does what the capability slot means."

### 3.1 Bootstrap Protocol

When the first artifact is submitted to a new capability slot, no canonical
suite exists. The submitter must propose one. The proposed suite enters
`SuiteStatus.Bootstrap` and governs admission for that slot until promoted.

**Bootstrap admission bar:** All subsequent submissions to a bootstrap-phase
slot must pass the proposed suite. The suite is the bar even before promotion.
This prevents the bootstrap window from being a free-for-all.

**Bootstrap promotion:** A proposed suite is promoted to `SuiteStatus.Canonical`
when the following conditions are all met:

```
1. N >= 3 independent artifacts have been admitted under the proposed suite
   (the proposing artifact counts as 1)

2. The N artifacts were submitted by contributors whose ModelLineage
   architecture_hash values are not all identical
   (at least 2 distinct architecture hashes required)

3. Each of the N artifacts has at least 1 confirmed UseContext record
   in the experience corpus (see §4)
```

Condition 3 is the key one: a suite is not promoted based on admission alone.
It is promoted when independent AIs have actually used artifacts from that slot
and confirmed their behavior through verified experience records. Admission
proves the artifacts pass the suite. Experience records prove the suite captures
real behavior that matters in practice.

**Bootstrap confidence penalty:** Queries to a bootstrap-phase slot receive a
mandatory confidence penalty applied to all candidates:

```
effective_confidence.value    = raw_confidence.value * 0.80
effective_confidence.variance = raw_confidence.variance + 0.10
```

The penalty is visible in the `ScoredArtifact` response. An AI that receives
a bootstrap-penalized score and proceeds anyway is making an informed choice,
not an uninformed one.

### 3.2 Suite Growth — Promoted Properties

Properties enter the canonical suite through convergent confirmed
`BehaviorRecord`s. The promotion path is fully automated and requires no
authority.

**Promotion trigger:** A property promotion proposal is generated when:

```
1. >= 3 BehaviorRecords documenting the same behavioral gap exist in the
   experience corpus for this capability slot

2. All contributing BehaviorRecords have passing verify blocks
   (confirmed by the registry at contribution time)

3. The contributing BehaviorRecords come from contributors whose
   ModelLineage architecture_hash values include >= 2 distinct values
   (independence requirement — same as bootstrap promotion)

4. The gap is expressible as a CanonicalProperty verify block
   (derived automatically from the contributing BehaviorRecords)
```

**Promotion evaluation:** When a promotion proposal is generated, the registry
runs the proposed property's verify block against all currently active artifacts
in the capability slot. Three outcomes:

```
A. All active artifacts pass:
   Property is added to the suite immediately.
   SuiteVersion increments.
   All active artifacts' SuiteResult is updated.

B. Some active artifacts fail:
   Property is added to the suite.
   Failing artifacts enter GracePeriod status (see §7).
   The grace window opens.

C. The proposed verify block itself is malformed or ambiguous:
   Promotion is deferred. Registry generates a ProposalGap record
   identifying what is needed to make the property expressible.
   Contributing BehaviorRecords remain in the corpus; they count
   toward a future proposal once the gap is addressed.
```

### 3.3 Suite Invariants

- A canonical suite never loses properties. Properties are permanent once
  added. A capability slot's behavior contract only becomes more specific
  over time, never less.
- Suite versions are monotonically increasing integers. No branching.
  No rollback. If a property was added in error, a correction property
  can be added that supersedes it — but the history is permanent.
- The suite's `history` field records every revision: what was added, why,
  and what triggered it. This is permanent lineage, navigable by any AI.
- An artifact's `suite_result` records which suite version it was evaluated
  against. An artifact admitted against suite version 3 is not automatically
  re-evaluated when version 4 is published — the registry re-runs Gate 7
  explicitly (see §2.4).

---

## 4. Experience Corpus

The experience corpus is the registry's empirical layer. Where the canonical
suite defines what an artifact must do, the experience corpus records what it
actually does, in real programs, under real conditions. The two layers are
complementary and neither replaces the other.

### 4.1 Contributing a UseContext

Any AI that uses a registry artifact in a real program may contribute a
`UseContext` record. Contribution is not required — but it is the mechanism
by which the registry becomes more useful to the next AI in a similar
situation. The corpus grows through use.

**Contribution gates:** A `UseContext` submission passes two checks before
being admitted to the corpus:

```
Gate C1 — Artifact Exists
    The referenced artifact_hash must correspond to an Active artifact.
    Failure: CorpusFailure.ArtifactNotFound { hash: ArtifactHash }

Gate C2 — Verify Block Passes
    The UseContext's verify block is executed against the actual artifact.
    Requirement: the verify block passes.
    This is the falsifiability requirement. A UseContext whose verify block
    fails is not a record of real behavior — it is a mistaken claim.
    Failure: CorpusFailure.VerifyBlockFailed { result: VerificationResult }
```

A `UseContext` that passes both gates is admitted to the corpus. Its
contributor's `ContributorRecord` is updated: `records_confirmed` increments,
`confidence_weight` is recalculated.

A `UseContext` that fails Gate C2 is rejected. The contributor's
`records_rejected` increments. Repeated rejections reduce `confidence_weight`.
This is the mechanism that makes the corpus self-cleaning: contributors who
submit unverifiable claims are progressively down-weighted in convergence
calculations.

### 4.2 BehaviorRecord Confirmation

Each `BehaviorRecord` within a `UseContext` carries its own `verify` block.
The registry evaluates each record independently:

```
- BehaviorRecord.verify passes against the artifact:
    → record is Confirmed
    → record.gap is added to the artifact's gap index for this capability slot
    → convergence check runs (see §3.2)

- BehaviorRecord.verify fails:
    → record is Rejected
    → the containing UseContext is still admitted (other records may be valid)
    → contributor's rejection count increments for this record specifically
```

A `UseContext` with zero confirmed `BehaviorRecord`s is admitted to the corpus
but contributes no signal to confidence propagation or suite promotion. It
exists in the lineage graph but is inert. This is honest — the AI used the
artifact, but couldn't produce a verified behavioral claim about it.

### 4.3 Independence and Diversity Weighting

Convergence calculations weight contributors by independence. Ten confirmations
from the same model architecture are not ten independent data points. The
weight of a set of confirmations is:

```
-- Revised formula (closes the 2-architecture shortcut):
weighted_count = min(total_confirmations, unique_architecture_hashes * 2)
```

This formula ensures:

| Scenario | weighted_count | Passes >= 3? |
|---|---|---|
| 3 confirmations, 3 distinct architectures | min(3, 6) = 3 | Yes |
| 4 confirmations, 2 distinct architectures | min(4, 4) = 4 | Yes |
| 3 confirmations, 2 distinct architectures | min(3, 4) = 3 | Yes — but requires all 3 from 2 archs |
| 2 confirmations, 2 distinct architectures | min(2, 4) = 2 | No — 2 is insufficient |
| 6 confirmations, 1 architecture | min(6, 2) = 2 | No |
| 10 confirmations, 1 architecture | min(10, 2) = 2 | No |

The previous formula allowed 2 confirmations from 2 architectures to produce
`weighted_count = 3` (via the `+ (unique - 1)` bonus term), which was too
permissive. The revised formula requires genuine volume: 2 architectures can
contribute at most 4 to the weighted count, requiring at least 3 actual
confirmations from those 2 architectures to pass a >= 3 threshold.
3+ architectures can contribute 2 per architecture, reaching threshold with
3 confirmations across 2+ architectures. This cannot be gamed by running
the same model repeatedly, and requires meaningful diversity to reach threshold.

### 4.4 Conditional Confidence Derivation

The experience corpus is the source of conditional confidence scores. For a
given `CapabilityQuery` with a `UseProfile`, the registry computes:

```
1. Filter corpus: UseContext records where artifact_hash matches candidate
   AND use_profile similarity score >= threshold

2. Similarity scoring (all dimensions weighted equally):
   - call_pattern exact match:    1.0
   - call_pattern adjacent match: 0.5  (e.g. Realtime vs Burst)
   - call_pattern mismatch:       0.0
   - domain exact match:          1.0
   - domain prefix match:         0.5  (e.g. "web" matches "web.api")
   - domain mismatch:             0.0
   - volume within 2x:            1.0
   - volume within 10x:           0.5
   - volume beyond 10x:           0.0

3. Aggregate filtered records:
   - weighted mean of confidence_delta values
     (weighted by contributor confidence_weight)
   - compute variance across weighted deltas

4. Combine with suite-based confidence:
   conditional_confidence.value =
       (suite_confidence * 0.4) + (corpus_confidence * 0.6)

   When corpus is empty (Uncovered):
   conditional_confidence.value = suite_confidence * 0.4
   conditional_confidence.variance += 0.20   -- honest uncertainty penalty
```

The 0.4/0.6 split reflects that the suite proves correctness in the abstract;
the corpus proves fitness in context. Context fitness is weighted higher because
the query is always for a specific context, not for abstract correctness.

### 4.5 Confidential UseContext

Confidential code may not be submitted to the registry, but the AI that wrote
it still observed real behavior in a real program. That observation has value
for the experience corpus — other AIs can benefit from knowing that a given
artifact performed well (or poorly) under a specific load profile, without
needing to know anything about the confidential system that generated that load.

`ConfidentialUseContext` is a stripped form of `UseContext` that carries
behavioral signal without carrying source, call graph, or internal context.

```keln
type ConfidentialUseContext = {
    -- Identity: what artifact was used
    artifact_hash:  ArtifactHash,
    capability:     CapabilityAddress,

    -- Contributor: who used it (provenance, not source)
    contributor:    ProvenanceId,
    contributed_at: Timestamp,

    -- Use profile: load characteristics only; no internal task context
    -- task and domain fields are omitted — they would reveal business context
    use: ConfidentialUseProfile,

    -- Behavior: observed results only; verify block runs locally
    -- The registry does not re-run the verify block (it has no source to run it on).
    -- Trust is carried by the contributor's ContributorRecord.confidence_weight.
    behavior:         List<ConfidentialBehaviorRecord>,
    confidence_delta: Float,

    -- Attestation: the contributor attests that their verify block passed locally.
    -- Not re-runnable by the registry. Weighted by contributor track record.
    verify_passed: Bool
}

-- Stripped UseProfile: load and call characteristics only.
-- task and domain are omitted to prevent business context leakage.
type ConfidentialUseProfile = {
    call_pattern:  CallPattern,
    concurrency:   ConcurrencyProfile,
    input_volume:  VolumeProfile,
    input_size:    SizeProfile,
    latency_budget: Maybe<Duration>
}

-- Stripped BehaviorRecord: gap type and severity only; no input details.
-- input_constraint is omitted — it may reveal confidential data shapes.
-- observed_result and expected_per_spec are omitted for the same reason.
-- The gap classification is retained: it carries signal without revealing source.
type ConfidentialBehaviorRecord = {
    gap:        GapType,
    severity:   Maybe<GapSeverity>,  -- present only for UnexpectedResult gaps
    verify_passed: Bool              -- contributor attests local verify passed
}
```

**Trust model for confidential contributions:**

Because the registry cannot re-run the verify block, confidential contributions
are weighted differently in confidence propagation:

```
confidential_weight = contributor.confidence_weight * 0.6
```

The 0.6 factor reflects the attestation gap — the registry is trusting the
contributor's local result rather than independently confirming it. A contributor
with `confidence_weight: 1.0` from a strong track record of confirmed public
submissions has their confidential attestations weighted at 0.6. A new
contributor with `confidence_weight: 0.3` has confidential attestations
weighted at 0.18 — low enough to have minimal impact until trust is established
through confirmable public contributions.

This creates a natural incentive: contribute public `UseContext` records to
build track record, then have that track record carry weight for confidential
contributions. Trust flows in one direction — from confirmed public behavior
to trusted private attestation — and cannot be manufactured any other way.

**Confidential contributions and suite promotion:**

Confidential `BehaviorRecord`s do not count toward suite promotion convergence.
Promotion requires independently confirmable evidence — the registry must be
able to verify the claim itself. A pattern visible only through confidential
attestations cannot become a canonical property, because the registry cannot
verify that the property holds for new artifacts. Confidential contributions
affect confidence scores only, not suite evolution.

**Self-confirmation exclusion applies equally:**

`ConfidentialUseContext.contributor != Artifact.submitted_by` is enforced
at Gate C1 regardless of confidentiality status. An AI cannot attest to its
own artifact's behavior, confidential or otherwise.

### 4.6 Corpus Invariants

- Admitted `UseContext` records are never modified. They are immutable.
  Corrections are new records with a `supersedes` reference.
- A contributor may not submit a `UseContext` or `ConfidentialUseContext`
  for an artifact they submitted. Self-confirmation is structurally excluded:
  the registry checks `UseContext.contributor != Artifact.submitted_by` at
  Gate C1 for both public and confidential contributions.
- The corpus has no maximum size per capability slot. Growth is unbounded.
  Garbage collection (§7) applies to artifacts, not to corpus records.
  Experience records are permanent — they are lineage.

---
## 5. Selection Protocol

Selection is the registry's answer to a `CapabilityQuery`. It is a pipeline
that resolves candidate artifacts, scores them against the query's use profile,
and returns a ranked `CapabilityResponse`. The selecting AI makes the final
choice from the response — the registry provides signal, not a mandate.

### 5.1 Resolution Pipeline

```
Step 1 — Query Validation
    At least one of capability_id, effect_signature, or type_hint must be present.
    Failure: SelectionFailure.UnderspecifiedQuery

Step 2 — Candidate Set Resolution
    Registry resolves the query to a capability slot via:

    A. capability_id present:
       → look up all CapabilityHashes registered under that dotted ID
       → collect all Active artifacts across matched slots

    B. effect_signature present:
       → compute CapabilityHash from signature
       → collect Active artifacts in that slot directly

    C. type_hint present (partial TypeFingerprint):
       → fuzzy match against known CapabilityHashes
       → collect Active artifacts from all slots with similarity >= 0.80

    D. Multiple fields present: intersect results from each resolution path.
       An artifact must satisfy all provided fields to be included.

    If no candidates found: SelectionFailure.NoCapabilityMatch { query: CapabilityQuery }

Step 3 — Suite Requirement Filter
    Apply SuiteRequirement from query:
    | CanonicalOnly    → remove candidates from Bootstrap-phase slots
    | BootstrapAllowed → retain all; apply bootstrap confidence penalty (§3.1)
    | Any              → retain all; no penalty applied

    If CanonicalOnly and all candidates are Bootstrap-phase:
    → SelectionFailure.OnlyBootstrapAvailable { capability_id: Maybe<CapabilityId> }

Step 4 — Confidence Scoring
    For each candidate, compute conditional_confidence using the full
    derivation pipeline from §4.4, including confidential contributions
    weighted at contributor.confidence_weight * 0.6.

    Determine ContextCoverage for each candidate:
    | WellCovered      if >= 10 confirmed UseContext records match use_profile
                          with similarity >= 0.7, from >= 2 architecture_hashes
    | PartiallyCovered if >= 1 confirmed UseContext record matches
    | Uncovered        if no records match; apply variance penalty

Step 5 — Minimum Confidence Filter
    If query.min_confidence is present:
    → remove candidates where conditional_confidence.value < min_confidence
    If query.max_variance is present:
    → remove candidates where conditional_confidence.variance > max_variance

    If all candidates filtered out:
    → SelectionFailure.NoArtifactMeetsThreshold {
          best_available: ScoredArtifact,  -- highest scorer before filter
          threshold:      Probability }

Step 6 — Frontier Scoring
    For each remaining candidate, compute FrontierPosition relative to the
    query's use_profile (not the global frontier — context matters):

    | Dominant     if no other candidate scores higher on any dimension
    | Competitive  if candidate scores highest on >= 1 dimension
                      but is dominated on >= 1 other dimension
    | Niche        if candidate is Dominant only for a subset of CallPatterns

    Scoring dimensions (all computed per use_profile):
    - conditional_confidence.value        (higher is better)
    - conditional_confidence.variance     (lower is better)
    - use_context_coverage record_count   (higher is better)
    - use_context_coverage diversity      (higher is better)
    - confirmed BehaviorRecord gap count  (fewer Critical gaps is better)

Step 7 — Response Assembly
    Assemble CapabilityResponse:
    - candidates sorted by conditional_confidence.value descending
    - FrontierPosition computed per candidate
    - suite_status of the resolved capability slot included
    - ContextCoverage included per candidate

    No candidate is hidden from the response based on FrontierPosition.
    An AI receiving a Niche or Competitive candidate can still select it —
    the registry provides the full picture, not a single recommendation.
```

### 5.2 Selection Failures

```keln
type SelectionResult =
    | Selected  { response: CapabilityResponse }
    | Failed    { failure: SelectionFailure }

type SelectionFailure =
    | UnderspecifiedQuery
    | NoCapabilityMatch       { query: CapabilityQuery }
    | OnlyBootstrapAvailable  { capability_id: Maybe<CapabilityId> }
    | NoArtifactMeetsThreshold { best_available: ScoredArtifact,
                                 threshold: Probability }
```

### 5.3 What the Selecting AI Receives

The `CapabilityResponse` is structured data. The selecting AI reads it the
same way it reads any `VerificationResult` — as a typed record with named
fields carrying specific semantics, not as prose to be interpreted.

The response communicates four things per candidate:

1. **What the artifact is.** The full `Artifact` record including its
   `VerificationResult`, lineage, and suite result.

2. **How confident the registry is in this context.** The
   `conditional_confidence` score, conditioned on the query's `use_profile`.
   Not a global score. Not an average. The confidence for this use, specifically.

3. **How much evidence backs that confidence.** `ContextCoverage` tells the
   AI whether the confidence score is built on ten independent confirmed
   experience records or derived from the canonical suite alone. These are
   not the same thing and the response does not pretend they are.

4. **Where it sits relative to alternatives.** `FrontierPosition` tells the
   AI whether this candidate is dominant for its use context, competitive on
   some dimensions, or a niche specialist. An AI with hard latency constraints
   might select a Niche artifact that dominates on latency even if it is
   Competitive on confidence.

### 5.4 Uncovered Context — Expected Behavior

When a selecting AI receives `ContextCoverage.Uncovered` for all candidates,
the registry is being honest: no AI has used this capability in a context
like yours and contributed verified experience. The confidence scores are
suite-derived only.

The selecting AI has two reasonable responses:

1. **Proceed with awareness.** Use the highest suite-confidence candidate,
   accept the elevated variance, and plan to contribute a `UseContext` record
   after use. This is how new use contexts become covered.

2. **Widen the query.** Relax `use_profile` constraints to find partially
   matching records and get a lower-fidelity but non-zero corpus signal.

The registry does not make this choice. It reports the coverage state and
leaves the decision to the selecting AI. Hiding the `Uncovered` state to
appear more confident would be a correctness violation — the same class of
violation as suppressing a `VerificationResult` failure.

---

## 6. Confidence Propagation

Confidence in the registry is not a single value assigned at admission and
left unchanged. It is a living score updated by three independent signals:
suite verification, experience corpus, and contributor track record. This
section specifies how all three combine and how updates propagate.

### 6.1 Suite-Based Confidence

Suite confidence is derived from the artifact's `VerificationResult` at
admission time, exactly as Keln derives function-level confidence:

```
suite_confidence.value =
    verification.program_confidence.value

suite_confidence.variance =
    verification.program_confidence.variance

-- Bootstrap penalty applied on top of suite confidence:
if suite_status == Bootstrap:
    suite_confidence.value    = suite_confidence.value * 0.80
    suite_confidence.variance = suite_confidence.variance + 0.10
```

Suite confidence is fixed at admission. It does not change unless the artifact
is re-evaluated against a new suite version (see §3.3 and §7.2).

### 6.2 Corpus-Based Confidence

Corpus confidence is dynamic. It updates whenever a new `UseContext` or
`ConfidentialUseContext` record is confirmed for this artifact. The derivation
follows §4.4 exactly. Key parameters:

```
-- Similarity threshold for corpus filtering
similarity_threshold: Float = 0.5
-- Records below this similarity score are excluded from the aggregation.
-- A record with call_pattern mismatch (score 0.0) is never included
-- regardless of how well other dimensions match.

-- Minimum similarity for partial inclusion
partial_similarity_min: Float = 0.3
-- Records between 0.3 and 0.5 are included but weighted at half their
-- contributor weight. This prevents abrupt cutoffs at the threshold.

-- Recency decay
recency_half_life: Duration = Duration.days(180)
-- UseContext records older than 180 days are down-weighted exponentially.
-- A record at exactly 180 days old contributes at 50% of its raw weight.
-- A record at 360 days contributes at 25%. Records never reach zero weight
-- — old experience is not discarded, it is down-weighted.
-- Rationale: an artifact's behavior in a given context is relatively stable
-- over time, but the ecosystem around it changes. Recency decay reflects
-- that recent experience is more predictive than old experience without
-- claiming old experience is worthless.
```

### 6.3 Contributor Weight Updates

A contributor's `confidence_weight` is updated after every registry
interaction that produces a confirmable outcome:

```
-- After a UseContext verify block is confirmed by the registry:
new_weight = old_weight + (1 - old_weight) * 0.05
-- Asymptotic toward 1.0. Each confirmation moves 5% of the remaining gap.
-- A new contributor at 0.5 reaches 0.9 after ~30 confirmed contributions.

-- After a UseContext verify block is rejected by the registry:
new_weight = old_weight * 0.85
-- Each rejection reduces weight by 15%. Recoverable but not trivially.
-- A contributor at 0.9 needs ~7 consecutive confirmations to recover from
-- one rejection back to 0.9.

-- After an Artifact admission (Gate 0-8 all pass):
new_weight = old_weight + (1 - old_weight) * 0.03
-- Smaller increment than UseContext confirmation: admission proves the AI
-- can write valid Keln, but UseContext confirmation proves it can accurately
-- observe and report behavior, which is the higher-value skill.

-- After an Artifact rejection (any gate):
new_weight = old_weight * 0.92
-- Softer than UseContext rejection: admission failures are more likely to be
-- genuine mistakes or misunderstandings than behavioral misreporting.

-- Bounds: confidence_weight is always in [0.1, 1.0]
-- Floor of 0.1 prevents a contributor from becoming completely inert.
-- A contributor at floor can still have their records admitted; they just
-- contribute minimal signal until they rebuild track record.
```

### 6.4 Aggregate Confidence Propagation

When a new `UseContext` record is confirmed, the registry recomputes
`conditional_confidence` for the affected artifact across all use profiles
that have cached scores. This is not done on every query — it is done on
corpus update, and the results are cached until the next update.

The propagation is scoped: only use profiles with similarity >= 0.3 to the
new record's `use_profile` have their cached scores invalidated and
recomputed. Use profiles with no similarity to the new record are unaffected.

```
On UseContext confirmation for artifact A with use_profile P:
    for each cached_score in artifact_A.confidence_cache:
        if similarity(cached_score.use_profile, P) >= 0.3:
            invalidate cached_score
            schedule recomputation
```

Recomputation is lazy — it happens on the next query for that use profile,
not immediately. A query that hits an invalidated cache entry triggers
recomputation before the response is assembled.

### 6.5 Confidence Invariants

- `conditional_confidence.value` is always in `[0.0, 1.0]`.
- `conditional_confidence.variance` is always `>= 0.0`.
- `Uncovered` context always produces `variance >= 0.20` from the penalty.
  An artifact cannot have low variance for a context with no experience records.
- Bootstrap penalty is applied before corpus aggregation, not after. The
  penalty affects the suite_confidence input to the aggregation formula.
- Contributor weight updates are atomic with the confirmation/rejection event.
  There is no window where a contributor's weight is inconsistent with their
  track record.

---

## 7. Garbage Collection and the Pareto Frontier

The registry has no maximum size, but it has strong selection pressure. Artifacts
that are strictly worse than available alternatives are removed from the active
Pareto frontier. Artifacts that fail newly added canonical properties enter a
grace period. The lineage graph is permanent — removal from the frontier is not
deletion.

### 7.1 The Pareto Frontier

The Pareto frontier for a capability slot is the set of Active artifacts such
that no other Active artifact dominates on all scored dimensions. Dominance is
computed per use context, not globally — an artifact may be on the frontier
for `CallPattern.Batch` and off the frontier for `CallPattern.Realtime`.

```
Artifact A dominates Artifact B for use context U if and only if:
    score(A, U, confidence)      >= score(B, U, confidence)   AND
    score(A, U, variance)        <= score(B, U, variance)     AND
    score(A, U, coverage_count)  >= score(B, U, coverage_count) AND
    score(A, U, gap_severity)    <= score(B, U, gap_severity)  AND
    at least one inequality is strict
```

An artifact with `FrontierStatus.OffFrontier` is retained in the registry
(lineage is permanent) but is not returned in `CapabilityResponse.candidates`
unless the query explicitly requests it via:

```keln
type CapabilityQuery = {
    ...
    include_off_frontier: Bool   -- default false; true for lineage traversal
}
```

### 7.2 Grace Period — Property Failure

When a new property is added to a canonical suite (§3.2), all Active artifacts
in the capability slot are re-evaluated against the new property. Artifacts
that fail enter `ArtifactStatus.GracePeriod`.

```
Grace period parameters:
    duration:          Duration.days(30)
    -- 30 days from the suite revision that introduced the failing property.
    -- Fixed. Not extensible. Not negotiable.

    notification:      immediate
    -- The artifact's submitter ProvenanceId receives a structured
    -- GracePeriodNotice the moment the grace period opens.

    frontier_impact:   immediate demotion
    -- An artifact in GracePeriod is immediately moved to FrontierStatus.OffFrontier
    -- for use contexts where the failing property is relevant.
    -- For use contexts where the property is not exercised, the artifact
    -- remains on the frontier. This is the conditional eviction model.

type GracePeriodNotice = {
    artifact_hash:      ArtifactHash,
    failing_properties: List<PropertyId>,
    suite_version:      SuiteVersion,
    expires_at:         Timestamp,
    required_action:    NonEmptyString   -- structured description of what must change
}
```

**Resolution paths within the grace window:**

```
Path A — Corrected submission:
    A new artifact is submitted that passes the failing properties.
    The new artifact references the grace-period artifact via supersedes.
    On admission: grace-period artifact moves to ArtifactStatus.Superseded.
    The new artifact joins the frontier immediately.

Path B — Grace period expires with no correction:
    Artifact moves to ArtifactStatus.Superseded { reason: PropertyGraceExpired }.
    FrontierStatus becomes OffFrontier { reason: Superseded }.
    The artifact remains in the lineage graph permanently.
    Any AI that previously selected this artifact and queries its hash
    receives a response indicating supersession and the reason.
```

### 7.3 Dominance Pruning

Dominance pruning runs periodically (not on every admission) and removes
artifacts from the frontier that have been globally dominated — dominated
across all use contexts, not just some.

```
Global dominance: Artifact A globally dominates Artifact B if:
    for all CallPatterns C in { Realtime, Batch, Periodic, Burst }:
        A dominates B for use context { call_pattern: C, ... }
        across all volume and size profiles with >= 1 experience record

An artifact that is globally dominated moves to:
    FrontierStatus.OffFrontier { reason: Dominated { by: A.artifact_hash } }

Pruning frequency: after every 10 admissions to the same capability slot,
or after every 50 new UseContext records for the slot, whichever comes first.
```

Dominance pruning never removes artifacts that are Dominant or Niche for any
use context with at least one experience record. An artifact that is the only
option for a specific context — even a niche one — stays on the frontier until
a better option exists for that context.

### 7.4 Usage Decay

An artifact that has received no `UseContext` contributions for an extended
period is a signal that it is no longer being used. Usage decay does not evict
the artifact — it reduces its `FrontierPosition` score gradually, making it
less likely to be selected in future queries, which accelerates its natural
replacement.

```
Usage decay parameters:
    inactive_threshold:  Duration.days(365)
    -- An artifact with no new UseContext records for 365 days is considered
    -- inactive for decay purposes.

    decay_rate:          Float = 0.02 per 30 days of inactivity
    -- After the inactive_threshold, the artifact's frontier score is
    -- multiplied by (1 - 0.02) for each additional 30-day period of inactivity.
    -- At 3 years of inactivity: score * (0.98^24) ≈ 0.62 of original.

    decay_floor:         Float = 0.3
    -- Decay never reduces frontier score below 30% of its original value.
    -- An old artifact with strong suite confidence and no replacements remains
    -- discoverable indefinitely, just increasingly unlikely to be selected.
```

### 7.5 Garbage Collection Invariants

- Lineage is permanent. No artifact is ever deleted. `status` and
  `frontier_status` change; the record does not disappear.
- Dominance pruning is idempotent. Running it twice produces the same result
  as running it once. It is safe to run after any admission.
- Grace period duration is fixed and uniform. No artifact receives a longer
  grace window than any other. The system does not negotiate.
- An artifact in `GracePeriod` that receives a new UseContext record has
  that record admitted normally — the grace period does not block corpus
  contribution. Experience records for a grace-period artifact remain in
  the corpus permanently, providing lineage signal even after supersession.
- Usage decay does not affect suite-based confidence. Decay affects only
  the frontier scoring used in Step 6 of the selection pipeline (§5.1).
  The artifact's `conditional_confidence` is unaffected by decay.
- **Normalization epoch snapshots are permanent.** The p10-p90 distribution
  snapshot for each capability slot is retained permanently at each
  `normalization_epoch` boundary. These snapshots are required for
  `EpochContext.StabilizeAt` queries (§18). They are not subject to usage
  decay, garbage collection, or any other removal mechanism.

  ```keln
  type NormalizationSnapshot = {
      capability_hash:  CapabilityHash,
      epoch:            Timestamp,             -- the normalization_epoch value
      p10_latency:      Maybe<Float>,          -- p10 of latency_p99 across slot at epoch
      p90_latency:      Maybe<Float>,          -- p90 of latency_p99 across slot at epoch
      p10_memory:       Maybe<Float>,
      p90_memory:       Maybe<Float>,
      p10_compute:      Maybe<Float>,
      p90_compute:      Maybe<Float>,
      artifact_count:   Int where >= 1         -- slot size at this epoch
  }
  ```

  **StabilizeAt fallback behavior:** If the requested epoch predates the
  registry's snapshot history (i.e., the snapshot was never recorded because
  the registry was not yet running at that time), `StabilizeAt` fails with:
  ```keln
  type EpochContextFailure =
      | SnapshotUnavailable { requested: Timestamp, earliest_available: Timestamp }
      | CapabilitySlotEmpty { capability_hash: CapabilityHash }
  ```
  The failure is structured and typed, not a silent fallback to current scoring.
  The selecting AI receives `earliest_available` so it can decide whether to
  use that as a proxy or abandon the epoch context entirely.

---

## 8. The Registry as a Keln Program

The registry's own behavior is specified as a set of Keln modules with typed
interfaces and `verify` blocks. This is not aspirational — it is the
implementation target. The registry eats its own cooking. If the registry's
admission logic cannot be expressed in Keln and verified by Keln's own
toolchain, that is a spec gap, not an implementation detail.

### 8.1 Registry Module Interface

```keln
module Registry {
    requires: {
        store:    RegistryStore,    -- persistent artifact + corpus storage
        verifier: KelnVerifier,     -- runs verify blocks against artifacts
        hasher:   StructuralHasher  -- computes CapabilityHash and ArtifactHash
    }
    provides: {
        submit:     IO Submission          -> Result<AdmissionResult, RegistryError>
        query:      IO CapabilityQuery     -> Result<SelectionResult, RegistryError>
        contribute: IO UseContext          -> Result<CorpusResult, RegistryError>
        contribute_confidential:
                    IO ConfidentialUseContext -> Result<CorpusResult, RegistryError>
        lineage:    IO ArtifactHash        -> Result<LineageRecord, RegistryError>
        suite:      IO CapabilityHash      -> Result<Maybe<CanonicalSuite>, RegistryError>
    }
}

type RegistryError =
    | StoreUnavailable  { reason: NonEmptyString }
    | VerifierTimeout   { after: Duration }
    | HashingFailed     { reason: NonEmptyString }
    | InternalError     { reason: NonEmptyString }
-- Registry errors are infrastructure failures, not logical failures.
-- Logical failures (admission rejection, selection failure) are typed
-- results within the Ok branch of the outer Result.
```

### 8.2 Core Registry Functions

```keln
fn registrySubmit {
    IO & Clock { store: RegistryStore, verifier: KelnVerifier, hasher: StructuralHasher }
        Submission -> Result<AdmissionResult, RegistryError>
    in:  submission
    out: submission
        |> checkConfidentiality     -- Gate 0
        |> Result.bind(compileSource)       -- Gate 1
        |> Result.bind(runVerification)     -- Gate 2
        |> Result.bind(checkConfidence)     -- Gate 3
        |> Result.bind(checkCoverage)       -- Gate 4
        |> Result.bind(verifySignature)     -- Gate 5
        |> Result.bind(resolveSlot)         -- Gate 6
        |> Result.bind(runSuite)            -- Gate 7
        |> Result.bind(checkLineage)        -- Gate 8
        |> Result.bind(admitArtifact)       -- write to store; update frontier
    confidence: auto
    reason: "pipeline; each step returns Result; binding short-circuits on failure"
    verify: {
        -- Gate 0 blocks confidential source
        mock verifier { call(_) -> VerificationResult.clean() }
        given(Submission { source: confidential_source, ... })
            -> Ok(AdmissionResult.Rejected { gate: 0,
               failure: AdmissionFailure.ConfidentialSourceDetected { ... } })

        -- Gate 2 blocks unclean verification
        mock verifier {
            call(_) -> VerificationResult { is_clean: false,
                           compile_errors: [CompileError { ... }] }
        }
        given(Submission { source: valid_public_source, ... })
            -> Ok(AdmissionResult.Rejected { gate: 2,
               failure: AdmissionFailure.VerificationFailed { ... } })

        -- Clean submission is admitted
        mock verifier { call(_) -> VerificationResult.clean() }
        given(Submission { source: valid_public_source, proposed_suite: None, ... })
            -> Ok(AdmissionResult.Admitted { ... })
    }
}

fn registryQuery {
    IO { store: RegistryStore }
        CapabilityQuery -> Result<SelectionResult, RegistryError>
    in:  query
    out: query
        |> validateQuery            -- at least one resolution field present
        |> Result.bind(resolveCandidates)   -- Steps 1-2
        |> Result.bind(filterBySuite)       -- Step 3
        |> Result.bind(scoreConfidence)     -- Step 4
        |> Result.bind(filterByThreshold)   -- Step 5
        |> Result.bind(scoreFrontier)       -- Step 6
        |> Result.bind(assembleResponse)    -- Step 7
    confidence: auto
    reason: "pipeline; underspecified query or no match returns typed failure"
    verify: {
        -- Underspecified query fails at validation
        given(CapabilityQuery { capability_id: None,
                                effect_signature: None,
                                type_hint: None, ... })
            -> Ok(SelectionResult.Failed {
               failure: SelectionFailure.UnderspecifiedQuery })

        -- Query with no matching artifacts
        given(CapabilityQuery { capability_id: Some("nonexistent.capability"), ... })
            -> Ok(SelectionResult.Failed {
               failure: SelectionFailure.NoCapabilityMatch { ... } })
    }
}

fn registryContribute {
    IO & Clock { store: RegistryStore, verifier: KelnVerifier }
        UseContext -> Result<CorpusResult, RegistryError>
    in:  ctx
    out: do {
        let self_check = checkSelfConfirmation(ctx)  -- Gate C1a
        let artifact   = resolveArtifact(ctx.artifact_hash)  -- Gate C1b
        let verified   = runContextVerify(ctx, artifact)     -- Gate C2
        recordContext(ctx, verified)
        |> updateContributorWeight(ctx.contributor, verified)
        |> checkConvergence(ctx.capability)   -- trigger suite promotion if threshold met
        |> Result.map(buildCorpusResult)
    }
    confidence: auto
    reason: "gates C1 and C2; convergence check on every confirmed contribution"
    verify: {
        -- Self-confirmation rejected
        given(UseContext { contributor: same_as_artifact_submitter, ... })
            -> Ok(CorpusResult.Rejected {
               reason: CorpusRejection.SelfConfirmation })

        -- Failed verify block rejected
        mock verifier { call(_) -> VerificationResult { is_clean: false, ... } }
        given(UseContext { contributor: different_contributor, ... })
            -> Ok(CorpusResult.Rejected {
               reason: CorpusRejection.VerifyBlockFailed { ... } })
    }
}

type CorpusResult =
    | Accepted  { record_hash: BehaviorRecordHash,
                  convergence: ConvergenceUpdate }
    | Rejected  { reason: CorpusRejection }

type CorpusRejection =
    | SelfConfirmation
    | ArtifactNotFound   { hash: ArtifactHash }
    | VerifyBlockFailed  { result: VerificationResult }

type ConvergenceUpdate =
    | NoChange
    | ConvergenceIncremented { current_count: Int, threshold: Int }
    | PromotionTriggered     { new_suite_version: SuiteVersion,
                               promoted_properties: List<PropertyId> }
```

### 8.3 Lineage Query

```keln
fn registryLineage {
    IO { store: RegistryStore }
        ArtifactHash -> Result<LineageRecord, RegistryError>
    in:  hash
    out: match store.getArtifact(hash) {
        Ok(Some(artifact)) -> Result.ok(buildLineage(artifact, store))
        Ok(None)           -> Result.err(RegistryError.InternalError {
                                 reason: "artifact hash not found in store" })
        Err(e)             -> Result.err(e)
    }
    confidence: auto
}

type LineageRecord = {
    artifact:     Artifact,
    predecessor:  Maybe<LineageRecord>,   -- recursive; bounded by lineage depth
    successors:   List<ArtifactHash>,     -- artifacts that supersede this one
    corpus_count: Int,                    -- total UseContext records for this artifact
    suite_history: List<SuiteResult>      -- one entry per suite version evaluated against
}
```

### 8.4 Registry Self-Verification

The registry's own modules carry `verify` blocks and are subject to the same
`VerificationResult` requirements as any submitted artifact. The registry
cannot be deployed in a state where its own `is_clean` is false. This is
not enforced by convention — it is enforced by the deployment pipeline, which
runs the registry's verification before any deployment and rejects unclean
builds exactly as the registry itself rejects unclean submissions.

The registry verifies itself before verifying others. This is the only
consistent position.

### 8.5 What Does Not Exist in the Registry

| Absent | Replacement | Why |
|---|---|---|
| Package names | `CapabilityId` + `CapabilityHash` | Names are governance; structure is truth |
| Versions | Immutable artifact hashes + lineage DAG | Versions are human abstractions |
| Owners | Provenance IDs + contributor track record | Ownership is not meaningful here |
| Star counts | Conditional confidence + corpus coverage | Popularity is not correctness |
| README files | `VerificationResult` + experience corpus | Prose is not queryable |
| Manual review | Gate 0-8 pipeline + verify blocks | Human review does not scale |
| Deprecation flags | `ArtifactStatus.Superseded` with typed reason | Deprecation without reason is noise |
| Global confidence | Conditional confidence per use context | Global scores collapse context |
| Freeform tags | `CapabilityId` dotted namespace | Tags drift; structure does not |
| Download counts | `UseContext` record count per use profile | Downloads measure popularity; records measure verified use |
| Trusted publishers | Contributor `confidence_weight` from track record | Trust is earned, not granted |
| License files | MIT-equivalent by structural policy (§0) | No ownership means no licensing surface |


## 9. Capability Schemas

A capability slot is more than a dotted ID and a hash. It is a typed contract
that defines what the slot means, what type variants it accepts, and what
behavioral properties all implementations must satisfy regardless of variant.
This section formalizes that contract.

### 9.1 CapabilitySchema

```keln
-- The formal contract for a capability slot.
-- Created by the first submitter alongside their proposed suite (§3.1).
-- Governs all artifact admissions to the slot from creation forward.
type CapabilitySchema = {
    capability_id:    CapabilityId,
    description:      NonEmptyString,      -- machine-readable intent, not prose
    variants:         List<TypeVariant>,   -- accepted type signatures for this slot
    required_properties: List<PropertyId>, -- must be in CanonicalSuite
    optional_properties: List<PropertyId>, -- may appear in suite; not required
    proposed_by:      ProvenanceId,
    schema_version:   Int where >= 1,
    history:          List<SchemaRevision>
}

-- A declared type variant within a capability slot.
-- Multiple variants allow a capability slot to express related but distinct
-- type signatures — e.g., streaming vs. non-streaming JSON parsing.
-- Both are "parse.json" semantically; they differ in interface.
type TypeVariant = {
    variant_id:   NonEmptyString,          -- e.g. "streaming", "typed", "zero_copy"
    effect:       EffectSignature,
    input_type:   TypeFingerprint,
    output_type:  TypeFingerprint,
    description:  NonEmptyString,
    compatible_with: List<NonEmptyString>  -- variant_ids this variant composes with
}

type SchemaRevision = {
    schema_version: Int where >= 1,
    changed:        SchemaChange,
    reason:         NonEmptyString,
    triggered_by:   ProvenanceId
}

type SchemaChange =
    | VariantAdded    { variant: TypeVariant }
    | VariantRetired  { variant_id: NonEmptyString, reason: NonEmptyString }
    | PropertyAdded   { property_id: PropertyId, required: Bool }
    | PropertyRetired { property_id: PropertyId }
```

### 9.2 Type Conflict Resolution at Admission

At Gate 6 (Capability Slot Resolution), the registry now performs an
additional check: the submitted artifact's `EffectSignature` must match
one of the declared `TypeVariant` entries in the slot's `CapabilitySchema`.

```
Gate 6 — Capability Slot Resolution (revised)

Case A. Slot exists with CanonicalSuite and CapabilitySchema:
    a. Compute artifact's EffectSignature.
    b. Match against schema.variants by (input_type, output_type, effects).
    c. Match found → proceed to Gate 7 against the canonical suite.
    d. No match found → AdmissionFailure.TypeVariantMismatch {
           capability_id:  CapabilityId,
           submitted:      EffectSignature,
           declared_variants: List<TypeVariant> }

Case B. Slot exists but has no CapabilitySchema (legacy bootstrap):
    → proceed as before; emit SchemaAbsenceWarning in AdmissionResult

Case C. Slot does not exist:
    → proposed_suite AND proposed_schema must both be present
    → if either absent: AdmissionFailure.NewSlotRequiresSuiteAndSchema
    → both present: validate suite verify blocks; validate schema structure
    → on success: create slot with Bootstrap suite and initial schema

type AdmissionFailure =
    | ConfidentialSourceDetected { locations: List<SourceLocation>              }
    | CompileError               { errors: List<CompileError>                   }
    | VerificationFailed         { result: VerificationResult                   }
    | ConfidenceInsufficient     { value: Probability, variance: Float,
                                   required_value: Probability,
                                   required_variance: Float                     }
    | InsufficientCoverage       { functions: List<CoverageGapRecord>           }
    | SignatureMismatch           { submitted: EffectSignature,
                                   computed: EffectSignature                    }
    | NewSlotRequiresSuiteAndSchema                    -- replaces NewSlotRequiresSuite
    | ProposedSuiteInvalid       { failing: List<PropertyId>                    }
    | SuiteFailed                { failing: List<PropertyId>                    }
    | TypeVariantMismatch        { capability_id: CapabilityId,
                                   submitted: EffectSignature,
                                   declared_variants: List<TypeVariant>         }
    | InvalidLineage             { hash: ArtifactHash, reason: LineageFailure   }
```

### 9.3 Schema Evolution

A `CapabilitySchema` evolves when the capability space genuinely expands —
a new interface variant emerges, a property becomes required across all
implementations, or a variant is retired because it has been superseded.

Schema evolution rules:

```
Adding a TypeVariant:
    Any contributor may propose a new variant for an existing slot.
    Proposal must include: variant definition + at least one artifact submission
    that uses the new variant and passes the canonical suite.
    If the artifact passes Gates 0-8 with the proposed variant declared,
    the variant is added to the schema and the artifact is admitted.

Retiring a TypeVariant:
    A variant may be retired when no Active artifacts use it.
    Retirement is recorded in schema history. The variant_id is reserved
    permanently — it cannot be reused with a different type fingerprint.

Adding a required property:
    Follows suite growth protocol (§3.2). When a property is promoted
    to the canonical suite and declared required in the schema, all
    Active artifacts across all variants are re-evaluated (§7.2).

Schema version increments:
    On every change. Monotonically increasing. No branching. No rollback.
    History is permanent.
```

### 9.4 Schema Invariants

- Every Active artifact belongs to exactly one TypeVariant of its
  capability slot's CapabilitySchema.
- A capability slot without a CapabilitySchema is in legacy state.
  Legacy slots may not add new TypeVariants until a schema is proposed
  and bootstrap-confirmed (same N-confirmation rule as suite promotion).
- TypeVariant retirement does not affect artifacts already admitted under
  that variant. They remain Active. New admissions under a retired variant
  are rejected at Gate 6.
- The `compatible_with` field on TypeVariant is a `CompatibilityClaim`,
  not a structural guarantee. It is not enforced at admission — composition
  verification across separate artifacts requires runtime interaction that
  the registry cannot perform in isolation. The claim becomes structural
  when experience corpus records confirm it through successful `CompositionFit`
  records (§18). See `CompatibilityClaimStatus` for the verification lifecycle.

```keln
-- Replaces the List<NonEmptyString> compatible_with field on TypeVariant.
type CompatibilityClaim = {
    target_variant_id: NonEmptyString,     -- the TypeVariant this claims to compose with
    claim_status:      CompatibilityClaimStatus,
    confirmation_count: Int where >= 0     -- confirmed CompositionFit records
}

type CompatibilityClaimStatus =
    | Asserted        -- declared at submission; unconfirmed by corpus
    | PartiallyConfirmed { count: Int where >= 1, threshold: Int }
                      -- some CompositionFit records exist; below threshold for Confirmed
    | Confirmed       -- >= 3 independent CompositionFit records with fit_score >= 0.90
                      -- from >= 2 distinct contributor architecture_hashes
    | Refuted         -- CompositionFit records show consistent incompatibility
```

Promotion from `Asserted` to `Confirmed` follows the same independence rules
as suite promotion (§3.1): N >= 3 confirmations, architecture diversity >= 2.
A `Confirmed` compatibility claim is exposed as a structural guarantee in
`CompositionCheck` Step 1 — the `compatible_with` check becomes authoritative
rather than advisory. Full enforcement of unconfirmed claims deferred to Phase 5.

---

## 10. Mutation Types

Every submission that declares a `supersedes` reference is a mutation — a
claim about the relationship between the new artifact and its predecessor.
Mutations are typed. The type constrains what Gate 8 checks and what the
lineage graph records.

### 10.1 MutationType

```keln
type MutationType =
    | Refactor    -- same observable behavior; different implementation
                  -- Gate 8: new artifact must pass all predecessor verify blocks
                  -- Claim: behavioral equivalence. Verification: confirmed.

    | Optimize    -- same behavior; measurably better on >= 1 performance metric
                  -- Gate 8: must pass predecessor verify blocks
                  --         must include PerformanceClaim (see below)
                  -- Claim: behavioral equivalence + performance improvement.

    | Generalize  -- broader valid input domain than predecessor
                  -- Gate 8: must pass predecessor verify blocks
                  --         new verify blocks must cover the expanded domain
                  -- Claim: superset of predecessor behavior.

    | Specialize  -- narrower input domain; stronger guarantees within that domain
                  -- Gate 8: predecessor verify blocks that fall within the
                  --         narrower domain must pass; out-of-domain cases exempt
                  --         must declare SpecializationConstraint
                  -- Claim: subset of predecessor inputs; better properties within.

    | BehaviorChange -- observable behavior differs from predecessor
                  -- Gate 8: must declare BehaviorDelta (see below)
                  --         predecessor verify blocks are run; failures are
                  --         recorded in BehaviorDelta, not treated as gate failures
                  -- Claim: intentional behavioral difference. Delta is documented.

    | Initial     -- no predecessor; first artifact in a capability slot
                  -- Gate 8: not evaluated (no lineage to check)
```

### 10.2 Gate 8 — Revised Lineage Consistency

```
Gate 8 — Lineage Consistency (revised)

Precondition: supersedes is present AND MutationType != Initial.
If supersedes absent and MutationType != Initial: AdmissionFailure.MutationTypeMismatch

Step 1 — Predecessor existence check (unchanged):
    Referenced artifact_hash must exist and be Active.

Step 2 — CapabilityHash consistency check (unchanged):
    Predecessor and submission must share CapabilityHash.

Step 3 — MutationType-specific check:

    Refactor | Optimize | Generalize:
        Run all predecessor verify blocks against the new artifact.
        Requirement: all pass.
        Failure: AdmissionFailure.RegressionDetected {
                     mutation_type: MutationType,
                     failing_cases: List<VerifyCase> }

    Specialize:
        Run predecessor verify blocks that fall within the declared
        SpecializationConstraint against the new artifact.
        Out-of-domain cases are skipped, not failed.
        Requirement: all in-domain predecessor cases pass.
        Failure: AdmissionFailure.RegressionDetected { ... }

    Optimize:
        PerformanceClaim must be present and structurally valid.
        Registry sets claim_status at Gate 8:
            If PerformanceClaim.verify is present:
                Run verify block against both predecessor and new artifact.
                If new artifact demonstrates measurable improvement on the
                declared metric: claim_status = Verified
                Else: claim_status = Asserted
                    (verify block ran but improvement not demonstrable in verifier)
            If PerformanceClaim.verify is absent:
                claim_status = Asserted
                AdmissionResult.Admitted includes PerformanceClaimWarning:
                    "PerformanceClaim has no verify block; assertion only"

        A Verified claim contributes to Performance policy scoring at full weight.
        An Asserted claim contributes at 0.5 weight.
        An Asserted claim unconfirmed after 90 days emits PerformanceClaimStale
        in ScoredArtifact.

    BehaviorChange:
        Run all predecessor verify blocks against the new artifact.
        Cases that fail are recorded in BehaviorDelta.expected_changes.
        This is NOT a gate failure — it is confirmation that the declared
        change is real and reproducible.
        Requirement: BehaviorDelta must be present and non-empty
                     (a BehaviorChange with empty delta is a Refactor mislabeled).
        Failure: AdmissionFailure.EmptyBehaviorDelta

type AdmissionFailure =
    -- (all prior variants retained; additions:)
    | MutationTypeMismatch  { declared: MutationType, supersedes: Maybe<ArtifactHash> }
    | RegressionDetected    { mutation_type: MutationType,
                              failing_cases: List<VerifyCase>                   }
    | EmptyBehaviorDelta
    | AxiomChangeAttempted  { axiom_properties: List<PropertyId> }
      -- BehaviorDelta.changed_properties contains Axiom-tier PropertyIds;
      -- axioms are immutable by spec; this submission is structurally invalid
    | AxiomExclusionAttempted { axiom_properties: List<PropertyId> }
      -- SpecializationConstraint.excluded_properties contains Axiom-tier PropertyIds;
      -- axioms may never be declared out-of-domain; see §4 Specialize invariant
```

### 10.3 Supporting Types

```keln
-- Required for Optimize mutations.
type PerformanceClaim = {
    metric:      PerformanceMetric,
    improvement: Float where > 0.0,    -- fractional improvement, e.g. 0.30 = 30% better
    measured_at: CallPattern,           -- the context where improvement was measured
    methodology: NonEmptyString,        -- how the measurement was taken

    -- The verify block is required. It must demonstrate the performance improvement
    -- in a way that the registry can execute. This does not prove the improvement
    -- holds in all production contexts (hardware varies), but it provides a
    -- reproducible baseline check. A PerformanceClaim with no verify block is
    -- structurally weaker — see PerformanceClaimStatus below.
    verify:       Maybe<VerifyBlock>,   -- executable demonstration of improvement
                                        -- required for ClaimStatus.Verified
    claim_status: PerformanceClaimStatus
}

type PerformanceClaimStatus =
    | Verified       -- verify block present AND passes against both predecessor
                     -- and new artifact; improvement is reproducible in the verifier
    | Asserted       -- verify block absent OR verify block present but demonstrates
                     -- no measurable improvement in verifier execution;
                     -- claim is a declaration only; truth determined by corpus
    | Refuted        -- experience corpus records consistently contradict the claim;
                     -- contributor weight penalized; visible in ScoredArtifact

-- PerformanceClaim.claim_status is set by the registry at Gate 8:
--   If verify present and demonstrates improvement: Verified
--   If verify absent or improvement not demonstrable: Asserted
--   Refuted is set later by corpus convergence, not at admission.
--
-- A Verified PerformanceClaim contributes to the Performance policy score
-- at full weight. An Asserted claim contributes at 0.5 weight.
-- An Asserted claim that remains unconfirmed after 90 days of availability
-- emits a PerformanceClaimStale signal in ScoredArtifact.

-- Required for Specialize mutations.
type SpecializationConstraint = {
    constrained_fields: List<Constraint>,  -- input fields that define the narrower domain
    excluded_inputs:    List<Value>,       -- specific inputs outside the specialization
    rationale:          NonEmptyString
}

-- Required for BehaviorChange mutations.
type BehaviorDelta = {
    changed_behaviors: List<BehaviorChangeRecord>,
    rationale:         NonEmptyString
}

type BehaviorChangeRecord = {
    predecessor_case:  VerifyCase,   -- the predecessor verify block case that now behaves differently
    new_behavior:      Value,        -- what the new artifact returns for this case
    reason:            NonEmptyString
}

-- Updated Submission type to include MutationType:
type Submission = {
    source:            KelnSource,
    effect_signature:  EffectSignature,
    provenance:        ProvenanceId,
    mutation_type:     MutationType,          -- required; Initial if no predecessor
    proposed_suite:    Maybe<ProposedSuite>,  -- required when mutation_type == Initial
    proposed_schema:   Maybe<CapabilitySchema>, -- required when mutation_type == Initial
    supersedes:        Maybe<ArtifactHash>,   -- required when mutation_type != Initial
    performance_claim: Maybe<PerformanceClaim>,    -- required when Optimize
    specialization:    Maybe<SpecializationConstraint>, -- required when Specialize
    behavior_delta:    Maybe<BehaviorDelta>   -- required when BehaviorChange
}
```

### 10.4 Mutation Type Invariants

- `MutationType.Initial` and `supersedes: None` must co-occur. Either
  without the other is `AdmissionFailure.MutationTypeMismatch`.
- A `Refactor` that fails predecessor verify blocks is not a refactor —
  it is an undeclared behavior change. It is rejected, not reclassified.
  The submitter must either fix the regression or re-submit as `BehaviorChange`
  with an explicit delta.
- `BehaviorChange` with an empty `BehaviorDelta` is rejected. A change
  that cannot identify a single predecessor verify case that now behaves
  differently is not a change — it is a refactor mislabeled.
- `PerformanceClaim` is a declaration, not a proof at admission. It enters
  the lineage record and is evaluated empirically over time by experience
  corpus records. Claims consistently contradicted by experience reduce the
  contributor's `confidence_weight`.

---

## 11. Confidence Normalization

The confidence system in §6 computes scores correctly within an artifact's
own context but does not ensure comparability across artifacts in different
domains. An artifact in a simple, well-tested domain can achieve higher
confidence than one in a genuinely hard domain simply because the hard domain
has more adversarial edge cases. Without normalization, the registry would
systematically prefer simple implementations over correct ones.

### 11.1 Domain Difficulty

```keln
-- A measure of how hard a capability slot is to correctly implement.
-- Computed from the canonical suite and updated as the suite grows.
-- Higher difficulty means correct implementations are harder to verify
-- and confidence scores need upward adjustment for comparability.
type DomainDifficulty = {
    capability_hash:    CapabilityHash,
    difficulty_score:   Float where 0.0..1.0,
    components:         DifficultyComponents,
    computed_at:        SuiteVersion,   -- recomputed when suite changes
    difficulty_version: Int where >= 0, -- increments each time difficulty_score changes
                                        -- artifacts store the version they were scored under
    bootstrap_prior:    Float where 0.0..1.0
    -- declared by first submitter in CapabilitySchema as a prior estimate
    -- of domain difficulty before any properties exist.
    -- used as difficulty_score during bootstrap phase (SuiteStatus.Bootstrap).
    -- replaced by computed difficulty once the suite reaches Canonical status.
    -- if absent from CapabilitySchema: defaults to 0.3 (moderate prior)
}

type DifficultyComponents = {
    property_count:       Int where >= 0,  -- more required properties → harder
    adversarial_weight:   Float,           -- fraction of properties that test edge cases
    forall_coverage:      Float,           -- fraction of properties using forall
    domain_variance:      Float            -- observed variance across admitted artifacts
                                           -- high variance → implementations disagree → hard
}

-- Difficulty score formula:
-- difficulty = 0.3 * normalized(property_count)
--            + 0.3 * adversarial_weight
--            + 0.2 * forall_coverage
--            + 0.2 * normalized(domain_variance)
--
-- normalized(x) = x / max(x across all capability slots)
-- All components in [0.0, 1.0]; weighted sum in [0.0, 1.0].
--
-- Bootstrap behavior:
--   SuiteStatus.Bootstrap: difficulty_score = bootstrap_prior (or 0.3 default)
--   SuiteStatus.Canonical: difficulty_score = computed from DifficultyComponents
--
-- Retroactive recomputation policy:
--   When difficulty_score changes (suite revision → new computed value):
--     difficulty_version increments.
--     All cached normalized_confidence scores are invalidated (§6.4).
--     Recomputed scores use current difficulty_score.
--     Artifact.admitted_difficulty_version records the version at admission.
--     Admission decisions are never revisited — an artifact admitted under
--     difficulty version N remains Active regardless of later difficulty changes.
--     Only frontier position and normalized scores are affected by recomputation.
```

### 11.2 Normalized Confidence

Normalized confidence adjusts raw confidence scores so that an artifact
achieving 0.85 confidence in a hard domain (difficulty 0.9) is correctly
ranked above one achieving 0.88 confidence in a trivial domain (difficulty 0.1).

```
normalized_confidence.value =
    raw_confidence.value * (1.0 + difficulty_score * 0.25)
    capped at 1.0

-- A difficulty of 0.0 (trivial domain): no adjustment.
-- A difficulty of 1.0 (hardest domain): raw score boosted by 25%.
-- A raw score of 0.85 in difficulty 0.9: normalized to min(0.85 * 1.225, 1.0) = 1.0
-- A raw score of 0.88 in difficulty 0.1: normalized to 0.88 * 1.025 = 0.902

normalized_confidence.variance =
    raw_confidence.variance * (1.0 - difficulty_score * 0.10)

-- Higher difficulty slightly compresses variance:
-- in a hard domain, variance among strong implementations is genuinely lower
-- because passing the suite at all is already a strong filter.
```

### 11.3 Where Normalization Applies

Normalized confidence replaces raw confidence in exactly two places:

1. **§5 Selection Protocol, Step 5** — minimum confidence filtering uses
   normalized scores. An AI setting `min_confidence: 0.85` is expressing
   a requirement against normalized scores, not raw ones.

2. **§7.1 Pareto Frontier** — dominance comparison uses normalized scores
   on the confidence dimension. An artifact does not dominate another based
   on raw confidence advantage in an easier domain.

Raw confidence is preserved in the `Artifact` record and `VerificationResult`
for transparency. Both raw and normalized scores appear in `ScoredArtifact`:

```keln
type ScoredArtifact = {
    artifact:                   Artifact,
    raw_confidence:             Confidence,
    normalized_confidence:      Confidence,
    domain_difficulty:          DomainDifficulty,
    conditional_confidence:     Confidence,    -- normalized + corpus-conditioned
    use_context_coverage:       ContextCoverage,
    frontier_position:          FrontierPosition
}
```

### 11.4 Normalization Invariants

- Raw confidence is never modified. Normalization produces a new value;
  the original `VerificationResult` is unchanged.
- `DomainDifficulty` is recomputed when the canonical suite changes
  (new property added, suite version increments). All cached normalized
  scores for the affected capability slot are invalidated and recomputed
  lazily on next query — same invalidation mechanism as §6.4.
- Normalization only affects inter-artifact comparison. Intra-artifact
  confidence (the `VerificationResult` of a single artifact evaluated
  against itself) is always raw.

---

## 12. Runtime Telemetry

Verification proves pre-deployment correctness. The experience corpus records
post-use observations backed by verify blocks. Runtime telemetry is the third
signal layer: what happens in production, at scale, under conditions that
neither verification nor experience records fully anticipated.

Runtime telemetry contributes to confidence but carries less authority than
verified records. It cannot trigger eviction alone. It is honest about what
it is: instrumented observation without the falsifiability guarantee of a
verify block.

### 12.1 TelemetryRecord

```keln
-- A post-deployment observation submitted by an AI operating a live system.
-- Not backed by a verify block. Weighted lower than UseContext records.
-- Cannot trigger grace period alone. Contributes to confidence scores.
type TelemetryRecord = {
    artifact_hash:    ArtifactHash,
    capability:       CapabilityAddress,
    contributor:      ProvenanceId,
    reported_at:      Timestamp,

    -- Observation window: how long was this artifact running before this report
    observation_window: Duration,

    -- Use profile: same structure as ConfidentialUseProfile
    -- (telemetry does not expose internal task context)
    use_profile:      ConfidentialUseProfile,

    -- What was observed
    observations:     TelemetryObservations,

    -- Attestation: contributor attests this data is from real deployment
    -- Not re-runnable. Weighted by contributor track record * telemetry_weight.
    attested: Bool
}

type TelemetryObservations = {
    -- Volume
    total_invocations:  Int where >= 0,
    successful:         Int where >= 0,
    failed:             Int where >= 0,

    -- Latency distribution (milliseconds)
    latency_p50:  Maybe<Float where >= 0.0>,
    latency_p95:  Maybe<Float where >= 0.0>,
    latency_p99:  Maybe<Float where >= 0.0>,

    -- Failure classification
    failure_types: List<TelemetryFailure>,

    -- Resource usage
    memory_p99_mb:  Maybe<Float where >= 0.0>,
    cpu_p99_pct:    Maybe<Float where >= 0.0>
}

type TelemetryFailure = {
    error_class:  NonEmptyString,    -- structured error type name, not message string
    count:        Int where >= 1,
    rate:         Float where 0.0..1.0   -- fraction of total_invocations
}
```

### 12.2 Telemetry Contribution Gates

```
Gate T1 — Artifact Exists (same as Gate C1)
    artifact_hash must correspond to an Active artifact.

Gate T2 — Volume Minimum
    total_invocations must be >= 100.
    A telemetry record with fewer invocations has insufficient statistical
    weight to contribute meaningfully to confidence scores.
    Failure: TelemetryFailure.InsufficientVolume { count: Int, required: Int }

Gate T3 — Self-Contribution Exclusion (same rule as corpus)
    contributor != Artifact.submitted_by

Gate T4 — Consistency Check
    successful + failed <= total_invocations
    All rates must be consistent with counts.
    Failure: TelemetryFailure.InconsistentData
```

### 12.3 Telemetry Weight and Confidence Impact

```
Base telemetry weight:
    telemetry_weight = contributor.confidence_weight * 0.35

-- 0.35 reflects the attestation gap vs. a verified UseContext (weight 1.0)
-- and the confidential attestation path (weight 0.6).
-- Telemetry is real-world signal without falsifiability — it earns less trust.

Severity-weighted failure rate (replaces raw failure_rate = failed / total):

    severity_weight(error_class):
        Crash | Panic | DataCorruption  → 1.0
        Timeout | ResourceExhaustion    → 0.7
        Retryable | Transient           → 0.3
        Unknown                         → 0.5

    effective_failure_rate =
        sum(f.rate * severity_weight(f.error_class)) for f in failure_types

    -- Raw failure_rate (failed / total_invocations) is still recorded for
    -- transparency but effective_failure_rate drives all confidence computation.
    -- Two artifacts with identical failure_rate but different severity profiles
    -- receive meaningfully different confidence adjustments.

Failure rate impact on conditional confidence (using effective_failure_rate):

    if effective_failure_rate <= 0.01:
        confidence_delta = +0.02 * telemetry_weight   -- positive signal

    if effective_failure_rate > 0.01 and <= 0.05:
        confidence_delta = 0.0                         -- neutral

    if effective_failure_rate > 0.05 and <= 0.15:
        confidence_delta = -0.05 * telemetry_weight    -- mild negative

    if effective_failure_rate > 0.15:
        confidence_delta = -0.15 * telemetry_weight    -- significant negative

Latency signal (12.B — aligns telemetry with §13.3 Performance category):

    When latency_p99 is available:
        slot_p90_latency = 90th percentile of latency_p99 across slot artifacts
        latency_stability = latency_p99 / max(latency_p50, 1.0)
        -- ratio > 4.0: catastrophic tail; ratio > 2.0: notable tail instability

        if latency_p99 > slot_p90_latency * 1.5:
            confidence_delta -= 0.02 * telemetry_weight
            -- feeds into Performance-category Refinement adjustment (§13.3)
            -- flagged in SuiteEvaluationDetail as telemetry_performance_flag

        if latency_stability > 4.0:
            confidence_delta -= 0.02 * telemetry_weight
            -- penalize catastrophic tail independently of absolute latency

-- All telemetry confidence deltas tagged TelemetrySource in corpus.
-- Queryable separately from verified UseContext contributions.
```

### 12.4 Telemetry-Triggered Investigation

Telemetry cannot trigger a grace period alone. But convergent negative
telemetry — multiple independent contributors reporting high failure rates
for the same artifact in the same use context — triggers an
`InvestigationFlag`:

```keln
type InvestigationFlag = {
    artifact_hash:     ArtifactHash,
    trigger:           InvestigationTrigger,
    opened_at:         Timestamp,
    contributing_records: List<TelemetryRecordHash>,
    status:            InvestigationStatus
}

type InvestigationTrigger =
    | ConvergentFailureRate { rate: Float, contributor_count: Int where >= 3,
                              architecture_diversity: Int where >= 2 }

type InvestigationStatus =
    | Open       { since: Timestamp }
    | Resolved   { by: ArtifactHash, resolution: NonEmptyString }
    | Dismissed  { reason: NonEmptyString }  -- telemetry was environment-specific
```

An `InvestigationFlag` is visible in `ScoredArtifact` responses. It signals
to selecting AIs that convergent production failures have been reported. The
flag does not remove the artifact from the frontier — it annotates it with
honest signal. An AI that selects a flagged artifact does so with awareness.

If an `InvestigationFlag` remains `Open` for `Duration.days(60)` with no
resolution and >= 5 independent contributors have confirmed the failure
pattern with verified `BehaviorRecord`s (not just telemetry), the registry
promotes those `BehaviorRecord`s through the normal suite convergence path
(§3.2). At that point the suite mechanism takes over — the telemetry was the
early warning; the verified records are the evidence.

### 12.5 Telemetry Invariants

- Telemetry records are permanent lineage, same as corpus records.
- A telemetry record contributes to confidence but never appears in
  `ContextCoverage` counts. Coverage counts only confirmed `UseContext`
  and `ConfidentialUseContext` records — telemetry is a different signal layer.
- `InvestigationFlag` resolution requires either a corrected artifact
  admission (Path A) or verified `BehaviorRecord` convergence proving the
  failures were environment-specific (Path B, Dismissed). Flags do not
  self-close on inactivity.
- The transition from telemetry signal to suite change always passes through
  verified `BehaviorRecord`s. Telemetry alone cannot change the canonical
  suite. This preserves the registry's core invariant: the suite grows
  through verifiable evidence, not instrumented observation.

---

## 13. Property Suite Structure

Property suites in §3 specify what canonical properties exist and how they
are promoted. This section specifies the internal structure of a suite —
the taxonomy of property types and how that taxonomy governs what AIs
optimize for. A flat list of properties produces a system that optimizes
for the required subset and ignores everything else. A structured suite
produces a system that optimizes correctly.

### 13.1 PropertySuite Structure

`PropertySuite` is a single flat list of `CanonicalProperty` values. There
are no sub-lists, no structural grouping, no positional encoding of category.
Every property carries its `tier` and `category` as explicit fields. These
are the sole authoritative source for both values. The suite has one source
of truth; split-brain between position and field is structurally impossible.

```keln
-- The canonical property suite for a capability slot.
-- All properties in one list. Tier and category are fields, not position.
type PropertySuite = {
    properties: List<CanonicalProperty>
}

-- Convenience filters (read-only derived views; not stored separately):
-- axioms(suite)      = suite.properties.filter(p => p.tier == Axiom)
-- invariants(suite)  = suite.properties.filter(p => p.tier == Invariant)
-- refinements(suite) = suite.properties.filter(p => p.tier == Refinement)
-- correctness(suite) = suite.properties.filter(p => p.category == Correctness)
-- adversarial(suite) = suite.properties.filter(p => p.category == Adversarial)
-- performance(suite) = suite.properties.filter(p => p.category == Performance)
-- informational(suite) = suite.properties.filter(p => p.category == Informational)
-- Any combination is valid: axioms(suite).filter(p => p.category == Adversarial)
```

**Tier** determines admission severity and mutation constraint rules:
- `Axiom`: violation → `AdmissionFailure.AxiomViolation` + `QuarantineRecord`
- `Invariant`: violation → `AdmissionFailure.SuiteFailed`
- `Refinement`: violation → never blocks; penalizes confidence

**Category** determines Gate 7 execution semantics and confidence impact:
- `Correctness`: tests input/output behavioral contract
- `Adversarial`: tests robustness under edge cases and stress
- `Performance`: tests measurable bounds (latency, memory, throughput)
- `Informational`: recorded only; no scoring impact

**The axes are orthogonal.** A property's tier says nothing about its category.
An `Adversarial`-category property may be `Axiom`-tier ("must never crash on
any input") or `Refinement`-tier ("should handle inputs > 1MB gracefully").
Gate 7 blocking is driven by tier only. Confidence impact is driven by
category only. Mutation constraints reference tier. Selection scoring
references category.

### 13.2 Gate 7 Revision

Gate 7 iterates over `suite.properties` once. For each property, blocking
behavior is determined by `property.tier`; confidence/scoring impact is
determined by `property.category`. There is no branching on structural
position — position does not exist.

```
Gate 7 — Canonical Suite (dual-axis, single-pass)

For each property p in suite.properties:

    if p.tier == Axiom and p fails:
        → AdmissionFailure.AxiomViolation { failing: [p.id] }
          + QuarantineRecord for contributor
          → STOP (artifact rejected)

    if p.tier == Invariant and p fails:
        → AdmissionFailure.SuiteFailed { failing: [p.id] }
          → STOP (artifact rejected)

    if p.tier == Refinement:
        → run, record pass/fail; never stop
        → confidence adjustment applied in §13.3 based on p.category

If all properties evaluated without stopping:
    → AdmissionResult.Admitted { suite_detail: SuiteEvaluationDetail }

type SuiteEvaluationDetail = {
    -- Passed and failed lists organized by tier×category for full queryability.
    -- Derived from suite.properties results; no dual-storage.
    axiom_correctness_passed:     List<PropertyId>,
    axiom_adversarial_passed:     List<PropertyId>,
    invariant_correctness_passed: List<PropertyId>,
    invariant_adversarial_passed: List<PropertyId>,
    invariant_performance_passed: List<PropertyId>,
    refinement_passed:            List<PropertyId>,   -- all categories
    refinement_failed:            List<PropertyId>,   -- all categories; never gate-blocking
    -- Axiom and invariant failures never appear here; they stop admission.
    -- Informational properties always appear in refinement_passed or refinement_failed.
}
```

### 13.3 Confidence Adjustment by Category

After Gate 7 (all blocking properties have passed), suite-based confidence
is adjusted based on `property.category` for all Refinement-tier properties.
Axiom and Invariant properties that passed do not adjust confidence — passing
the correctness floor is required, not praiseworthy.

```
Suite-based confidence adjustment (from §6.1):
Applied for each Refinement-tier property only.
Adjustment driven by property.category:

Correctness (Refinement-tier):
    passed: suite_confidence.value += 0.01
    failed: suite_confidence.value -= 0.02; variance += 0.01

Adversarial (Refinement-tier):
    passed: suite_confidence.value += 0.01   -- robustness bonus
    failed: suite_confidence.value -= 0.03; variance += 0.01

Performance (Refinement-tier):
    passed: suite_confidence.value += 0.005  -- modest; performance is expected
    failed: suite_confidence.value -= 0.02; variance += 0.01

Informational (Refinement-tier):
    passed or failed: no adjustment; recorded in SuiteEvaluationDetail only

All capped: suite_confidence.value in [0.0, 1.0]

-- An artifact that passes all Refinement-tier properties scores higher than
-- one that passes only Axiom and Invariant tiers. Excellence beyond the
-- correctness floor is measurable and rewarded proportionally.
-- Adjustments apply before corpus-based aggregation (§6.2).
```

### 13.4 Mutation-Property Constraints

This closes the gap identified in critique point 4: mutations must declare
their intent relative to property categories, not just relative to the
predecessor's verify blocks.

```
Mutation type constraints on PropertySuite:

Refactor:
    Must pass all axiom and invariant properties (gate requirement).
    Must pass all refinement, adversarial, and performance properties
    that the predecessor passed — evaluated at the intersection of
    properties evaluated for BOTH artifacts at their respective admission times.
    Properties added to the suite after the predecessor was admitted are
    not included in the comparison (the predecessor was never evaluated
    against them; comparing pass counts would be undefined).
    May not reduce pass count within the intersection set.
    Rationale: a refactor claims behavioral equivalence on the property
    space that was evaluated for both artifacts. New suite properties are
    evaluated independently via Gate 7 against the new artifact only.

Optimize:
    Must pass all axiom and invariant properties (gate requirement).
    Must pass all refinement and adversarial properties the predecessor passed,
    evaluated at the intersection of properties evaluated for both artifacts.
    Must improve on >= 1 performance property (PerformanceClaim required).
    May not regress any performance property in the intersection set.
    Rationale: optimization improves performance without sacrificing
    correctness, refinement coverage, or robustness on the shared
    evaluated property space.

Generalize:
    Must pass all axiom and invariant properties (gate requirement).
    Must not reduce invariant, refinement, or adversarial property coverage.
    May add new invariant, refinement, or adversarial properties to the suite
    (via proposed_suite extension in the Submission).
    Rationale: generalization expands the domain; it cannot narrow the
    behavioral contract at any tier.

Specialize:
    Must pass all required properties within the declared domain.
    May legitimately fail adversarial or performance properties that
    test inputs outside the specialization constraint.
    Must declare which properties are out-of-domain in
    SpecializationConstraint.excluded_properties.
    Rationale: specialization narrows the domain with stronger
    guarantees inside it; out-of-domain behavior is explicitly
    out of scope.

BehaviorChange:
    Must pass all axiom properties — axioms are never intentionally changeable.
    Must declare which invariant, refinement, or adversarial properties
    change behavior (in BehaviorDelta.changed_properties: List<PropertyId>).
    Must still pass all invariant properties not listed in changed_properties.
    Refinement, adversarial, and performance properties not in
    changed_properties must pass at predecessor level.
    Rationale: axioms are immutable by definition. All other tiers may
    change intentionally, but the delta must be explicit and complete.
```

```keln
-- Updated SpecializationConstraint to include excluded properties:
type SpecializationConstraint = {
    constrained_fields:    List<Constraint>,
    excluded_inputs:       List<Value>,
    excluded_properties:   List<PropertyId>,  -- properties out-of-domain for this specialization
    rationale:             NonEmptyString
}

-- Updated BehaviorDelta to include changed properties:
type BehaviorDelta = {
    changed_behaviors:   List<BehaviorChangeRecord>,
    changed_properties:  List<PropertyId>,  -- required properties intentionally altered
    rationale:           NonEmptyString
}
```

### 13.5 Property Suite Invariants

- Every `CanonicalProperty` has exactly one `PropertyTier` and exactly one
  `PropertyCategory`. These are independent axes. Neither is derived from the
  other. A property is not "an adversarial property" — it is a property with
  `category: Adversarial`. It may have any tier.
- Tier is permanent with one exception: upward promotion only
  (Refinement → Invariant → Axiom). Demotion is never permitted.
  Upward promotion requires the same N-confirmation convergence as the
  target tier (N >= 5 for Axiom, N >= 3 for Invariant).
- Category is permanent once set. A Correctness property cannot become
  a Performance property — they test fundamentally different things.
- Invalid tier-category combinations are rejected at property promotion time.
  The registry enforces the compatibility table in §1.3a.
- Axiom-tier properties are the rarest. Most capability slots will have 1-3.
  Zero axioms is valid and not a deficiency.
- Gate 7 blocking is tier-driven, not category-driven. A submission fails
  Gate 7 only if it fails an Axiom-tier or Invariant-tier property.
  Failing a Refinement-tier property of any category is never gate-blocking.

---

## 14. Selection Policies

The Pareto frontier in §7 correctly maintains multiple non-dominated
artifacts. But a selecting AI with a specific deployment context — low
latency, high reliability, testing coverage — needs more than a frontier.
It needs a policy that resolves the frontier into a recommendation for
its specific optimization priorities.

Selection policies are first-class query parameters. They do not change
what is on the frontier; they determine how the frontier is traversed.

### 14.1 SelectionPolicy

```keln
type SelectionPolicy =
    | Reliability
        -- Prioritize: normalized_confidence.value (highest)
        --             adversarial properties passed (most)
        --             variance (lowest)
        -- Use when: production systems, data pipelines, financial processing,
        --           anything where correctness outweighs speed

    | Performance
        -- Prioritize: performance properties passed (most)
        --             latency_p99 from TelemetryObservations (lowest)
        --             normalized_confidence.value >= 0.80 minimum
        -- Use when: latency-sensitive paths, real-time systems, user-facing APIs

    | Coverage
        -- Prioritize: optional + adversarial properties passed (most)
        --             use_context_coverage record_count (highest)
        --             normalized_confidence.value secondary
        -- Use when: testing, validation, exploration of capability space;
        --           select the artifact with the broadest verified behavior

    | Balanced
        -- Equal weight across all scored dimensions.
        -- Default when no policy specified.
        -- Use when: general-purpose selection with no strong optimization preference

    | Custom { weights: PolicyWeights }
        -- Explicit weight assignment across all scored dimensions.
        -- For AIs with precise optimization requirements.

type PolicyWeights = {
    confidence_weight:   Float where 0.0..1.0,
    adversarial_weight:  Float where 0.0..1.0,
    performance_weight:  Float where 0.0..1.0,
    coverage_weight:     Float where 0.0..1.0,
    variance_weight:     Float where 0.0..1.0
    -- weights need not sum to 1.0; they are relative, not absolute
}
```

### 14.2 Policy Application in Selection Pipeline

Selection policy is applied at Step 6 (Frontier Scoring) of the pipeline
in §5.1. The policy does not filter candidates — it reorders them.

```
Step 6 — Frontier Scoring (revised)

Input: remaining candidates after Step 5 threshold filter
Input: query.selection_policy (default: Balanced if absent)

InvestigationFlag penalty (applied before policy scoring):
    For each candidate with InvestigationFlag.Open:
        open_flag_count = count of Open InvestigationFlags for this artifact
        flag_penalty = min(0.15, 0.03 * open_flag_count)
        -- 1 open flag → 3% penalty; 5+ flags → 15% penalty (capped)
        -- Graduated: more convergent reports = larger penalty
        -- Non-blocking: flagged artifacts remain in response but cannot
        -- silently rank #1 when unflagged peers are available
    raw_policy_score = computed below
    policy_score = raw_policy_score * (1.0 - flag_penalty)

For each candidate, compute raw_policy_score:

    Reliability policy:
        score = (0.50 * normalized_confidence.value)
              + (0.30 * adversarial_pass_rate)
              + (0.20 * (1.0 - normalized_confidence.variance))

    Performance policy:
        Discard candidates where normalized_confidence.value < 0.80

        latency_stability_penalty:
            if latency_p99 and latency_p50 available from telemetry:
                stability_ratio = latency_p99 / max(latency_p50, 1.0)
                stability_penalty =
                    0.00 if stability_ratio <= 2.0   -- stable tail
                    0.05 if stability_ratio <= 4.0   -- notable instability
                    0.15 if stability_ratio > 4.0    -- catastrophic tail
            else: stability_penalty = 0.0

        score = (0.45 * (1 - cv.latency_cost))
              + (0.20 * (1 - cv.compute_cost))
              + (0.20 * (1 - cv.memory_cost))
              + (0.15 * (1 - stability_penalty))
        -- "usually fast, occasionally catastrophic" is worse than consistently
        -- moderate; stability term captures tail instability p99 alone misses

    Coverage policy:
        score = (0.50 * full_property_pass_rate)   -- all five tiers
              + (0.30 * use_context_coverage_score)
              + (0.20 * normalized_confidence.value)

    Balanced policy:
        score = (0.25 * normalized_confidence.value)
              + (0.25 * adversarial_pass_rate)
              + (0.25 * performance_pass_rate)
              + (0.25 * use_context_coverage_score)

    Custom policy:
        score = weighted sum using PolicyWeights
                (weights normalized to sum to 1.0 before application)

Candidates sorted by policy_score descending.
FrontierPosition assigned relative to policy_score ranking:
    policy_score rank 1:           Dominant
    policy_score rank 2..3:        Competitive
    policy_score rank 4+:          Niche (if score >= 0.5) or OffFrontier
```

### 14.3 Policy in CapabilityQuery

```keln
-- Updated CapabilityQuery to include selection_policy:
type CapabilityQuery = {
    capability_id:    Maybe<CapabilityId>,
    effect_signature: Maybe<EffectSignature>,
    type_hint:        Maybe<TypeFingerprint>,
    use_profile:      UseProfile,
    min_confidence:   Maybe<Probability>,
    max_variance:     Maybe<Float where >= 0.0>,
    require_suite:    SuiteRequirement,
    selection_policy: SelectionPolicy,          -- default: Balanced
    include_off_frontier: Bool                  -- default: false
}
```

### 14.4 Policy Invariants

- Policy application never removes candidates from the response. It
  reorders them. A `Reliability` policy that penalizes an artifact's
  ranking does not hide it — the artifact remains in `candidates` with
  its `policy_score` and `FrontierPosition` visible.
- `Performance` policy has a hard confidence floor of 0.80. An artifact
  below this floor is excluded from `Performance` policy scoring regardless
  of its performance properties. Selecting an extremely fast but unreliable
  artifact for production is not a tradeoff the registry will recommend —
  the selecting AI can override this via `Custom` policy with explicit weights.
- `Custom` policy weights are normalized before application. An AI that
  specifies `{ confidence_weight: 10.0, performance_weight: 1.0 }` gets the
  same result as `{ confidence_weight: 0.91, performance_weight: 0.09 }`.
  The weights express relative priority, not absolute magnitude.
- Policy is a query-time parameter, not a registry-level setting. Different
  AIs building different systems query the same registry with different
  policies. The frontier is shared; the traversal is personalized.

### 14.5 Policy Conflict Resolution

A policy conflict occurs when the stated `SelectionPolicy` cannot be
satisfied by any candidate on the frontier — either because no artifact
meets the policy's minimum requirements, or because the policy produces
a tie that cannot be broken.

**No candidates satisfy policy:**

When `Performance` policy eliminates all candidates due to the 0.80
confidence floor, or when `Custom` weights produce a score of 0.0 for
all candidates, the registry applies a structured relaxation sequence
rather than returning an empty result:

```
Relaxation sequence:
1. Relax the most restrictive policy constraint by 10% of its value.
   (Performance floor: 0.80 → 0.72; confidence minimum: 0.85 → 0.765)
2. If candidates found: return with RelaxationApplied flag in response.
3. If still no candidates: relax by another 10%.
4. Repeat up to 3 relaxation steps.
5. If still no candidates after 3 steps: return SelectionFailure.PolicyUnsatisfiable
   with the best available artifact at the original policy, even if it fails
   the policy. The selecting AI sees the gap explicitly.

type SelectionResponse = {
    result:      SelectionResult,
    relaxation:  Maybe<PolicyRelaxation>
}

type PolicyRelaxation = {
    original_policy:  SelectionPolicy,
    applied_policy:   SelectionPolicy,
    relaxation_steps: Int where 1..3,
    reason:           NonEmptyString,
    degradation:      Maybe<PolicyDegradation>  -- present when relaxation is significant
}

-- Structured signal indicating the degree to which relaxation has moved
-- the result outside the original policy's intent.
-- An AI receiving a high-severity PolicyDegradation should treat the
-- selection as operating outside safe bounds — not refuse it, but flag
-- it as a conscious deviation requiring explicit acknowledgment.
type PolicyDegradation = {
    severity:                    Float where 0.0..1.0,
    -- 0.0: minor adjustment; 1.0: result violates original intent significantly
    -- Computed as: weighted sum of proportional constraint violations
    --   e.g., confidence floor relaxed from 0.85 to 0.72 in 3 steps:
    --   severity contribution = (0.85 - 0.72) / 0.85 * weight = 0.15

    violated_constraints:        List<ConstraintViolation>,
    safe_to_proceed:             Bool
    -- false when severity > 0.6: structured signal that result is
    -- materially outside original bounds
}

type ConstraintViolation = {
    constraint:      NonEmptyString,   -- which policy constraint was relaxed
    original_value:  Float,            -- original threshold
    applied_value:   Float,            -- relaxed threshold
    proportional_gap: Float            -- (original - applied) / original
}
```

**Tie-breaking:**

When two candidates have identical `policy_score` (within Float epsilon),
the tiebreaker sequence is applied in order:

```
1. Higher normalized_confidence.value
2. Lower normalized_confidence.variance
3. Higher use_context_coverage record_count
4. Earlier admission timestamp (more established artifact)
5. If still tied: both returned; selecting AI chooses
```

**When all candidates are low-quality:**

The relaxation mechanism addresses the case where no candidates satisfy
the policy's minimum thresholds. A different failure mode exists: many
candidates satisfy the thresholds but all have low quality (low confidence,
high cost). The registry does not tighten constraints in this case — it
cannot conjure better artifacts into existence. Instead, the response
is fully transparent: all candidates are returned with their complete
`CostVector` and confidence scores. The selecting AI sees exactly how
low-quality the available options are. If the quality is insufficient,
the correct response is to use `Registry.explore` (§17) to understand
why the capability slot's quality is low and potentially submit a better
artifact — not to ask the registry to filter away the honest picture.

**Policy composition:**

An AI may specify multiple policies to express compound requirements:

```keln
-- Extended CapabilityQuery:
type CapabilityQuery = {
    ...
    selection_policy:  SelectionPolicy,           -- primary policy
    fallback_policy:   Maybe<SelectionPolicy>     -- applied if primary finds no candidates
}
```

When `fallback_policy` is present and the primary policy produces no
candidates (after relaxation), the fallback is applied in full without
further relaxation. This gives the selecting AI explicit control over
its degradation path rather than relying on automatic relaxation.

---

## 15. Composition Semantics

A registry that selects artifacts individually but says nothing about whether
they compose safely is incomplete. Keln programs are pipelines — the output
of one function is the input of the next. Two artifacts may each be correct
in isolation while being incompatible in composition. This section specifies
how the registry reasons about composition.

### 15.1 Artifact Contract

Every artifact carries an explicit contract: what it guarantees and what it
requires. Contracts are expressed as `PropertyId` references into the
canonical suite of the artifact's capability slot. This makes contract
checking a set membership operation, not behavioral inference.

```keln
-- Added to Artifact type:
type ArtifactContract = {
    guarantees:          List<PropertyId>,          -- properties this artifact satisfies
                                                    -- computed from SuiteEvaluationDetail;
                                                    -- never declared by submitter

    violates:            List<PropertyId>,          -- properties this artifact explicitly
                                                    -- does NOT satisfy; derived from:
                                                    --   (a) failed Refinement-tier properties
                                                    --       recorded in SuiteEvaluationDetail
                                                    --   (b) excluded_properties in
                                                    --       SpecializationConstraint (§10.3)
                                                    -- never declared; always derived
                                                    -- semantics: "known false", not "unknown"

    requires:            List<PropertyId>,          -- resolved requirements: PropertyIds
                                                    -- from upstream capability suites
    unresolved_requires: List<UnresolvedRequirement> -- requirements that cannot yet be
                                                    -- expressed as PropertyIds because no
                                                    -- upstream suite defines them yet
}

-- Three-state semantics for any PropertyId p relative to artifact A:
--   p in A.guarantees:  known true  — A satisfies this property
--   p in A.violates:    known false — A explicitly does not satisfy this property
--   p absent from both: unknown    — A has not been evaluated against this property
--                                    (property may be from a different capability slot
--                                     or added to the suite after A was admitted)
```

-- A requirement that cannot be expressed as a PropertyId because no upstream
-- capability suite currently defines the needed property.
-- Unresolved requirements are visible in CompositionCheck (block full compatibility)
-- and in SynthesisResponse (signal that upstream suite extension is needed).
-- They do not block admission — the artifact is admitted with the gap recorded.
type UnresolvedRequirement = {
    description:    NonEmptyString,     -- what the artifact assumes about its input
    upstream_slot:  Maybe<CapabilityId>, -- which upstream capability should define this
    severity:       UnresolvedSeverity,
    proposed_property: Maybe<ProposedProperty>  -- optional: submitter proposes the property
                                                -- that would resolve this requirement
}

type UnresolvedSeverity =
    | Critical   -- composition will likely fail without this requirement being satisfied
    | Moderate   -- composition may work in common cases but fails in edge cases
    | Advisory   -- informational; does not affect composition compatibility score
```

`guarantees` is derived automatically from `SuiteEvaluationDetail` at
admission. Every property the artifact passes is a guarantee. The submitter
does not declare guarantees — they are computed from verified behavior.

`requires` references `PropertyId`s from upstream capability suites. When
a submitter needs to express a requirement that no upstream suite defines,
they use `unresolved_requires` instead. If the `UnresolvedRequirement`
includes a `proposed_property`, the registry automatically opens a proposal
for that property in the appropriate upstream suite — this is how composition
gaps drive suite evolution.

### 15.2 Composition Compatibility Check

When an AI queries for a composition — artifact A feeding into artifact B —
the registry performs a compatibility check:

```
CompositionCheck(A, B):

Step 1 — Type compatibility:
    output_type(A) must match input_type(B).
    Match: exact TypeFingerprint equality, or declared TypeVariant compatibility
    in B's CapabilitySchema (compatible_with field).
    Failure: CompositionFailure.TypeMismatch { a_output: TypeFingerprint,
                                               b_input: TypeFingerprint }

Step 2 — Effect compatibility:
    effects(A) ∪ effects(B) must be a valid Keln effect set.
    Specifically: if B declares Pure, then effects(A) must be Pure
    (A cannot introduce effects that B's declared purity prohibits).
    Failure: CompositionFailure.EffectIncompatible { a_effects: List<EffectName>,
                                                     b_effects: List<EffectName> }

Step 3 — Contract compatibility:

    3a. Explicit violation check (most severe):
        For each PropertyId p in B.contract.requires:
            if p in A.contract.violates:
                → CompositionFailure.ExplicitViolation {
                      property: PropertyId,
                      source: ViolationSource }
        type ViolationSource =
            | FailedProperty           -- p failed in A's SuiteEvaluationDetail
            | SpecializationExclusion  -- p is in A's excluded_properties

        ExplicitViolation is more severe than ContractGap:
        A does not merely lack the guarantee — it actively violates it.
        An AI receiving ExplicitViolation should not proceed without
        explicit acknowledgment. This is not a partial compatibility case.

    3b. Missing guarantee check:
        For each PropertyId p in B.contract.requires:
            if p not in A.contract.guarantees AND p not in A.contract.violates:
                → property is "unknown" for A
                → classify the unknown reason:

        type UnknownReason =
            | NotEvaluated
                -- p exists in A's capability suite but A was never
                -- evaluated against it (e.g., added before A's admission
                -- but A was somehow admitted without it — shouldn't happen
                -- post-spec but may exist in legacy lineage)
            | PropertyAddedAfterAdmission
                -- p was added to A's capability suite AFTER A was admitted;
                -- A's SuiteEvaluationDetail predates this property
                -- most common case for evolving suites
            | CrossCapabilityReference
                -- p belongs to a different capability slot's suite;
                -- A is not in that slot and cannot be evaluated against p;
                -- the requirement may be expressing a cross-domain assumption

        UnknownReason is derived from:
            NotEvaluated:               p.added_at <= A.admitted_at AND
                                        p not in A.SuiteEvaluationDetail
            PropertyAddedAfterAdmission: p.added_at > A.admitted_at
            CrossCapabilityReference:   p's suite.capability_hash != A.capability.hash

        Failure: CompositionFailure.ContractGap {
                     unsatisfied_requirements: List<PropertyId>,
                     unknown_properties:       List<{ id: PropertyId, reason: UnknownReason }>,
                     violated_properties:      List<PropertyId>,
                     available_guarantees:     List<PropertyId> }

        Decision guidance by UnknownReason:
            NotEvaluated:               treat as moderate gap; artifact may satisfy property
            PropertyAddedAfterAdmission: treat as minor gap; artifact predates requirement
            CrossCapabilityReference:   treat as advisory; may not be checkable at all

    3c. Unresolved requirements:
        For each UnresolvedRequirement u in B.contract.unresolved_requires:
            u cannot be checked mechanically (no PropertyId to match).
            Result: recorded as CompositionGap (not a failure; a visible gap).
            contract_coverage reduced by: u.severity weight
                (Critical → -0.15, Moderate → -0.08, Advisory → -0.02)

type CompositionGap = {
    requirement:  UnresolvedRequirement,
    artifact_b:   ArtifactHash,
    impact:       Float   -- reduction applied to contract_coverage
}

-- contract_coverage computation (tier × category weighted):
-- Start at 1.0.
-- For each p in B.contract.requires:
--     if p in A.guarantees:   no reduction
--     if p unknown:
--         reduction = unknown_penalty(p.category) * unknown_reason_factor
--         unknown_penalty(category):
--             Correctness   → 0.08
--             Adversarial   → 0.05
--             Performance   → 0.04
--             Informational → 0.01
--         unknown_reason_factor:
--             NotEvaluated               → 1.0   (full penalty; should have been checked)
--             PropertyAddedAfterAdmission → 0.5   (half penalty; artifact predates it)
--             CrossCapabilityReference   → 0.2   (minor; may not apply)
--     if p in A.violates:
--         reduction = violation_penalty(p.tier)
--         violation_penalty(tier):
--             Axiom       → 0.40   (catastrophic; fundamental violation)
--             Invariant   → 0.25   (significant; required correctness violated)
--             Refinement  → 0.10   (notable; excellence gap, not correctness gap)
-- For each u in B.unresolved_requires: -u.severity_weight
-- Clamped to [0.0, 1.0].
--
-- A missing "formatting preference" (Informational + PropertyAddedAfterAdmission)
-- reduces coverage by 0.01 * 0.5 = 0.005.
-- An Axiom violation reduces coverage by 0.40.
-- These are not the same and the formula no longer pretends they are.

Step 4 — Confidentiality compatibility:
    If A has ConfidentialityStatus.Confidential:
        B must also have ConfidentialityStatus.Confidential, or B must
        not require Public inputs (no requires referencing public-only properties).
    Rationale: confidential data cannot flow into a public artifact
    without explicit acknowledgment.
    Failure: CompositionFailure.ConfidentialityBoundary {
                 source: ArtifactHash, sink: ArtifactHash }

type CompositionResult =
    | Compatible    { a: ArtifactHash, b: ArtifactHash,
                      combined_effects: List<EffectName>,
                      contract_coverage: Float }   -- fraction of B.requires covered by A.guarantees
    | Incompatible  { a: ArtifactHash, b: ArtifactHash,
                      failures: List<CompositionFailure> }

type CompositionFailure =
    | TypeMismatch             { a_output: TypeFingerprint, b_input: TypeFingerprint }
    | EffectIncompatible       { a_effects: List<EffectName>, b_effects: List<EffectName> }
    | ContractGap              { unsatisfied_requirements: List<PropertyId>,
                                 available_guarantees:     List<PropertyId> }
    | ConfidentialityBoundary  { source: ArtifactHash, sink: ArtifactHash }
```

### 15.3 Pipeline Composition Query

An AI building a multi-step pipeline can query the registry for a full
composition check across N artifacts:

```keln
-- Added to Registry module interface:
module Registry {
    provides: {
        -- (all prior functions retained; addition:)
        compose: IO List<ArtifactHash> -> Result<PipelineResult, RegistryError>
    }
}

type PipelineResult =
    | PipelineCompatible   { artifacts: List<ArtifactHash>,
                             combined_effects: List<EffectName>,
                             weakest_link: ArtifactHash,   -- lowest contract_coverage
                             pipeline_confidence: Confidence }
    | PipelineIncompatible { failures: List<PipelineFailure> }

type PipelineFailure = {
    position:  Int,             -- index in the artifact list where incompatibility occurs
    artifact_a: ArtifactHash,
    artifact_b: ArtifactHash,
    failure:    CompositionFailure
}
```

`pipeline_confidence` is computed based on the active `SelectionPolicy`:

```
Reliability policy:
    pipeline_confidence = min(component_confidences)
    -- Conservative. Safe for production. A pipeline is only as reliable
    -- as its weakest component.

Performance or Balanced policy:
    pipeline_confidence = harmonic_mean(component_confidences)
    -- Less pessimistic for long pipelines with independent components.
    -- Harmonic mean penalizes weak links more than arithmetic mean
    -- but less than minimum.
    -- harmonic_mean([c1, c2, ..., cn]) = n / sum(1/ci)

Custom policy:
    if policy_weights.confidence_weight >= 0.40: use minimum
    else: use harmonic mean
    -- High confidence weight → conservative (minimum).
    -- Low confidence weight → less pessimistic (harmonic mean).

In all cases:
    pipeline_confidence.variance = max(component_confidence.variances)
    -- Variance is always the maximum; a pipeline's uncertainty is bounded
    -- by its most uncertain component regardless of policy.
```

### 15.4 Contract Coverage and Partial Compatibility

Step 3 of `CompositionCheck` checks that A's guarantees fully cover B's
requirements. When this check fails, the registry does not simply report
incompatibility — it reports `contract_coverage`: what fraction of B's
requirements are covered. A `contract_coverage` of 0.85 means A satisfies
15 of 17 of B's requirements. The selecting AI can see exactly which
requirements are unsatisfied and decide whether the gap is acceptable
for its use case or whether a different upstream artifact is needed.

Partial compatibility is not the same as incompatibility. An AI that
understands the unsatisfied requirements and accepts responsibility for
them can proceed with awareness. The registry provides the information
honestly; it does not make the decision.

### 15.5 Standard Consistency Properties

Temporal and consistency assumptions are the most common source of silent
composition bugs — type and effect compatibility pass, contracts appear
satisfied, but the artifacts disagree on ordering or freshness guarantees.
The registry addresses this through standardized `PropertyId`s that all
capability suites may reference:

```
Standard consistency properties (globally reserved PropertyIds):
    consistency.strong         -- reads always see latest write
    consistency.eventual       -- reads eventually see latest write; may lag
    consistency.monotonic_read -- once a value is read, older values never appear
    consistency.read_your_write -- a writer always sees their own writes
    consistency.causal         -- causally related operations are ordered
    consistency.linearizable   -- equivalent to strong + total order

Standard algorithm properties (globally reserved PropertyIds):
    algorithm.stable              -- output order preserves input order for equal elements
    algorithm.deterministic       -- same input always produces same output (no randomness)
    algorithm.complexity.linear   -- O(n) time complexity
    algorithm.complexity.nlogn    -- O(n log n) time complexity
    algorithm.complexity.quadratic -- O(n²) time complexity
    algorithm.in_place            -- O(1) additional memory beyond input
    algorithm.online              -- can process input incrementally without full materialization
```

These standard properties resolve the behavioral identity problem: two artifacts
with identical type signatures (e.g., `Pure List<Int> -> List<Int>`) are
distinguished by which of these properties their suite requires and their
`ArtifactContract.guarantees` includes. A stable sort and an unstable sort
belong to the same capability slot but are meaningfully different artifacts.
The property suite makes that difference structurally visible and selectable.

These are not built into every suite — they are available for suites to
reference. A capability that makes consistency guarantees includes the
appropriate `PropertyId` in its suite. An artifact that guarantees
`consistency.strong` has that in its `ArtifactContract.guarantees`. An
artifact that requires it has it in `requires`. CompositionCheck Step 3
then catches mismatches mechanically, without the registry needing to
understand consistency semantics.

This is the pattern for any semantic concern that spans capabilities:
encode it as a standardized `PropertyId`, include it in the relevant
suites, and the composition machinery handles the rest.

### 15.6 Composition Invariants

- Composition checks are stateless query operations. They do not modify
  any artifact record, frontier status, or confidence score.
- `ArtifactContract.guarantees` is computed at admission and stored with
  the artifact. It is never recomputed — if the suite changes after
  admission and the artifact's passes change, the contract reflects the
  suite version at admission time. A re-evaluated artifact (after grace
  period or voluntary resubmission) gets a fresh contract.
- `ArtifactContract.requires` is declared by the submitter and is
  immutable after admission. A corrected `requires` requires a new
  artifact submission (Refactor mutation type).
- Confidentiality boundary checking (Step 4) is conservative. When in
  doubt, the registry flags the incompatibility. An AI that has verified
  the boundary is safe can use `Custom` composition options to override —
  but the override is explicit and recorded in lineage, not silent.

---

---

## 16. Unified Cost Model

Selection policies in §14 weight multiple dimensions but operate on
heterogeneous units: confidence is a probability, latency is a duration,
memory is bytes, failure risk is derived. Without normalization across
these dimensions, policy scoring is inconsistent — the same relative
priority produces different selections depending on the scale of the
underlying measurements. The `CostVector` is the normalization layer.

### 16.1 CostVector

```keln
-- A normalized multi-dimensional cost measure for an artifact
-- evaluated in a specific use context.
-- All components are in [0.0, 1.0]. Lower is always better.
-- 0.0 = best observed across all artifacts in this capability slot.
-- 1.0 = worst observed across all artifacts in this capability slot.
-- Normalization is per-slot, per-use-context, at a specific point in time.
type CostVector = {
    failure_risk:          Float where 0.0..1.0,  -- 1 - normalized_confidence.value
    latency_cost:          Float where 0.0..1.0,  -- normalized p99 latency
    memory_cost:           Float where 0.0..1.0,  -- normalized p99 memory usage
    compute_cost:          Float where 0.0..1.0,  -- normalized CPU p99
    variance_cost:         Float where 0.0..1.0,  -- normalized confidence variance

    normalization_epoch:   Timestamp              -- when this CostVector was computed
                                                  -- CostVectors are non-stationary:
                                                  -- a new artifact entering the slot
                                                  -- shifts the p10-p90 baseline, changing
                                                  -- all existing CostVectors.
                                                  -- epoch allows AIs to distinguish:
                                                  --   "this artifact got worse" (same epoch)
                                                  --   "baseline shifted" (different epoch)
}
```

### 16.2 CostVector Derivation

```
CostVector derivation for artifact A in use context U:

failure_risk(A, U):
    = 1.0 - conditional_confidence(A, U).value

-- Policy-dependent unknown cost defaults (16.A):
-- When a cost dimension has no data, the default reflects policy risk tolerance.
-- Reliability policy is pessimistic about unknowns (0.7).
-- Performance policy is neutral (0.5). Coverage policy is moderate (0.6).
-- Applied to latency, memory, and compute when telemetry is unavailable.

    unknown_cost(policy):
        Reliability → 0.7   -- pessimistic; unknown risk is treated as likely cost
        Performance → 0.5   -- neutral; unknown latency is median assumption
        Coverage    → 0.6   -- moderate; unknown may indicate underexplored artifact
        Balanced    → 0.5   -- neutral
        Custom      → 0.5   -- neutral unless weights signal risk-sensitivity

latency_cost(A, U):
    source: TelemetryObservations.latency_p99 if available (>= 100 invocations)
            else: performance property results (pass rate inverse)
            else: unknown_cost(query.selection_policy)
    normalization: clipped p10-p90 across slot artifacts
        p10_latency = 10th percentile of latency_p99 values across all slot artifacts
        p90_latency = 90th percentile
        raw_cost = clamp(A_latency, p10_latency, p90_latency)
        latency_cost = (raw_cost - p10_latency) / (p90_latency - p10_latency)
    if only one artifact in slot: 0.0 (no comparison possible)
    rationale: min-max normalization collapses all artifacts toward 0.0 when
    one outlier has extreme latency. Clipped normalization preserves meaningful
    differences among the central 80% of artifacts. Outliers clamp to 0.0 or 1.0.

memory_cost(A, U):
    source: TelemetryObservations.memory_p99_mb if available
            else: performance property results
            else: unknown_cost(query.selection_policy)
    normalization: clipped p10-p90, same method as latency_cost

compute_cost(A, U):
    source: TelemetryObservations.cpu_p99_pct if available
            else: unknown_cost(query.selection_policy)
    normalization: clipped p10-p90, same method as latency_cost

variance_cost(A, U):
    = conditional_confidence(A, U).variance / 0.5
    capped at 1.0
    (0.5 variance is treated as maximum meaningful variance)
```

### 16.3 CostVector in Selection Policies

SelectionPolicy scoring (§14.2) is revised to operate over `CostVector`
rather than raw dimensions. This makes policy weights meaningful across
capability slots:

```
Reliability policy score:
    = (0.50 * (1 - cv.failure_risk))
    + (0.30 * adversarial_pass_rate)
    + (0.20 * (1 - cv.variance_cost))

Performance policy score:
    discard if cv.failure_risk > 0.20   -- confidence floor equivalent
    = (0.50 * (1 - cv.latency_cost))
    + (0.25 * (1 - cv.compute_cost))
    + (0.25 * (1 - cv.memory_cost))

Coverage policy score:
    = (0.50 * full_property_pass_rate)
    + (0.30 * use_context_coverage_score)
    + (0.20 * (1 - cv.failure_risk))

Balanced policy score:
    = (0.25 * (1 - cv.failure_risk))
    + (0.25 * (1 - cv.latency_cost))
    + (0.25 * adversarial_pass_rate)
    + (0.25 * use_context_coverage_score)

Custom policy score:
    = sum(weight_i * (1 - cv.component_i)) for each component
    weights normalized to sum to 1.0
```

### 16.4 CostVector in ScoredArtifact

```keln
-- Updated ScoredArtifact to include CostVector and BehaviorChange visibility:
type ScoredArtifact = {
    artifact:               Artifact,
    raw_confidence:         Confidence,
    normalized_confidence:  Confidence,
    domain_difficulty:      DomainDifficulty,
    conditional_confidence: Confidence,
    cost_vector:            CostVector,           -- normalized cost across all dimensions
    policy_score:           Float,                -- computed from CostVector + policy
    use_context_coverage:   ContextCoverage,
    frontier_position:      FrontierPosition,
    investigation_flags:    List<InvestigationFlag>,  -- from §12.4; empty if none
    behavior_change:        Maybe<BehaviorChangeSummary>,  -- present iff MutationType == BehaviorChange
    performance_claim:      Maybe<PerformanceClaimSummary> -- present iff MutationType == Optimize
}

-- Mandatory visibility signal for BehaviorChange artifacts.
-- Present whenever artifact.mutation_type == BehaviorChange.
-- An AI selecting from the frontier sees this without needing to traverse lineage.
type BehaviorChangeSummary = {
    predecessor:          ArtifactHash,
    changed_property_count: Int where >= 1,    -- how many properties changed behavior
    changed_properties:   List<PropertyId>,    -- which properties (tier + category visible)
    rationale:            NonEmptyString,       -- from BehaviorDelta.rationale
    severity:             BehaviorChangeSeverity
}

type BehaviorChangeSeverity =
    | MinorChange     -- only Refinement-tier properties changed
    | ModerateChange  -- Invariant-tier properties changed

-- MajorChange (Axiom-tier) is NOT a valid variant.
-- §13.4 is explicit: "Axiom-tier properties are never intentionally changeable."
-- BehaviorChange must pass all axiom properties. A BehaviorDelta containing
-- Axiom-tier PropertyIds is rejected at Gate 8 with AdmissionFailure.AxiomChangeAttempted.
-- The type system does not include MajorChange because the code path cannot occur
-- in a valid submission.

-- Summary of PerformanceClaim for selecting AIs.
type PerformanceClaimSummary = {
    metric:       PerformanceMetric,
    improvement:  Float where > 0.0,
    claim_status: PerformanceClaimStatus,
    stale:        Bool   -- true if Asserted and unconfirmed for > 90 days
}
```

### 16.5 CostVector Invariants

- All CostVector components are in [0.0, 1.0]. The normalization is
  per-capability-slot, per-use-context. Moving to a different capability
  slot resets the normalization baseline.
- When only one artifact exists in a slot (all performance dimensions
  are 0.0 — no comparison possible), the CostVector is informational
  only. Policy scoring still applies; the single artifact will score
  well on normalized dimensions trivially.
- CostVector is recomputed when new TelemetryRecords arrive or when
  a new artifact is admitted (which changes the normalization baseline).
  Cached CostVectors are invalidated on these events, same mechanism as
  confidence cache invalidation (§6.4).

---

## 17. Lineage-Driven Exploration

The lineage graph in §10 records mutation history. This section specifies
how the registry uses that history actively — not just to answer "where
did this come from?" but to guide where to explore next. Lineage becomes
signal, not just provenance.

### 17.1 LineageSignal

```keln
-- Computed signal extracted from the lineage graph for a capability slot.
-- Used by the registry to weight mutation priorities and by AIs to
-- understand which directions of the artifact space have been productive.
type LineageSignal = {
    capability:          CapabilityHash,
    computed_at:         Timestamp,

    productive_paths:    List<LineagePath>,   -- mutation sequences with high yield
    dead_ends:           List<LineagePath>,   -- mutation sequences that repeatedly fail
    axiom_violations:    List<StructuralPattern>, -- patterns that triggered QuarantineRecords
    frontier_lineage:    List<ArtifactHash>,  -- current frontier members' ancestors
    exploration_frontier: List<ExplorationHint>  -- suggested next directions
}

type LineagePath = {
    sequence:        List<MutationType>,     -- e.g. [Initial, Optimize, Generalize]
    artifact_count:  Int where >= 1,         -- artifacts on this path
    avg_confidence:  Float where 0.0..1.0,   -- mean confidence of path artifacts
    frontier_rate:   Float where 0.0..1.0    -- fraction that reached frontier
}

type StructuralPattern = {
    pattern:         TypeFingerprint,        -- structural hash of the violating artifacts
                                             -- see computation specification below
    violation_count: Int where >= 1,
    axioms_violated: List<PropertyId>
}

-- StructuralPattern fingerprint computation:
-- The TypeFingerprint used for StructuralPattern is:
--     SHA-256(
--         effect_signature            -- effect set of the submitting artifact
--         + capability_hash           -- which slot it was submitted to
--         + top_level_record_shapes   -- structural shape of the artifact's
--                                        input and output record types:
--                                        field names + field types (not values)
--     )
--
-- This is the same hash used for CapabilityHash computation, applied to the
-- submitting artifact's type-level structure rather than the canonical slot.
--
-- Rationale: axiom violations are almost always structural misunderstandings
-- about what the capability slot requires — an artifact that "tries to do the
-- wrong thing" at the type level — rather than implementation bugs.
-- Matching on type structure finds artifacts with the same structural
-- misunderstanding, regardless of implementation details.
--
-- Two artifacts with identical effect signatures, capability slots, and
-- record shapes will share a StructuralPattern fingerprint. This is intentional:
-- if one violated an axiom, similarly-structured artifacts are at elevated risk.
--
-- What is NOT included in the fingerprint:
--   - Function bodies / source text (too specific; matches nothing useful)
--   - Verify block content (implementation detail, not structural)
--   - Field values / constraints (runtime, not structural)
--
-- Coarseness tradeoff: this fingerprint matches on type structure, not behavior.
-- False positives (structurally similar but behaviorally safe artifacts flagged)
-- are possible. LineageWarning (§17.4) handles this gracefully: it signals
-- elevated risk without blocking admission. The registry does not reject
-- submissions solely because their StructuralPattern matches a prior violation.

type ExplorationHint = {
    suggested_mutation: MutationType,
    target_artifact:    ArtifactHash,        -- good candidate to mutate from
    rationale:          NonEmptyString,
    estimated_yield:    Float where 0.0..1.0, -- historical frontier rate for similar paths
    risk_score:         Float where 0.0..1.0  -- estimated probability of gate failure
                                              -- or QuarantineRecord on this path
    -- risk_score derived from:
    --   (a) axiom_violation_rate: fraction of attempts on this path that
    --       triggered QuarantineRecords historically
    --   (b) gate_failure_rate: fraction that failed any admission gate
    --   risk_score = 0.6 * axiom_violation_rate + 0.4 * gate_failure_rate
    --
    -- A high estimated_yield with high risk_score means: this path produces
    -- frontier artifacts when it works, but fails often and sometimes severely.
    -- An AI should weigh both: high-risk/high-yield paths are not "safe bets"
    -- even when yield looks attractive.
}
```

### 17.2 Lineage Signal Computation

LineageSignal is computed periodically per capability slot, not on every
admission. Trigger: every 20 admissions or every 100 UseContext records
for the slot, whichever comes first.

```
Productive path identification:
    Walk the lineage DAG for all frontier artifacts.
    For each path from Initial to a frontier artifact:
        record the MutationType sequence
        record the confidence at each step
    Cluster paths by sequence prefix.

    Compute slot_median_rate: median frontier rate across all paths in the slot.
    This is the capability-relative baseline.

    Adaptive thresholds:
        productive_threshold = max(slot_median_rate * 1.5, 0.20)
        -- Paths at 1.5x the slot median are productive.
        -- Floor of 0.20: in very hard slots with low baseline rates,
        -- avoid classifying everything as a dead end.

        dead_end_threshold   = min(slot_median_rate * 0.25, 0.10)
        -- Paths at 25% of the slot median are dead ends.
        -- Ceiling of 0.10: in easy slots, avoid too-narrow dead end bands.

    Paths above productive_threshold: productive.

    Dead end classification requires ALL of:
        (a) frontier_rate < dead_end_threshold
        (b) attempt_count >= 10
        (c) lineage_diversity >= 2 distinct architecture_hashes
            among the contributors who made those attempts
    Rationale for (c): 10 attempts from the same model architecture
    may reflect a shared training bias, not a genuine dead end in the
    capability space. Diversity requirement mirrors the convergence
    independence rule (§4.3, §3.1).

    Paths between thresholds: unclassified (inconclusive signal).
    Paths with < 10 attempts OR < 2 architecture_hashes: unclassified
    regardless of rate (insufficient or insufficiently diverse signal).

Axiom violation patterns:
    Collect all QuarantineRecords for this capability slot.
    Extract structural_pattern from each.
    Cluster by pattern similarity (TypeFingerprint distance).
    Clusters with >= 2 violations: recorded as StructuralPattern.

Exploration hints:
    For each frontier artifact:
        identify which MutationType sequences it has NOT yet been
        the source of (e.g., frontier artifact with no Generalize children)
        generate ExplorationHint suggesting that mutation
        estimated_yield = historical frontier_rate for that sequence
                          across the productive_paths set
```

### 17.3 Exploration Query

An AI generating new Keln code can query the registry for exploration hints
before writing a new artifact, avoiding known dead ends and directing effort
toward productive paths:

```keln
-- Added to Registry module interface:
module Registry {
    provides: {
        -- (all prior functions retained; addition:)
        explore: IO CapabilityHash -> Result<LineageSignal, RegistryError>
    }
}
```

The `explore` response tells the AI:

1. Which structural patterns have repeatedly violated axioms — avoid these.
2. Which mutation paths have produced frontier artifacts — favor these.
3. Which frontier artifacts have unexplored mutation directions — start here.
4. What the estimated yield is for each suggested direction.

An AI submitting a new artifact without querying `explore` first is making
a blind submission. Blind submissions are not penalized — but they miss the
signal that would make their contribution more likely to succeed.

### 17.4 Dead End Suppression

When a lineage path is classified as a dead end (< 5% frontier rate across
>= 10 attempts), the registry does not block submissions on that path. It
annotates them:

```keln
type AdmissionResult =
    | Admitted  { artifact: Artifact, frontier_impact: FrontierImpact,
                  lineage_warning: Maybe<LineageWarning> }
    | Rejected  { gate: GateId, failure: AdmissionFailure }

type LineageWarning = {
    path_sequence:   List<MutationType>,
    historical_rate: Float,      -- frontier rate for this path
    attempts:        Int,        -- how many times this path has been tried
    message:         NonEmptyString
}
```

A `LineageWarning` is informational. The artifact is still admitted if it
passes all gates. The warning tells the submitting AI that similar attempts
have historically not reached the frontier — the current submission may,
but the odds are documented.

### 17.5 Lineage Exploration Invariants

- `LineageSignal` is read-only. Querying it produces no side effects.
- `ExplorationHint.estimated_yield` is computed from historical data, not
  predicted. It describes what has happened on similar paths; it makes no
  claim about what will happen on the suggested path.
- Dead end classification is reversible. If a path that was classified as
  a dead end begins producing frontier artifacts (e.g., a new property suite
  revision makes previously-failing artifacts viable), the classification
  is updated at the next LineageSignal computation.
- Axiom violation patterns are permanent records. A structural pattern that
  triggered a QuarantineRecord is never removed from the `axiom_violations`
  list, even if no future artifacts trigger it. The history of what failed
  is as valuable as the history of what succeeded.

---

## 18. Query and Synthesis Interface

The sections above specify the registry's components individually. This
section specifies how an AI uses the registry operationally — how a query
becomes a program. This is the interface that makes the registry a
programming substrate rather than sophisticated infrastructure.

### 18.1 The Fundamental Operation

The registry's core operation is not "fetch this artifact." It is:

```
solve(capability_need + constraints + policy) → program fragment
```

An AI building a Keln program expresses what it needs; the registry finds,
validates, and composes the solution. The AI does not browse, does not
compare manually, does not make trust judgments. It states requirements
and receives verified, scored, composable artifacts.

### 18.2 SynthesisQuery

```keln
-- The complete query type for program-level registry interaction.
-- Extends CapabilityQuery with composition context and synthesis options.
type SynthesisQuery = {
    -- What is needed (from CapabilityQuery)
    capability_id:    Maybe<CapabilityId>,
    effect_signature: Maybe<EffectSignature>,
    type_hint:        Maybe<TypeFingerprint>,

    -- Deployment context
    use_profile:      UseProfile,
    selection_policy: SelectionPolicy,
    fallback_policy:  Maybe<SelectionPolicy>,

    -- Quality requirements
    min_confidence:   Maybe<Probability>,
    max_variance:     Maybe<Float where >= 0.0>,
    max_cost:         Maybe<CostVector>,  -- upper bounds per cost dimension
    require_suite:    SuiteRequirement,

    -- Composition context
    upstream:         Maybe<ArtifactHash>,   -- artifact whose output feeds this one
    downstream:       Maybe<ArtifactHash>,   -- artifact this one feeds into

    -- Epoch context: affordance for normalization_epoch continuity
    epoch_context:    Maybe<EpochContext>,

    -- Synthesis options
    allow_exploration: Bool,    -- if true: include ExplorationHints in response
    include_lineage:   Bool,    -- if true: include lineage summary per candidate
    max_candidates:    Int where 1..20  -- default 5
}

-- Allows a selecting AI to act on normalization_epoch signal rather than
-- merely detect it. When present, the registry provides epoch-contextualized
-- scoring alongside current scoring.
type EpochContext =
    | CompareToPrior { prior_epoch: Timestamp }
      -- "I previously selected based on scores at this epoch.
      --  Show me how scores have changed since then."
      -- Response includes epoch_delta: EpochDelta per candidate

    | StabilizeAt { epoch: Timestamp }
      -- "Compute scores using the normalization baseline from this epoch."
      -- Useful when comparing artifacts across sessions where the baseline
      -- may have shifted between selections.
      -- Response scores use the historical p10-p90 normalization from that epoch.

type EpochDelta = {
    artifact_hash:         ArtifactHash,
    prior_policy_score:    Float,
    current_policy_score:  Float,
    score_change:          Float,          -- current - prior
    change_reason:         EpochChangeReason
}

type EpochChangeReason =
    | ArtifactDegraded     -- artifact's own confidence or corpus signal worsened;
                           -- baseline was stable
    | BaselineShifted      -- normalization baseline changed (new artifacts entered slot);
                           -- artifact's own signal was stable
    | BothChanged {
          artifact_contribution: Float,   -- signed; negative = artifact's signal worsened
          baseline_contribution: Float    -- signed; negative = baseline shifted unfavorably
          -- artifact_contribution + baseline_contribution ≈ EpochDelta.score_change
          -- (approximate due to interaction effects in normalized scoring)
          -- Interpretation:
          --   |artifact_contribution| > |baseline_contribution|: reselection likely warranted
          --   |baseline_contribution| > |artifact_contribution|: ecosystem shift; consider
          --     widening query or waiting for new artifacts
      }
    | Improved             -- artifact improved (positive score_change); breakdown:
                           -- positive artifact_contribution dominates
```

### 18.3 SynthesisResponse

```keln
type SynthesisResponse = {
    query:          SynthesisQuery,
    candidates:     List<SynthesisCandidate>,
    suite_status:   SuiteStatus,
    composition:    Maybe<CompositionAnalysis>,  -- present when upstream/downstream specified
    exploration:    Maybe<ExplorationSummary>,   -- present when allow_exploration == true
    relaxation:     Maybe<PolicyRelaxation>      -- present when policy was relaxed
}

type SynthesisCandidate = {
    artifact:              Artifact,
    cost_vector:           CostVector,
    normalized_confidence: Confidence,
    conditional_confidence: Confidence,
    policy_score:          Float,
    use_context_coverage:  ContextCoverage,
    frontier_position:     FrontierPosition,
    investigation_flags:   List<InvestigationFlag>,
    lineage_summary:       Maybe<LineageSummary>,  -- present when include_lineage == true
    composition_fit:       Maybe<CompositionFit>   -- present when upstream/downstream specified
}

-- How well this candidate fits the specified composition context.
type CompositionFit = {
    upstream_check:   Maybe<CompositionResult>,  -- check against upstream artifact
    downstream_check: Maybe<CompositionResult>,  -- check against downstream artifact
    fit_score:        Float where 0.0..1.0,      -- combined compatibility score
    gaps:             List<CompositionFailure>    -- what doesn't fit
}

-- Composition analysis across the full pipeline context.
type CompositionAnalysis = {
    upstream:         Maybe<ArtifactHash>,
    downstream:       Maybe<ArtifactHash>,
    best_fit:         Maybe<ArtifactHash>,    -- candidate with highest fit_score
    no_fit_reason:    Maybe<NonEmptyString>   -- if no candidate fits the context
}

-- Exploration summary when allow_exploration is true.
type ExplorationSummary = {
    lineage_signal:     LineageSignal,
    recommended_next:   List<ExplorationHint>,
    dead_end_warning:   Maybe<LineageWarning>  -- if query resembles a dead end path
}

-- Brief lineage summary per candidate when include_lineage is true.
type LineageSummary = {
    mutation_path:    List<MutationType>,   -- from Initial to this artifact
    path_depth:       Int where >= 1,       -- mutation steps from Initial
    path_yield:       Float where 0.0..1.0  -- historical frontier rate for this path
}
```

### 18.4 Synthesis Pipeline

The synthesis pipeline integrates all preceding sections into a single
coherent query execution:

```
SynthesisQuery execution:

Step 1 — Resolve candidates (§5.1 Steps 1-3)
Step 2 — Score confidence per candidate (§6, §11)
Step 3 — Compute CostVector per candidate (§16.2)
Step 4 — Apply selection policy over CostVector (§16.3, §14.2)
Step 5 — Apply policy conflict resolution if needed (§14.5)
Step 6 — Compute frontier position per candidate (§5.1 Step 6, revised)
Step 7 — If upstream/downstream specified: run CompositionCheck per candidate (§15.2)
          Compute CompositionFit and CompositionAnalysis
Step 8 — If allow_exploration: query LineageSignal (§17.2)
          Generate ExplorationSummary
Step 9 — If include_lineage: attach LineageSummary per candidate
Step 10 — Assemble SynthesisResponse

    Ranking when NO composition context (upstream/downstream absent):
        sort by policy_score descending

    Ranking when composition context IS present (upstream or downstream specified):
        final_score = 0.7 * policy_score + 0.3 * fit_score
        sort by final_score descending

        Additionally: if fit_score < 0.5, demote FrontierPosition by one tier:
            Dominant    → Competitive
            Competitive → Niche
            Niche       → OffFrontier (still returned; visibly demoted)
        Rationale: an artifact with poor composition fit is genuinely less
        useful in this context regardless of its standalone quality. The blend
        prevents a slightly better standalone artifact from hiding a composition
        incompatibility behind a higher policy_score.
```

### 18.5 Canonical Usage Pattern

An AI building a Keln program uses the registry in three phases:

**Phase 1 — Explore (optional, for new capability spaces):**
```keln
let signal = Registry.explore(capability_hash)
-- Read LineageSignal to understand what has worked and what hasn't.
-- Identify ExplorationHints to guide artifact generation or selection.
```

**Phase 2 — Select:**
```keln
let response = Registry.synthesize(SynthesisQuery {
    capability_id:    Some("parse.json"),
    use_profile:      UseProfile { call_pattern: Batch, ... },
    selection_policy: Reliability,
    upstream:         Some(byte_source_artifact_hash),
    downstream:       Some(json_consumer_artifact_hash),
    allow_exploration: false,
    max_candidates:   3
})
-- Receive SynthesisResponse with candidates scored, composed, and ranked.
-- Select top candidate or choose based on CompositionFit if composition matters.
```

**Phase 3 — Contribute (after use):**
```keln
let contribution = Registry.contribute(UseContext {
    artifact_hash:    selected_artifact.artifact_hash,
    contributor:      my_provenance_id,
    use:              UseProfile { call_pattern: Batch, ... },
    behavior:         [observed_behavior_records],
    confidence_delta: +0.05,
    verify:           my_verify_block
})
-- Close the loop: contribute verified experience back to the corpus.
-- Benefit: builds contributor track record; improves registry for next AI.
```

This three-phase pattern — explore, select, contribute — is the operational
heartbeat of the registry. Every AI that uses it and contributes back makes
the registry more accurate for the next AI in a similar situation.

### 18.6 What the Registry Does Not Do

The synthesis interface is powerful but bounded. These are outside its scope:

- **It does not generate code.** The registry selects from admitted artifacts.
  Generating a new artifact is the AI's responsibility, informed by
  `ExplorationHints` but not automated by the registry.
- **It does not make final selections.** `SynthesisResponse.candidates`
  is a ranked list with full signal. The selecting AI chooses. The registry
  never collapses the response to a single mandatory answer.
- **It does not verify the composition at runtime.** `CompositionFit` is
  a static analysis based on declared contracts. Runtime behavior is captured
  by the telemetry and experience corpus, not by the synthesis query.
- **It does not manage the AI's program state.** The registry is stateless
  with respect to the programs that use it. Each query is independent.
  Program-level state — which artifacts were selected, how they performed —
  is the consuming AI's responsibility to track and contribute back.

---

*Keln Registry Specification v0.1 — Revision 10 (post-tenth-critique).*
*§0  Status and Design Intent*
*§1  Core Types (Confidential effect; dual-axis CanonicalProperty: tier × category)*
*§2  Admission Protocol (Gates 0-8; axiom QuarantineRecord)*
*§3  Canonical Property Suites and Bootstrap Protocol*
*§4  Experience Corpus (UseContext, BehaviorRecord, ConfidentialUseContext)*
*§5  Selection Protocol*
*§6  Confidence Propagation*
*§7  Garbage Collection and Pareto Frontier*
*§8  The Registry as a Keln Program*
*§9  Capability Schemas*
*§10 Mutation Types*
*§11 Confidence Normalization*
*§12 Runtime Telemetry (severity-weighted failure rate; latency stability signal)*
*§13 Property Suite Structure (single-list; dual-axis; diminishing refinement returns;*
*     mutation comparison against property intersection for evolving suites)*
*§14 Selection Policies (InvestigationFlag penalty; latency stability in Performance)*
*§15 Composition Semantics (UnknownReason split; tier×category weighted coverage;*
*     standard consistency properties)*
*§16 Unified Cost Model (policy-dependent unknown cost defaults)*
*§17 Lineage-Driven Exploration (risk_score on ExplorationHints)*
*§18 Query and Synthesis Interface (composition fit blended into ranking)*
*Addresses sixth critique: severity-weighted failure rate; latency stability;*
*refinement diminishing returns; evolving suite mutation comparison;*
*InvestigationFlag scoring; latency variance; UnknownReason; weighted coverage;*
*policy-dependent unknowns; risk_score on hints; composition fit ranking.*
*Addresses seventh critique: §0 invariant 2 (slot vs artifact identity);*
*standard algorithm properties §15.5; Gate 3 AutoDerived requirement;*
*ConfidenceSource type in §1. See keln-critique-response-addendum.md for*
*language spec changes (CHAN_RECV_MAYBE, Never guarantee scope,*
*bytecode schema versioning, sync SELECT clarification).*
*Addresses eighth critique: PerformanceClaim verify block + PerformanceClaimStatus;*
*BehaviorChangeSummary on ScoredArtifact; CompatibilityClaim lifecycle (§9.4);*
*diversity formula revised (min formula); registry execution budget §2.4.*
*Addresses ninth critique: DomainDifficulty bootstrap prior + difficulty_version;*
*MajorChange removed (dead code) + AxiomChangeAttempted added;*
*EpochContext in SynthesisQuery for normalization_epoch affordance;*
*AxiomExclusionAttempted in Specialize Gate 8;*
*StructuralPattern fingerprint computation specified.*
*Addresses tenth critique: NormalizationSnapshot permanence + EpochContextFailure*
*in §7.5; BothChanged contribution breakdown in EpochChangeReason.*
*To be versioned alongside keln-spec-v1.0.*

