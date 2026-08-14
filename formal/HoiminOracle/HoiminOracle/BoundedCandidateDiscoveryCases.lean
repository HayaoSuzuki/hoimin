import HoiminOracle.BoundedCandidateDiscoveryProofs

namespace HoiminOracle.BoundedCandidateDiscovery

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  limit : Nat
  token : List Candidate := []
  ast : List Candidate := []
  annotation : List Candidate := []
  targets : List (List Candidate) := []
  deriving Repr, DecidableEq, BEq

structure Observation where
  identities : List Nat
  truncated : Bool
  sequences : List Nat
  targetsRead : Nat
  spoolFinished : Bool
  candidateLimitDiagnostic : Bool
  exitCode : Nat
  deriving Repr, DecidableEq, BEq

def candidate
    (identity orderKey : Nat) (producer : Producer)
    (eligible : Bool := true) (emissionIndex : Nat := 0) : Candidate where
  identity := identity
  orderKey := orderKey
  producer := producer
  eligible := eligible
  emissionIndex := emissionIndex
  spanStart := orderKey
  replacement := toString identity

def fixtureCandidate
    (identity orderKey : Nat) (producer : Producer) (path : String)
    (spanStart spanLength : Nat) (original replacement operatorKey : String)
    (line column : Nat) (eligible : Bool := true) : Candidate where
  identity := identity
  orderKey := orderKey
  producer := producer
  eligible := eligible
  path := path
  spanStart := spanStart
  spanLength := spanLength
  original := original
  replacement := replacement
  operatorKey := operatorKey
  line := line
  column := column

def discoveryObservation (result : Discovery) : Observation where
  identities := result.candidates.map Candidate.identity
  truncated := result.truncated
  sequences := sequences result.candidates.length
  targetsRead := 0
  spoolFinished := false
  candidateLimitDiagnostic := result.truncated
  exitCode := 0

def targetObservation (result : TargetState) : Observation where
  identities := result.candidates.map Candidate.identity
  truncated := result.truncated
  sequences := sequences result.candidates.length
  targetsRead := result.targetsRead
  spoolFinished := result.spoolFinished
  candidateLimitDiagnostic := result.truncated
  exitCode := 0

def observe (item : OracleCase) : Observation :=
  match item.scenario with
  | "producer_merge" =>
      discoveryObservation
        (mergeProducerWindows item.token item.ast item.annotation item.limit)
  | "ordered_targets" => targetObservation (discoverTargets item.targets item.limit)
  | "public_plan" =>
      let result := discoveryObservation (bounded item.token item.limit)
      { result with exitCode := if result.truncated then 4 else 0 }
  | _ => discoveryObservation (bounded item.token item.limit)

def outOfOrderDuplicate : OracleCase where
  id := "out_of_order_duplicate"
  mode := "internal-fixture"
  scenario := "candidate_prefix"
  limit := 3
  token :=
    [ candidate 3 30 .token true 0
    , candidate 1 10 .token true 1
    , candidate 1 10 .ast true 2
    , candidate 2 20 .token true 3 ]

def threeProducerMerge : OracleCase where
  id := "three_producer_merge"
  mode := "internal-fixture"
  scenario := "producer_merge"
  limit := 2
  token := [fixtureCandidate 1 37 .token "pkg/three.py" 37 2 "==" "!=" "compare_eq_ne" 2 9]
  ast := [fixtureCandidate 2 50 .ast "pkg/three.py" 50 4 "list" "tuple" "collection_list_tuple" 3 4]
  annotation := [fixtureCandidate 3 65 .annotation "pkg/three.py" 65 9 "list[int]" "Sequence[int]" "type_list_sequence" 4 3]

def eligibilityBeforeCapacity : OracleCase where
  id := "eligibility_before_capacity"
  mode := "internal-fixture"
  scenario := "candidate_prefix"
  limit := 1
  token :=
    [ fixtureCandidate 9 6 .token "pkg/sample.py" 6 4 "True" "False" "boolean_literal" 1 6 false
    , fixtureCandidate 1 23 .token "pkg/sample.py" 23 1 "+" "-" "binary_add_sub" 2 11 ]

def completeTwoTargets : OracleCase where
  id := "complete_two_targets"
  mode := "internal-fixture"
  scenario := "ordered_targets"
  limit := 2
  targets :=
    [ [candidate 1 10 .token]
    , [candidate 2 5 .ast] ]

