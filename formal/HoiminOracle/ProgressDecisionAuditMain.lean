import HoiminOracle.ProgressDecisionCases
import Lean.Data.Json

open HoiminOracle.ProgressDecision

namespace HoiminOracle.ProgressDecision.Executable

private def statusName : StatusTag → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

private def reasonName : UnusableReason → String
  | .missingBaseline => "missing_baseline"
  | .baselineFailed => "baseline_failed"
  | .incomplete => "incomplete"

private def pairStateName : PairState → String
  | .improving => "improving"
  | .regressing => "regressing"
  | .stalled => "stalled"
  | .indeterminate => "indeterminate"

private def latestStateName : LatestState → String
  | .improving => "improving"
  | .regressing => "regressing"
  | .stalled => "stalled"
  | .saturated => "saturated"
  | .indeterminate => "indeterminate"

private def eligibilityName : Eligibility → String
  | .matching => "matching"
  | .different => "different"
  | .duplicate => "duplicate"

private def mutantJson (mutant : OracleMutant) : Lean.Json :=
  Lean.Json.mkObj [
    ("candidate_id", Lean.toJson mutant.candidateId),
    ("content_key", Lean.toJson mutant.contentKey),
    ("status", .str (statusName mutant.status))
  ]

private def reportJson : OracleReport → Lean.Json
  | .unusable reason => Lean.Json.mkObj [
      ("usable", Lean.toJson false),
      ("reason", .str (reasonName reason)),
      ("mutants", .arr #[])
    ]
  | .usable mutants => Lean.Json.mkObj [
      ("usable", Lean.toJson true),
      ("reason", .null),
      ("mutants", .arr (mutants.toArray.map mutantJson))
    ]

private def scoreJson (killed survived : Nat) : Lean.Json :=
  let decidable := killed + survived
  if decidable = 0 then .null
  else Lean.Json.mkObj [
    ("killed", Lean.toJson killed),
    ("decidable", Lean.toJson decidable)
  ]

private def scoreDeltaJson
    (previousKilled previousSurvived currentKilled currentSurvived : Nat) : Lean.Json :=
  let previousDecidable := previousKilled + previousSurvived
  let currentDecidable := currentKilled + currentSurvived
  if previousDecidable = 0 || currentDecidable = 0 then .null
  else
    let numerator :=
      Int.ofNat (currentKilled * previousDecidable) -
        Int.ofNat (previousKilled * currentDecidable)
    Lean.Json.mkObj [
      ("numerator", Lean.toJson numerator),
      ("denominator", Lean.toJson (previousDecidable * currentDecidable))
    ]

private def comparisonJson (comparison : PairObservation) : Lean.Json :=
  let counts := comparison.counts
  Lean.Json.mkObj [
    ("eligibility", .str (eligibilityName comparison.eligibility)),
    ("common", Lean.toJson counts.common),
    ("added", Lean.toJson counts.added),
    ("removed", Lean.toJson counts.removed),
    ("ambiguous", Lean.toJson counts.ambiguous),
    ("inconclusive", Lean.toJson counts.inconclusive),
    ("improvements", Lean.toJson counts.improvements),
    ("regressions", Lean.toJson counts.regressions),
    ("carried_survivors", Lean.toJson counts.carriedSurvivors),
    ("previous_score", scoreJson counts.previousKilled counts.previousSurvived),
    ("current_score", scoreJson counts.currentKilled counts.currentSurvived),
    ("score_delta", scoreDeltaJson counts.previousKilled counts.previousSurvived
      counts.currentKilled counts.currentSurvived),
    ("state", .str (pairStateName comparison.state))
  ]

private def caseJson (item : OracleCase) : Lean.Json :=
  let expected := item.observed
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("patience", Lean.toJson item.patience),
    ("reports", .arr (item.reports.toArray.map reportJson)),
    ("expected", Lean.Json.mkObj [
      ("latest_state", .str (latestStateName expected.latest)),
      ("consecutive_stalls", Lean.toJson expected.consecutiveStalls),
      ("saturated", Lean.toJson (expected.latest == .saturated)),
      ("comparisons", .arr (expected.comparisons.toArray.map comparisonJson))
    ])
  ]

def renderCorpus : String :=
  String.join (progressDecisionCases.map fun item => (caseJson item).compress ++ "\n")

private def statusDomain : List Status := [.killed, .survived, .inconclusive]
private def roleDomain : List Nat := [0, 1]

private def singleMutants : List (List Mutant) :=
  roleDomain.flatMap fun candidateId =>
    roleDomain.flatMap fun contentKey =>
      statusDomain.map fun status => [{ candidateId, contentKey, status }]

private def doubleMutants : List (List Mutant) :=
  roleDomain.flatMap fun leftContent =>
    statusDomain.flatMap fun leftStatus =>
      roleDomain.flatMap fun rightContent =>
        statusDomain.map fun rightStatus => [
          { candidateId := 0, contentKey := leftContent, status := leftStatus },
          { candidateId := 1, contentKey := rightContent, status := rightStatus }
        ]

