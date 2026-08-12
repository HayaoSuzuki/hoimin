import HoiminOracle.ProgressDecisionProofs

namespace HoiminOracle.ProgressDecision

inductive StatusTag
  | killed
  | survived
  | timeout
  | outOfMemory
  | processLimit
  | error
  | notRun
  deriving Repr, DecidableEq, BEq

def StatusTag.modelStatus : StatusTag → Status
  | .killed => .killed
  | .survived => .survived
  | .timeout | .outOfMemory | .processLimit | .error | .notRun => .inconclusive

inductive UnusableReason
  | missingBaseline
  | baselineFailed
  | incomplete
  deriving Repr, DecidableEq, BEq

structure OracleMutant where
  candidateId : Nat
  contentKey : Nat
  status : StatusTag
  deriving Repr, DecidableEq, BEq

inductive OracleReport
  | unusable (reason : UnusableReason)
  | usable (mutants : List OracleMutant)
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String := "strict"
  patience : Nat
  reports : List OracleReport
  deriving Repr, DecidableEq, BEq

def OracleMutant.toModel (mutant : OracleMutant) : Mutant :=
  { candidateId := mutant.candidateId
    contentKey := mutant.contentKey
    status := mutant.status.modelStatus }

def OracleReport.toModel : OracleReport → Report
  | .unusable _ => .unusable
  | .usable mutants => .usable (mutants.map OracleMutant.toModel)

def OracleCase.observed (item : OracleCase) : HistoryObservation :=
  compareHistory (item.reports.map OracleReport.toModel) item.patience

def om (candidateId contentKey : Nat) (status : StatusTag) : OracleMutant :=
  { candidateId, contentKey, status }

def oru (mutants : List OracleMutant) : OracleReport := .usable mutants

def one (status : StatusTag) : OracleReport := oru [om 0 0 status]

def progressDecisionCases : List OracleCase := [
  { id := "patience_1_first_stall", patience := 1,
    reports := [one .killed, one .killed] },
  { id := "patience_2_two_stalls", patience := 2,
    reports := [one .killed, one .killed, one .killed] },
  { id := "patience_3_short_suffix", patience := 3,
    reports := [one .killed, one .killed, one .killed] },
  { id := "improvement_resets_stalls", patience := 2,
    reports := [one .survived, one .survived, one .killed] },
  { id := "regression_resets_stalls", patience := 2,
    reports := [one .killed, one .killed, one .survived] },
  { id := "simultaneous_regression_wins", patience := 2,
    reports := [
      oru [om 0 0 .killed, om 1 1 .survived],
      oru [om 0 0 .survived, om 1 1 .killed]] },
  { id := "inconclusive_resets_stalls", patience := 2,
    reports := [one .killed, one .killed, one .timeout, one .killed, one .killed] },
  { id := "unusable_gap_resets_stalls", patience := 2,
    reports := [one .killed, one .killed, .unusable .incomplete,
      one .killed, one .killed] },
  { id := "matching_ids_ignore_duplicate_content", patience := 2,
    reports := [
      oru [om 0 0 .killed, om 1 0 .survived],
      oru [om 0 0 .survived, om 1 0 .killed]] },
  { id := "different_ids_are_indeterminate", patience := 2,
    reports := [oru [om 0 0 .killed], oru [om 1 0 .killed]] },
  { id := "different_ids_duplicate_content_is_ambiguous", patience := 2,
    reports := [oru [om 0 0 .killed, om 1 0 .survived], oru [om 0 0 .killed]] },
  { id := "added_and_removed_counts", patience := 2,
    reports := [oru [om 0 0 .killed], oru [om 1 1 .survived]] },
  { id := "empty_comparable_common", patience := 2,
    reports := [oru [om 0 0 .killed], oru [om 1 1 .killed]] },
  { id := "score_zero", patience := 2,
    reports := [one .survived, one .survived] },
  { id := "score_half", patience := 2,
    reports := [
      oru [om 0 0 .killed, om 1 1 .survived],
      oru [om 0 0 .killed, om 1 1 .survived]] },
  { id := "score_one", patience := 2,
    reports := [one .killed, one .killed] },
  { id := "inconclusive_timeout", patience := 2,
    reports := [one .survived, one .timeout] },
  { id := "inconclusive_out_of_memory", patience := 2,
    reports := [one .survived, one .outOfMemory] },
  { id := "inconclusive_process_limit", patience := 2,
    reports := [one .survived, one .processLimit] },
  { id := "inconclusive_error", patience := 2,
    reports := [one .survived, one .error] },
  { id := "inconclusive_not_run", patience := 2,
    reports := [one .survived, one .notRun] },
  { id := "unusable_missing_baseline", patience := 2,
    reports := [one .killed, .unusable .missingBaseline] },
  { id := "unusable_baseline_failed", patience := 2,
    reports := [one .killed, .unusable .baselineFailed] },
  { id := "duplicate_candidate_id_model_only", mode := "model-only", patience := 2,
    reports := [
      oru [om 0 0 .killed, om 0 1 .survived],
      oru [om 0 0 .killed, om 0 1 .survived]] }
]

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "model-only"

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 && validMode item.mode && item.patience > 0 &&
    item.reports.length > 0