def truncatedNonFinalTarget : OracleCase where
  id := "truncated_non_final_target"
  mode := "internal-fixture"
  scenario := "ordered_targets"
  limit := 1
  targets :=
    [ [candidate 1 10 .token, candidate 2 20 .token]
    , [candidate 3 5 .ast] ]

def zeroLimit : OracleCase where
  id := "zero_limit"
  mode := "internal-fixture"
  scenario := "candidate_prefix"
  limit := 0
  token := [fixtureCandidate 1 13 .token "pkg/sample.py" 13 2 "==" "!=" "compare_eq_ne" 1 13]

def usizeMax : OracleCase where
  id := "usize_max"
  mode := "model-only"
  scenario := "candidate_prefix"
  limit := 18446744073709551615
  token := [candidate 1 10 .token]

def publicTruncatedPlan : OracleCase where
  id := "public_truncated_plan"
  mode := "strict"
  scenario := "public_plan"
  limit := 1
  token :=
    [ fixtureCandidate 1 10 .token "src/alpha.py" 13 2 "==" "!=" "compare_eq_ne" 1 13
    , fixtureCandidate 2 20 .token "src/beta.py" 13 2 "==" "!=" "compare_eq_ne" 1 13 ]

set_option maxHeartbeats 100000 in
theorem three_producer_window_is_global_identity_projection :
    reference
      (producerWindow threeProducerMerge.token threeProducerMerge.limit ++
        producerWindow threeProducerMerge.ast threeProducerMerge.limit ++
        producerWindow threeProducerMerge.annotation threeProducerMerge.limit) =
    (reference
      (threeProducerMerge.token ++ threeProducerMerge.ast ++
        threeProducerMerge.annotation)).filter
      (retainedByIdentity
        (producerWindow threeProducerMerge.token threeProducerMerge.limit ++
          producerWindow threeProducerMerge.ast threeProducerMerge.limit ++
          producerWindow threeProducerMerge.annotation threeProducerMerge.limit)) := by
  decide

set_option maxHeartbeats 100000 in
theorem three_producer_top_rank_is_covered (item : Candidate) :
    item ∈
      (reference
        (threeProducerMerge.token ++ threeProducerMerge.ast ++
          threeProducerMerge.annotation)).take (threeProducerMerge.limit + 1) →
    retainedByIdentity
      (producerWindow threeProducerMerge.token threeProducerMerge.limit ++
        producerWindow threeProducerMerge.ast threeProducerMerge.limit ++
        producerWindow threeProducerMerge.annotation threeProducerMerge.limit)
      item = true := by
  simp [threeProducerMerge, reference, productionSort, deduplicate, producerWindow,
    retainedByIdentity, fixtureCandidate, insertCandidate, productionBefore]
  rintro (rfl | rfl | rfl) <;> decide

set_option maxHeartbeats 100000 in
theorem three_producer_local_overflow_implies_global_overflow :
    (threeProducerMerge.limit < (reference threeProducerMerge.token).length ∨
      threeProducerMerge.limit < (reference threeProducerMerge.ast).length ∨
      threeProducerMerge.limit < (reference threeProducerMerge.annotation).length) →
    threeProducerMerge.limit <
      (reference
        (threeProducerMerge.token ++ threeProducerMerge.ast ++
          threeProducerMerge.annotation)).length := by
  decide

set_option maxHeartbeats 100000 in
theorem three_producer_candidates_match_global_prefix :
    (mergeProducerWindows threeProducerMerge.token threeProducerMerge.ast
      threeProducerMerge.annotation threeProducerMerge.limit).candidates =
    (reference
      (threeProducerMerge.token ++ threeProducerMerge.ast ++
        threeProducerMerge.annotation)).take threeProducerMerge.limit :=
  merge_candidates_eq_unbounded_reference_prefix _ _ _ _
    three_producer_window_is_global_identity_projection
    three_producer_top_rank_is_covered

set_option maxHeartbeats 100000 in
theorem three_producer_truncation_matches_global_overflow :
    (mergeProducerWindows threeProducerMerge.token threeProducerMerge.ast
      threeProducerMerge.annotation threeProducerMerge.limit).truncated = true ↔
    threeProducerMerge.limit <
      (reference
        (threeProducerMerge.token ++ threeProducerMerge.ast ++
          threeProducerMerge.annotation)).length :=
  merge_truncated_iff_unbounded_reference_overflows _ _ _ _
    three_producer_window_is_global_identity_projection
    three_producer_top_rank_is_covered
    three_producer_local_overflow_implies_global_overflow

