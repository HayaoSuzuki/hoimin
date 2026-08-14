import HoiminOracle.CompoundPatternGuardCases
import Lean.Data.Json

open HoiminOracle.CompoundPatternGuard

namespace HoiminOracle.CompoundPatternGuard.Executable

private def observationName : ObservationKind → String
  | .caseEntry => "case_entry"
  | .nextCaseEntry => "next_case_entry"
  | .publicCandidate => "public_candidate"
  | .modelWitness => "model_witness"

private def optionStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def optionNatJson : Option Nat → Lean.Json
  | none => .null
  | some value => Lean.toJson value

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("observation_kind", .str (observationName item.observationKind)),
    ("family", .str item.family),
    ("source", .str item.source),
    ("marker", .str item.marker),
    ("expected_facts", Lean.toJson item.expectedFacts),
    ("candidate_count", Lean.toJson item.candidateCount),
    ("candidate_path", optionStringJson item.candidatePath),
    ("candidate_start", optionNatJson item.candidateStart),
    ("candidate_length", optionNatJson item.candidateLength),
    ("candidate_operator", optionStringJson item.candidateOperator),
    ("candidate_original", optionStringJson item.candidateOriginal),
    ("candidate_replacement", optionStringJson item.candidateReplacement),
    ("candidate_symbol", optionStringJson item.candidateSymbol)
  ]

def renderCorpus : String :=
  String.join (compoundPatternGuardCases.map fun item =>
    (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "compound pattern audit did not distinguish every broken variant"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "compound pattern fixed cases violate the corpus contract"
    return .error 2
  return .ok ()

example : sensitivityPasses = true := by native_decide
example : fixedCasesPass = true := by native_decide

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
        IO.eprintln s!"stale Lean compound pattern corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean compound pattern corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"pre_pattern_failure_detected={prePatternFailureSensitivity}"
  IO.println s!"late_capture_on_failure_detected={lateCaptureFailureSensitivity}"
  IO.println s!"or_keep_first_success_detected={orKeepFirstSuccessSensitivity}"
  IO.println s!"or_keep_last_success_detected={orKeepLastSuccessSensitivity}"
  IO.println s!"or_drop_failure_detected={orDropFailureSensitivity}"
  IO.println s!"pre_guard_failure_detected={preGuardFailureSensitivity}"
  IO.println s!"unreachable_outcome_detected={unreachableOutcomeSensitivity}"
  IO.println s!"overbroad_cleanup_detected={overbroadCleanupSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in compoundPatternGuardCases do
    IO.println s!"{item.id}={caseValid item}"
  return if fixedCasesPass then 0 else 2

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println "schema=1"
      IO.println "generated_depth=0"
      IO.println "event_alphabet=0"
      IO.println "explored_states=0"
      IO.println "transitions=0"
      IO.println s!"fixed_cases={compoundPatternGuardCases.length}"
      IO.println s!"strict_cases={(compoundPatternGuardCases.filter fun item => item.mode == "strict").length}"
      IO.println s!"internal_fixture_cases={(compoundPatternGuardCases.filter fun item => item.mode == "internal-fixture").length}"
      IO.println s!"model_only_cases={(compoundPatternGuardCases.filter fun item => item.mode == "model-only").length}"
      IO.println "sensitivity_families=8"
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
      IO.eprintln "usage: generate_compound_pattern_guards --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.CompoundPatternGuard.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.CompoundPatternGuard.Executable.main args