private def reportDomain : List Report :=
  .unusable :: .usable [] ::
    (singleMutants ++ doubleMutants).map Report.usable

private def specClassify (observation : PairObservation) : PairState :=
  let counts := observation.counts
  let comparable := counts.previousKilled + counts.previousSurvived
  if observation.eligibility != .matching || comparable = 0 then .indeterminate
  else if 0 < counts.regressions then .regressing
  else if 0 < counts.improvements then .improving
  else .stalled

private def pairSafe : PairStep → Bool
  | .unusableAdjacency => true
  | .compared observation => observation.state == specClassify observation

private def boundedPairPass : Bool :=
  reportDomain.all fun previous =>
    reportDomain.all fun current => pairSafe (pairStep previous current)

private def abstractObservation (state : PairState) : PairObservation :=
  match state with
  | .improving =>
      { eligibility := .matching
        counts := { common := 1, improvements := 1, previousSurvived := 1, currentKilled := 1 }
        state := .improving }
  | .regressing =>
      { eligibility := .matching
        counts := { common := 1, regressions := 1, previousKilled := 1, currentSurvived := 1 }
        state := .regressing }
  | .stalled =>
      { eligibility := .matching
        counts := { common := 1, previousKilled := 1, currentKilled := 1 }
        state := .stalled }
  | .indeterminate =>
      { eligibility := .different, counts := {}, state := .indeterminate }

private def abstractSteps : List PairStep := [
  .unusableAdjacency,
  .compared (abstractObservation .improving),
  .compared (abstractObservation .regressing),
  .compared (abstractObservation .stalled),
  .compared (abstractObservation .indeterminate)
]

private def tracesExact : Nat → List (List PairStep)
  | 0 => [[]]
  | depth + 1 => abstractSteps.flatMap fun step =>
      (tracesExact depth).map fun rest => step :: rest

private def tracesUpTo (depth : Nat) : List (List PairStep) :=
  (List.range (depth + 1)).flatMap tracesExact

private def specTrailing (steps : List PairStep) : Nat :=
  (steps.reverse.takeWhile isStalledStep).length

private def specBaseLatest : Option PairStep → LatestState
  | none | some .unusableAdjacency => .indeterminate
  | some (.compared observation) =>
      match observation.state with
      | .improving => .improving
      | .regressing => .regressing
      | .stalled => .stalled
      | .indeterminate => .indeterminate

private def specLatest (steps : List PairStep) (patience : Nat) : LatestState :=
  let suffix := specTrailing steps
  if 0 < patience && patience ≤ suffix then .saturated
  else specBaseLatest steps.getLast?

private def historySafe (steps : List PairStep) (patience : Nat) : Bool :=
  let observed := foldPairStepsWithPatience steps patience
  observed.consecutiveStalls == specTrailing steps &&
    observed.latest == specLatest steps patience

private def boundedHistoryPass : Bool :=
  (tracesUpTo 4).all fun steps =>
    [1, 2, 3].all fun patience => historySafe steps patience

def boundedCheck : Bool := boundedPairPass && boundedHistoryPass

private def reportPairCount : Nat := reportDomain.length * reportDomain.length
private def historyCount : Nat := (tracesUpTo 4).length * 3

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "progress decision audit did not distinguish every broken variant"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "progress decision fixed cases violate the corpus contract"
    return .error 2
  unless boundedCheck do
    IO.eprintln "progress decision bounded semantic check found a counterexample"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      IO.FS.writeFile path renderCorpus
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then return 0
        IO.eprintln s!"stale Lean progress decision corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean progress decision corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"regression_precedence_detected={precedenceSensitivity}"
  IO.println s!"indeterminate_reset_detected={indeterminateResetSensitivity}"
  IO.println s!"unusable_reset_detected={unusableResetSensitivity}"
  IO.println s!"patience_boundary_detected={patienceSensitivity}"
  IO.println s!"matching_id_join_detected={matchingJoinSensitivity}"
  IO.println s!"duplicate_content_detected={duplicateContentSensitivity}"
  IO.println s!"inconclusive_exclusion_detected={inconclusiveSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in progressDecisionCases do
    IO.println s!"{item.id}={caseSafe item}"
  return if fixedCasesPass then 0 else 2

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println "schema=1"
      IO.println "max_history_length=4"
      IO.println "patience_min=1"
      IO.println "patience_max=3"
      IO.println s!"report_domain={reportDomain.length}"
      IO.println s!"pair_checks={reportPairCount}"
      IO.println s!"history_checks={historyCount}"
      IO.println s!"fixed_cases={progressDecisionCases.length}"
      IO.println s!"strict_cases={(progressDecisionCases.filter fun item => item.mode == "strict").length}"
      IO.println s!"internal_fixture_cases={(progressDecisionCases.filter fun item => item.mode == "internal-fixture").length}"
      IO.println s!"model_only_cases={(progressDecisionCases.filter fun item => item.mode == "model-only").length}"
      return 0

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | _ => do
      IO.eprintln "usage: generate_progress_decision --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.ProgressDecision.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ProgressDecision.Executable.main args