set_option maxHeartbeats 100000 in
theorem three_producer_merge_matches_unbounded_reference :
    mergeProducerWindows threeProducerMerge.token threeProducerMerge.ast
      threeProducerMerge.annotation threeProducerMerge.limit =
    bounded
      (threeProducerMerge.token ++ threeProducerMerge.ast ++ threeProducerMerge.annotation)
      threeProducerMerge.limit := by
  decide

set_option maxHeartbeats 100000 in
theorem stable_equal_key_preserves_producer_input_order :
    (reference
      [candidate 1 10 .token true 0, candidate 2 10 .ast true 0]).map
        Candidate.identity = [1, 2] := by
  decide

def cases : List OracleCase :=
  [ outOfOrderDuplicate
  , threeProducerMerge
  , eligibilityBeforeCapacity
  , completeTwoTargets
  , truncatedNonFinalTarget
  , zeroLimit
  , usizeMax
  , publicTruncatedPlan ]

def brokenEmissionReference (items : List Candidate) : List Candidate :=
  deduplicate (items.filter Candidate.eligible)

def brokenCapacityBeforeEligibility (items : List Candidate) (limit : Nat) : List Candidate :=
  deduplicate (productionSort ((items.take limit).filter Candidate.eligible))

def brokenDedupAfterTake (items : List Candidate) (limit : Nat) : List Candidate :=
  deduplicate ((productionSort (items.filter Candidate.eligible)).take limit)

def lookaheadSensitivity : Bool :=
  producerWindow
      [candidate 1 10 .token, candidate 2 20 .token, candidate 3 30 .token] 2 !=
    (reference
      [candidate 1 10 .token, candidate 2 20 .token, candidate 3 30 .token]).take 2

def orderingSensitivity : Bool :=
  reference outOfOrderDuplicate.token != brokenEmissionReference outOfOrderDuplicate.token &&
    (reference
      [candidate 1 10 .token true 0, candidate 2 10 .ast true 0]).map
        Candidate.identity == [1, 2]

def eligibilitySensitivity : Bool :=
  (bounded eligibilityBeforeCapacity.token 1).candidates !=
    brokenCapacityBeforeEligibility eligibilityBeforeCapacity.token 1

def duplicateSensitivity : Bool :=
  let items :=
    [candidate 1 10 .token, candidate 1 10 .ast, candidate 2 20 .token]
  (bounded items 2).candidates != brokenDedupAfterTake items 2

def producerOverflowSensitivity : Bool :=
  let result := mergeProducerWindows
    [candidate 1 10 .token, candidate 2 20 .token, candidate 3 30 .token] [] [] 2
  result.truncated && result.candidates.length == 2

def brokenProducerOrderMerge
    (token ast annotation : List Candidate) (limit : Nat) : List Candidate :=
  (producerWindow token limit ++ producerWindow ast limit ++
    producerWindow annotation limit).take limit

def mergeOrderSensitivity : Bool :=
  let token := [candidate 3 30 .token]
  let ast := [candidate 1 10 .ast]
  let annotation := [candidate 2 20 .annotation]
  let expected := mergeProducerWindows token ast annotation 2
  expected.candidates != brokenProducerOrderMerge token ast annotation 2

def globalCapacitySensitivity : Bool :=
  let result := discoverTargets completeTwoTargets.targets 1
  result.candidates.length == 1 && result.targetsRead == 2 && result.truncated

def terminalSensitivity : Bool :=
  let result := discoverTargets truncatedNonFinalTarget.targets 1
  result.targetsRead == 1 && result.spoolFinished && result.truncated

def publicProjectionSensitivity : Bool :=
  let expected := observe publicTruncatedPlan
  expected.truncated && expected.identities.length == 1 &&
    expected.candidateLimitDiagnostic && expected.exitCode == 4

def sensitivityPasses : Bool :=
  lookaheadSensitivity && orderingSensitivity && eligibilitySensitivity &&
    duplicateSensitivity && producerOverflowSensitivity && mergeOrderSensitivity &&
    globalCapacitySensitivity && terminalSensitivity && publicProjectionSensitivity

def caseSafe (item : OracleCase) : Bool :=
  let result := observe item
  result.identities.length ≤ item.limit &&
    result.sequences == sequences result.identities.length &&
    (if item.scenario == "ordered_targets" && !item.targets.isEmpty then
      result.spoolFinished else true)

end HoiminOracle.BoundedCandidateDiscovery
