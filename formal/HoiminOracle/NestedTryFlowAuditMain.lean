import HoiminOracle.NestedTryFlowCases
import Lean.Data.Json

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

namespace HoiminOracle.NestedTryFlow.Executable

private def categoryName : ExitCategory → String
  | .fallthrough => "fallthrough"
  | .break => "break"
  | .continue => "continue"
  | .terminate => "terminate"

private def candidateName : CandidateExpectation → String
  | .notObserved => "not_observed"
  | .present => "present"
  | .absent => "absent"

private def factStrings (environment : Env) : List String :=
  let source := match environment.source with
    | .known .typing => ["direct:Sequence=typing.Sequence"]
    | .known .builtin => ["direct:Sequence=builtin.Sequence"]
    | .absent | .shadowed | .unknown => []
  let destination := match environment.destination with
    | .known .typing => ["direct:Mapping=typing.Mapping"]
    | .known .builtin => ["direct:Mapping=builtin.Mapping"]
    | .absent | .shadowed | .unknown => []
  destination ++ source

private def stateJson (environment : Env) : Lean.Json :=
  .arr ((factStrings environment).toArray.map Lean.toJson)

private def statesJson (states : List Env) : Lean.Json :=
  .arr (states.toArray.map stateJson)

private def exitsJson (exits : Exits) : Lean.Json :=
  Lean.Json.mkObj [
    ("fallthrough", statesJson exits.fallthrough.toList),
    ("breaks", statesJson exits.breaks),
    ("continues", statesJson exits.continues),
    ("terminates", statesJson exits.terminates)
  ]

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("family", .str item.family),
    ("source", .str item.source),
    ("marker", .str item.marker),
    ("entry_category", .str (categoryName item.entryCategory)),
    ("expected", exitsJson item.expected),
    ("candidate", .str (candidateName item.candidate))
  ]

def renderCorpus : String :=
  String.join (nestedTryFlowCases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "nested try flow audit did not distinguish every broken variant"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "nested try flow fixed cases violate the corpus contract"
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
        IO.eprintln s!"stale Lean nested try flow corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean nested try flow corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"cleanup_boundary_detected={cleanupBoundarySensitivity}"
  IO.println s!"finalizer_coverage_detected={finalizerCoverageSensitivity}"
  IO.println s!"category_preservation_detected={categoryPreservationSensitivity}"
  IO.println s!"abrupt_replacement_detected={abruptReplacementSensitivity}"
  IO.println s!"unreachable_join_detected={unreachableJoinSensitivity}"
  IO.println s!"omitted_abrupt_detected={omittedAbruptSensitivity}"
  IO.println s!"cleanup_idempotency_detected={cleanupIdempotencySensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in nestedTryFlowCases do
    IO.println s!"{item.id}={item.schema == 1 && validMode item.mode}"
  return if fixedCasesPass then 0 else 2

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println "schema=1"
      IO.println "generated_depth=0"
      IO.println "event_alphabet=0"
      IO.println s!"fixed_cases={nestedTryFlowCases.length}"
      IO.println s!"strict_cases={(nestedTryFlowCases.filter fun item => item.mode == "strict").length}"
      IO.println s!"internal_fixture_cases={(nestedTryFlowCases.filter fun item => item.mode == "internal-fixture").length}"
      IO.println s!"model_only_cases={(nestedTryFlowCases.filter fun item => item.mode == "model-only").length}"
      IO.println "sensitivity_families=7"
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
      IO.eprintln "usage: generate_nested_try_flow --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.NestedTryFlow.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.NestedTryFlow.Executable.main args