def firstComparison? (item : OracleCase) : Option PairObservation :=
  item.observed.comparisons.head?

def fixedExpectationSafe (item : OracleCase) : Bool :=
  let observed := item.observed
  match item.id with
  | "patience_1_first_stall" =>
      observed.latest == .saturated && observed.consecutiveStalls == 1
  | "patience_2_two_stalls" =>
      observed.latest == .saturated && observed.consecutiveStalls == 2
  | "patience_3_short_suffix" =>
      observed.latest == .stalled && observed.consecutiveStalls == 2
  | "improvement_resets_stalls" =>
      observed.latest == .improving && observed.consecutiveStalls == 0
  | "regression_resets_stalls" =>
      observed.latest == .regressing && observed.consecutiveStalls == 0
  | "simultaneous_regression_wins" =>
      match firstComparison? item with
      | some comparison => comparison.state == .regressing &&
          comparison.counts.improvements == 1 && comparison.counts.regressions == 1
      | none => false
  | "inconclusive_resets_stalls" | "unusable_gap_resets_stalls" =>
      observed.latest == .stalled && observed.consecutiveStalls == 1
  | "matching_ids_ignore_duplicate_content" =>
      match firstComparison? item with
      | some comparison => comparison.eligibility == .matching &&
          comparison.counts.ambiguous == 0 && comparison.state == .regressing
      | none => false
  | "different_ids_are_indeterminate" =>
      match firstComparison? item with
      | some comparison => comparison.eligibility == .different &&
          comparison.counts.common == 1 && comparison.state == .indeterminate
      | none => false
  | "different_ids_duplicate_content_is_ambiguous" =>
      match firstComparison? item with
      | some comparison => comparison.counts.ambiguous == 1 &&
          comparison.counts.common == 0 && comparison.state == .indeterminate
      | none => false
  | "added_and_removed_counts" =>
      match firstComparison? item with
      | some comparison => comparison.counts.added == 1 &&
          comparison.counts.removed == 1
      | none => false
  | "empty_comparable_common" =>
      match firstComparison? item with
      | some comparison => comparison.counts.common == 0 &&
          comparison.state == .indeterminate
      | none => false
  | "score_zero" =>
      match firstComparison? item with
      | some comparison => comparison.counts.previousKilled == 0 &&
          comparison.counts.previousSurvived == 1
      | none => false
  | "score_half" =>
      match firstComparison? item with
      | some comparison => comparison.counts.previousKilled == 1 &&
          comparison.counts.previousSurvived == 1
      | none => false
  | "score_one" =>
      match firstComparison? item with
      | some comparison => comparison.counts.previousKilled == 1 &&
          comparison.counts.previousSurvived == 0
      | none => false
  | "inconclusive_timeout" | "inconclusive_out_of_memory" |
      "inconclusive_process_limit" | "inconclusive_error" | "inconclusive_not_run" =>
      match firstComparison? item with
      | some comparison => comparison.counts.inconclusive == 1 &&
          comparison.counts.previousKilled + comparison.counts.previousSurvived == 0 &&
          comparison.state == .indeterminate
      | none => false
  | "unusable_missing_baseline" | "unusable_baseline_failed" =>
      observed.comparisons.isEmpty && observed.latest == .indeterminate
  | "duplicate_candidate_id_model_only" =>
      match firstComparison? item with
      | some comparison => comparison.eligibility == .duplicate &&
          comparison.state == .indeterminate
      | none => false
  | _ => false

