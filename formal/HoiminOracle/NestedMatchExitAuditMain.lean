import HoiminOracle.NestedMatchExitCases
import Lean.Data.Json

open HoiminOracle.BindingFlow
open HoiminOracle.NestedMatchExit

namespace HoiminOracle.NestedMatchExit.Executable

private def observationName : ObservationKind → String
  | .tryExit => "try_exit"
  | .loopHead => "loop_head"
  | .publicCandidate => "public_candidate"

private def candidateName : CandidateExpectation → String
  | .notObserved => "not_observed"
  | .present => "present"
  | .absent => "absent"

private def optionStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

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

private def statesJson (states : List Env) : Lean.Json :=
  .arr (states.toArray.map fun environment =>
    .arr ((factStrings environment).toArray.map Lean.toJson))

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
    ("observation_kind", .str (observationName item.observationKind)),
    ("family", .str item.family),
    ("source", .str item.source),
    ("marker", .str item.marker),
    ("expected_exits", exitsJson item.expectedExits),
    ("expected_facts", .arr (item.expectedFacts.toArray.map Lean.toJson)),
    ("candidate", .str (candidateName item.candidate)),
    ("candidate_count", Lean.toJson item.candidateCount),
    ("candidate_path", optionStringJson item.candidatePath),
    ("candidate_operator", optionStringJson item.candidateOperator),
    ("candidate_original", optionStringJson item.candidateOriginal),
    ("candidate_replacement", optionStringJson item.candidateReplacement),
    ("candidate_symbol", optionStringJson item.candidateSymbol)
  ]

def renderCorpus : String :=
  String.join (nestedMatchExitCases.map fun item =>
    (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "nested match exit audit did not distinguish every broken variant"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "nested match exit fixed cases violate the corpus contract"
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
        IO.eprintln s!"stale Lean nested match exit corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean nested match exit corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"nested_category_detected={nestedCategorySensitivity}"
  IO.println s!"unreachable_arm_detected={retainUnreachableSensitivity}"
  IO.println s!"omitted_abrupt_detected={omitAbruptSensitivity}"
  IO.println s!"cleanup_boundary_detected={cleanupBoundarySensitivity}"
  IO.println s!"continue_back_edge_detected={continueBackEdgeSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in nestedMatchExitCases do
    IO.println s!"{item.id}={item.schema == 1 && validMode item.mode}"
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
      IO.println s!"fixed_cases={nestedMatchExitCases.length}"
      IO.println s!"strict_cases={(nestedMatchExitCases.filter fun item => item.mode == "strict").length}"
      IO.println s!"internal_fixture_cases={(nestedMatchExitCases.filter fun item => item.mode == "internal-fixture").length}"
      IO.println s!"model_only_cases={(nestedMatchExitCases.filter fun item => item.mode == "model-only").length}"
      IO.println "sensitivity_families=5"
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
      IO.eprintln "usage: generate_nested_match_exits --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.NestedMatchExit.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.NestedMatchExit.Executable.main args
