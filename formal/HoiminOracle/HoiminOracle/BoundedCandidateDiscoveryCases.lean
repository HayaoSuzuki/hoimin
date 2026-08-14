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
  deriving Repr, DecidableEq, BEq

def candidate
    (identity orderKey : Nat) (producer : Producer)
    (eligible : Bool := true) (emissionIndex : Nat := 0) : Candidate where
  identity := identity
  orderKey := orderKey
  producer := producer
  eligible := eligible
  emissionIndex := emissionIndex

def discoveryObservation (result : Discovery) : Observation where
  identities := result.candidates.map Candidate.identity
  truncated := result.truncated
  sequences := sequences result.candidates.length
  targetsRead := 0
  spoolFinished := false

def targetObservation (result : TargetState) : Observation where
  identities := result.candidates.map Candidate.identity
  truncated := result.truncated
  sequences := sequences result.candidates.length
  targetsRead := result.targetsRead
  spoolFinished := result.spoolFinished

def observe (item : OracleCase) : Observation :=
  match item.scenario with
  | "producer_merge" =>
      discoveryObservation
        (mergeProducerWindows item.token item.ast item.annotation item.limit)
  | "ordered_targets" => targetObservation (discoverTargets item.targets item.limit)
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
  token := [candidate 3 30 .token]
  ast := [candidate 1 10 .ast]
  annotation := [candidate 2 20 .annotation]

def eligibilityBeforeCapacity : OracleCase where
  id := "eligibility_before_capacity"
  mode := "internal-fixture"
  scenario := "candidate_prefix"
  limit := 1
  token :=
    [ candidate 9 5 .token false
    , candidate 1 10 .token
    , candidate 2 20 .token ]

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
  token := [candidate 1 10 .token]

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
  token := [candidate 1 10 .token, candidate 2 20 .token]

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
  reference outOfOrderDuplicate.token != brokenEmissionReference outOfOrderDuplicate.token

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

def globalCapacitySensitivity : Bool :=
  let result := discoverTargets completeTwoTargets.targets 1
  result.candidates.length == 1 && result.targetsRead == 2 && result.truncated

def terminalSensitivity : Bool :=
  let result := discoverTargets truncatedNonFinalTarget.targets 1
  result.targetsRead == 1 && result.spoolFinished && result.truncated

def publicProjectionSensitivity : Bool :=
  let expected := observe publicTruncatedPlan
  expected.truncated && expected.identities.length == 1

def sensitivityPasses : Bool :=
  lookaheadSensitivity && orderingSensitivity && eligibilitySensitivity &&
    duplicateSensitivity && producerOverflowSensitivity &&
    globalCapacitySensitivity && terminalSensitivity && publicProjectionSensitivity

def caseSafe (item : OracleCase) : Bool :=
  let result := observe item
  result.identities.length ≤ item.limit &&
    result.sequences == sequences result.identities.length &&
    (if item.scenario == "ordered_targets" && !item.targets.isEmpty then
      result.spoolFinished else true)

end HoiminOracle.BoundedCandidateDiscovery
