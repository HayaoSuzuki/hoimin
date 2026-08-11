import HoiminOracle.ExceptionMatchBindingCases
import Lean.Data.Json

open HoiminOracle.ExceptionMatchBinding

namespace HoiminOracle.ExceptionMatchBinding.Executable

private def optionStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.map Lean.Json.str).toArray

private def optionLabel : Option String → String
  | none => "none"
  | some value => value

private def factsLabel (values : List String) : String :=
  if values.isEmpty then "none" else String.intercalate ";" values

private def optionalFactsLabel : Option (List String) → String
  | none => "unreachable"
  | some values => factsLabel values

private def caseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("family", .str item.family.label),
  ("observation_kind", .str item.observationKind.label),
  ("source", .str item.source),
  ("marker", .str item.marker),
  ("name", .str item.name),
  ("expected_facts", stringsJson item.expectedFacts),
  ("expected_resolution", optionStringJson item.expectedResolution),
  ("expected_exit_category", optionStringJson item.expectedExitCategory),
  ("expected_present", Lean.toJson item.expectedPresent),
  ("operator", optionStringJson item.operator),
  ("original", optionStringJson item.original),
  ("replacement", optionStringJson item.replacement),
  ("symbol", optionStringJson item.symbol)
]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless fixedCasesPass do
    IO.eprintln "exception-match-binding fixed cases violate the closed schema"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "exception-match-binding audit did not distinguish every broken family"
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
        IO.eprintln s!"stale Lean exception-match-binding corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean exception-match-binding corpus {path}: {error}"
        return 1

private def countFamily (family : Family) : Nat :=
  (cases.filter fun item => item.family == family).length

private def countMode (mode : String) : Nat :=
  (cases.filter fun item => item.mode == mode).length

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println "structured_depth=0"
      IO.println "generated_depth_expansion=false"
      IO.println s!"fixed_cases={cases.length}"
      IO.println s!"handler_cases={countFamily .handler}"
      IO.println s!"match_cases={countFamily .matchCase}"
      IO.println s!"internal_fixture_cases={countMode "internal-fixture"}"
      IO.println s!"model_only_cases={countMode "model-only"}"
      IO.println s!"strict_cases={countMode "strict"}"
      IO.println "sensitivity_families=7"
      IO.println "broken_families=bind-before-type,handler-exit-cleanup,handler-join-meet,pattern-failure,guard-failure,refutable-unmatched,irrefutable-exhaustion"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"bind_before_type_detected={bindBeforeTypeSensitivity}"
  IO.println s!"handler_exit_cleanup_detected={handlerExitCleanupSensitivity}"
  IO.println s!"handler_fallthrough_cleanup_detected={handlerFallthroughCleanupSensitivity}"
  IO.println s!"handler_break_cleanup_detected={handlerBreakCleanupSensitivity}"
  IO.println s!"handler_continue_cleanup_detected={handlerContinueCleanupSensitivity}"
  IO.println s!"handler_terminate_cleanup_detected={handlerTerminateCleanupSensitivity}"
  IO.println s!"handler_join_meet_detected={handlerJoinMeetSensitivity}"
  IO.println s!"handler_join_projection=expected:{optionLabel handlerJoinResolution},broken:{optionLabel handlerJoinBrokenResolution}"
  IO.println s!"handler_join_observable=expected:{factsLabel handlerJoinFacts},broken:{factsLabel handlerJoinBrokenFacts}"
  IO.println s!"pattern_failure_detected={patternFailureSensitivity}"
  IO.println s!"pattern_failure_projection=expected:{optionLabel matchPartialFailureResolution},broken:{optionLabel patternFailureBrokenResolution}"
  IO.println s!"pattern_failure_observable=expected:{optionalFactsLabel matchPartialFailureObservableFacts},broken:{optionalFactsLabel patternFailureBrokenObservableFacts}"
  IO.println s!"guard_failure_detected={guardFailureSensitivity}"
  IO.println s!"guard_failure_observable=expected:{optionalFactsLabel matchFalseGuardObservableFacts},broken:{optionalFactsLabel guardFailureBrokenObservableFacts}"
  IO.println s!"refutable_unmatched_detected={refutableUnmatchedSensitivity}"
  IO.println s!"refutable_unmatched_observable=expected:{factsLabel matchRefutableJoinFacts},broken:{factsLabel refutableUnmatchedBrokenFacts}"
  IO.println s!"irrefutable_exhaustion_detected={irrefutableExhaustionSensitivity}"
  IO.println s!"irrefutable_projection=expected:{factsLabel matchIrrefutableFacts},broken:{factsLabel irrefutableBrokenFacts}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do
    IO.println s!"{item.id}={item.valid cases},present={item.expectedPresent}"
  return if fixedCasesPass then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--cases"] => printCases
  | ["--sensitivity"] => printSensitivity
  | ["--stats"] => printStats
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | _ =>
      IO.eprintln "usage: generate_exception_match_binding --cases | --sensitivity | --stats | --output PATH | --check PATH"
      return 2

end HoiminOracle.ExceptionMatchBinding.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ExceptionMatchBinding.Executable.main args
