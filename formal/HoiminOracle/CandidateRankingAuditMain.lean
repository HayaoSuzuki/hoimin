import HoiminOracle.CandidateRankingCases
import Lean.Data.Json

open HoiminOracle.CandidateRanking

namespace HoiminOracle.CandidateRanking.Executable

private def pathName : Path → String
  | .alpha => "alpha.py"
  | .beta => "beta.py"

private def operatorClassName : OperatorClass → String
  | .highValueControl => "high_value_control"
  | .exceptionHandling => "exception_handling"
  | .behavioral => "behavioral"
  | .arithmetic => "arithmetic"
  | .typeAnnotation => "type_annotation"

private def reasonName : Reason → String
  | .explicitLine => "explicit_line"
  | .explicitSymbol => "explicit_symbol"
  | .changedLine => "changed_line"
  | .operator kind => operatorClassName kind

private def stringsJson (items : List String) : Lean.Json :=
  .arr (items.toArray.map Lean.Json.str)

private def candidateJson (item : Candidate) : Lean.Json := Lean.Json.mkObj [
  ("id", .str item.id),
  ("path", .str (pathName item.path)),
  ("line", Lean.toJson item.line),
  ("column", Lean.toJson item.column),
  ("operator", .str item.operatorKey),
  ("operator_class", .str (operatorClassName item.operatorClass)),
  ("explicit_line", Lean.toJson item.explicitLine),
  ("explicit_symbol", Lean.toJson item.explicitSymbol),
  ("changed_line", Lean.toJson item.changedLine)
]

private def rankedJson (item : RankedCandidate) : Lean.Json := Lean.Json.mkObj [
  ("id", .str item.candidate.id),
  ("rank", Lean.toJson item.rank),
  ("score", Lean.toJson item.score),
  ("reasons", stringsJson (item.rankingReasons.map reasonName))
]

private def caseJson (item : OracleCase) : Lean.Json :=
  let ranked := rankCandidates item.candidates
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("limit", Lean.toJson item.limit),
    ("candidates", .arr (item.candidates.toArray.map candidateJson)),
    ("expected_ranking", .arr (ranked.toArray.map rankedJson)),
    ("expected_strict", stringsJson (strictSelect ranked item.limit)),
    ("expected_diverse", stringsJson (diverseSelect ranked item.limit))
  ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "candidate ranking audit did not distinguish every broken family"
    return .error 2
  unless cases.all caseSafe do
    IO.eprintln "candidate ranking fixed cases violate the modeled contract"
    return .error 2
  unless finiteAuditPasses do
    IO.eprintln "candidate ranking finite manifest audit found a counterexample"
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
        IO.eprintln s!"stale Lean candidate ranking corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean candidate ranking corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println s!"candidate_universe={diverseCandidates.length}"
      IO.println s!"finite_manifests={finiteManifests.length}"
      IO.println s!"fixed_cases={cases.length}"
      IO.println s!"sensitivity_validation={validationSensitivity}"
      IO.println s!"sensitivity_uniqueness={duplicateSensitivity}"
      IO.println s!"sensitivity_tier_boundary={tierSensitivity}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"validation_detected={validationSensitivity}"
  IO.println s!"uniqueness_detected={duplicateSensitivity}"
  IO.println s!"tier_boundary_detected={tierSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do
    IO.println s!"{item.id}={caseSafe item}"
  return if cases.all caseSafe then 0 else 2

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
      IO.eprintln "usage: generate_candidate_ranking --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.CandidateRanking.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.CandidateRanking.Executable.main args
