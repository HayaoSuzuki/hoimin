import HoiminOracle.BindingFlowCases
import Lean.Data.Json

open HoiminOracle.BindingFlow

namespace HoiminOracle.BindingFlow.Executable

private def optionStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def caseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("family", .str item.family),
  ("operator", .str item.operator),
  ("source", .str item.source),
  ("site_marker", .str item.siteMarker),
  ("expected_present", Lean.toJson (caseExpectedPresent item)),
  ("expected_replacement", optionStringJson (caseExpectedReplacement item)),
  ("expected_symbol", optionStringJson (caseExpectedSymbol item))
]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit (depth : Nat) : IO (Except UInt32 Unit) := do
  unless fixedCasesPass do
    IO.eprintln "binding-flow fixed cases violate the modeled contract"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "binding-flow audit did not distinguish every broken family"
    return .error 2
  unless stateCount depth <= 1024 do
    IO.eprintln s!"binding-flow state ceiling exceeded: {stateCount depth}"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit 2 with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      IO.FS.writeFile path renderCorpus
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit 2 with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then return 0
        IO.eprintln s!"stale Lean binding-flow corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean binding-flow corpus {path}: {error}"
        return 1

private def printStats (depth : Nat) : IO UInt32 := do
  match ← ensureAudit depth with
  | .error code => return code
  | .ok () =>
      IO.println s!"depth={depth}"
      IO.println s!"programs={(reachableAtDepth depth).length}"
      IO.println s!"states={stateCount depth}"
      IO.println s!"transitions={transitionCount depth}"
      IO.println "state_ceiling=1024"
      IO.println s!"fixed_cases={cases.length}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"union_meet_detected={unionSensitivity}"
  IO.println s!"class_resolution_detected={classSensitivity}"
  IO.println s!"finally_routing_detected={finallySensitivity}"
  IO.println s!"loop_iteration_detected={loopSensitivity}"
  IO.println s!"destination_gate_detected={destinationSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do
    IO.println s!"{item.id}={caseSafe item},present={caseExpectedPresent item}"
  return if fixedCasesPass then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats", depth] =>
      match depth.toNat? with
      | some value => printStats value
      | none =>
          IO.eprintln s!"invalid depth: {depth}"
          return 2
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | _ =>
      IO.eprintln "usage: generate_binding_flow --output PATH | --check PATH | --stats DEPTH | --sensitivity | --cases"
      return 2

end HoiminOracle.BindingFlow.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.BindingFlow.Executable.main args
