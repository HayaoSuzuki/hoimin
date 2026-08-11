import HoiminOracle.AnnotationScopeCases
import Lean.Data.Json

open HoiminOracle.AnnotationScope

namespace HoiminOracle.AnnotationScope.Executable

private def optionStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.map Lean.Json.str).toArray

private def caseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("scenario", .str item.scenario.label),
  ("observation_kind", .str item.observationKind.label),
  ("source", .str item.source),
  ("marker", .str item.marker),
  ("expected_facts", stringsJson item.expectedFacts),
  ("expected_symbol", optionStringJson item.expectedSymbol),
  ("expected_scope", optionStringJson item.expectedScope),
  ("expected_resolution", optionStringJson item.expectedResolution),
  ("expected_operator", optionStringJson item.expectedOperator),
  ("expected_original", optionStringJson item.expectedOriginal),
  ("expected_replacement", optionStringJson item.expectedReplacement),
  ("expected_present", Lean.toJson item.expectedPresent)
]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless fixedCasesPass do
    IO.eprintln "annotation-scope fixed cases violate the modeled contract"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "annotation-scope audit did not distinguish every broken family"
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
        IO.eprintln s!"stale Lean annotation-scope corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean annotation-scope corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println "structured_depth=2"
      IO.println "generated_depth_expansion=false"
      IO.println s!"fixed_cases={cases.length}"
      IO.println "sensitivity_families=5"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"comprehension_leak_detected={comprehensionLeakSensitivity}"
  IO.println s!"first_iterable_order_detected={firstIterableSensitivity}"
  IO.println s!"global_target_detected={globalTargetSensitivity}"
  IO.println s!"nonlocal_target_detected={nonlocalTargetSensitivity}"
  IO.println s!"site_entry_detected={siteEntrySensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do
    IO.println s!"{item.id}={CaseValid item},present={item.expectedPresent}"
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
  | _ =>
      IO.eprintln "usage: generate_annotation_scope --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.AnnotationScope.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.AnnotationScope.Executable.main args
