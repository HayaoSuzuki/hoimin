import HoiminOracle.TopBudgetProjectionCases
import Lean.Data.Json

open HoiminOracle.TopBudgetProjection

namespace HoiminOracle.TopBudgetProjection.Executable

private def natString (value : Nat) : Lean.Json := .str (toString value)

private def timeoutName : TimeoutMode → String
  | .auto => "auto"
  | .fixed _ => "fixed"

private def fixedTimeoutJson : TimeoutMode → Lean.Json
  | .auto => .null
  | .fixed ticks => natString ticks

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("selected", natString item.input.selected),
    ("jobs", natString item.input.jobs),
    ("planned_total_timeout_ns", natString item.input.plannedTotalTimeout),
    ("baseline_ns", natString item.input.baseline),
    ("timeout_mode", .str (timeoutName item.input.timeoutMode)),
    ("fixed_timeout_ns", fixedTimeoutJson item.input.timeoutMode),
    ("remaining_ns", natString item.input.remaining),
    ("duration_max_ns", natString item.input.durationMax),
    ("expected_effective_timeout_ns", natString item.expected.effectiveMutantTimeout),
    ("expected_waves", natString item.expected.waves),
    ("expected_capacity_ns", natString item.expected.projectedCapacity),
    ("expected_shortfall", Lean.toJson item.expected.shortfall)
  ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "top-budget projection audit did not distinguish every broken boundary"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "top-budget projection fixed cases violate the modeled contract"
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
        IO.eprintln s!"stale Lean top-budget projection corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean top-budget projection corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println s!"fixed_cases={cases.length}"
      IO.println s!"strict_cases={(cases.filter fun item => item.mode == "strict").length}"
      IO.println s!"duration_max_ticks={durationMax}"
      IO.println s!"sensitivity_floor={floorSensitivity}"
      IO.println s!"sensitivity_equality={equalitySensitivity}"
      IO.println s!"sensitivity_u32_capacity={u32CapacitySensitivity}"
      IO.println s!"sensitivity_auto={autoSensitivity}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"floor_detected={floorSensitivity}"
  IO.println s!"inclusive_boundary_detected={equalitySensitivity}"
  IO.println s!"u32_capacity_detected={u32CapacitySensitivity}"
  IO.println s!"auto_timeout_detected={autoSensitivity}"
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
      IO.eprintln "usage: generate_top_budget_projection --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.TopBudgetProjection.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.TopBudgetProjection.Executable.main args
