import HoiminOracle.TimeoutLimitCases
import Lean.Data.Json

open HoiminOracle.TimeoutLimit

namespace HoiminOracle.TimeoutLimit.Executable

private def natString (value : Nat) : Lean.Json := .str (toString value)

private def mutantModeName : MutantTimeout → String
  | .auto => "auto"
  | .fixed _ => "fixed"

private def fixedMutantJson : MutantTimeout → Lean.Json
  | .auto => .null
  | .fixed nanoseconds => natString nanoseconds

private def optionalString : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("analyzer_ns", natString item.input.analyzer),
    ("baseline_ns", natString item.input.baseline),
    ("mutant_mode", .str (mutantModeName item.input.mutant)),
    ("fixed_mutant_ns", fixedMutantJson item.input.mutant),
    ("total_ns", natString item.input.total),
    ("maximum_ns", natString item.input.maximum),
    ("expected_accepted", Lean.toJson item.expected.accepted),
    ("expected_invalid_field", optionalString item.expected.invalidField),
    ("expected_effective_mutant_ns", natString item.expected.effectiveMutantTimeout)
  ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "timeout-limit audit did not distinguish every broken boundary"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "timeout-limit fixed cases violate the modeled contract"
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
        IO.eprintln s!"stale Lean timeout-limit corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean timeout-limit corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println s!"fixed_cases={cases.length}"
      IO.println s!"strict_cases={(cases.filter fun item => item.mode == "strict").length}"
      IO.println s!"maximum_ns={maximum}"
      IO.println s!"sensitivity_zero={zeroSensitivity}"
      IO.println s!"sensitivity_inclusive={inclusiveSensitivity}"
      IO.println s!"sensitivity_derived_auto={derivedAutoSensitivity}"
      IO.println s!"sensitivity_attribution={attributionSensitivity}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"zero_detected={zeroSensitivity}"
  IO.println s!"inclusive_boundary_detected={inclusiveSensitivity}"
  IO.println s!"derived_auto_detected={derivedAutoSensitivity}"
  IO.println s!"attribution_detected={attributionSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if fixedCasesPass then 0 else 2

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
      IO.eprintln "usage: generate_timeout_limit --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.TimeoutLimit.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.TimeoutLimit.Executable.main args