def fixedCasesPass : Bool :=
  progressDecisionCases.all fun item => caseSafe item && fixedExpectationSafe item

def brokenImprovementFirst
    (pairEligibility : Eligibility)
    (comparableCommon regressions improvements : Nat) : PairState :=
  if pairEligibility != .matching || comparableCommon = 0 then .indeterminate
  else if improvements > 0 then .improving
  else if regressions > 0 then .regressing
  else .stalled

def brokenKeepIndeterminateSuffix (count : Nat) : PairState → Nat
  | .indeterminate => count
  | .stalled => count + 1
  | .improving | .regressing => 0

def brokenKeepUnusableSuffix (count : Nat) : PairStep → Nat
  | .unusableAdjacency => count
  | .compared observation => stallCountStep count (.compared observation)

def brokenStrictPatience (stalls patience : Nat) : Bool := stalls > patience

def brokenJoinMatchingByContent : Eligibility → JoinMode
  | .matching | .different | .duplicate => .contentKey

def brokenDuplicateContentUnique
    (previous current : List Mutant) : Counts :=
  let mode := JoinMode.contentKey
  let allKeys := (keys mode previous ++ keys mode current).eraseDups
  allKeys.foldl
    (fun counts key => Counts.add counts (keyCounts mode previous current key))
    ({} : Counts)

def brokenCountInconclusive (before after : Status) : Counts :=
  let base := transitionCounts before after
  if before == .inconclusive || after == .inconclusive then
    { base with
      previousSurvived := 1
      currentSurvived := 1
      carriedSurvivors := 1 }
  else base

def precedenceSensitivity : Bool :=
  classifyPair .matching 2 1 1 == .regressing &&
    brokenImprovementFirst .matching 2 1 1 == .improving

def indeterminateResetSensitivity : Bool :=
  stallCountStep 1 (.compared {
    eligibility := .different, counts := {}, state := .indeterminate }) == 0 &&
  brokenKeepIndeterminateSuffix 1 .indeterminate == 1

def unusableResetSensitivity : Bool :=
  stallCountStep 1 .unusableAdjacency == 0 &&
    brokenKeepUnusableSuffix 1 .unusableAdjacency == 1

def oneStalledObservation : PairObservation :=
  { eligibility := .matching
    counts := { common := 1, previousKilled := 1, currentKilled := 1 }
    state := .stalled }

def patienceSensitivity : Bool :=
  latestWithPatience [.compared oneStalledObservation] 1 == .saturated &&
    !(brokenStrictPatience 1 1)

def matchingJoinSensitivity : Bool :=
  joinMode .matching == .candidateId &&
    brokenJoinMatchingByContent .matching == .contentKey

def duplicateContentSensitivity : Bool :=
  let previous := [{ candidateId := 0, contentKey := 0, status := .killed },
    { candidateId := 1, contentKey := 0, status := .survived }]
  let current := [{ candidateId := 0, contentKey := 0, status := .killed }]
  (comparePair previous current).counts.ambiguous == 1 &&
    (brokenDuplicateContentUnique previous current).common == 1

def inconclusiveSensitivity : Bool :=
  let correct := transitionCounts .survived .inconclusive
  let broken := brokenCountInconclusive .survived .inconclusive
  correct.carriedSurvivors == 0 && correct.previousSurvived == 0 &&
    broken.carriedSurvivors == 1 && broken.previousSurvived == 1

def sensitivityPasses : Bool :=
  precedenceSensitivity && indeterminateResetSensitivity &&
    unusableResetSensitivity && patienceSensitivity && matchingJoinSensitivity &&
    duplicateContentSensitivity && inconclusiveSensitivity

example : fixedCasesPass = true := by native_decide
example : sensitivityPasses = true := by native_decide

end HoiminOracle.ProgressDecision
