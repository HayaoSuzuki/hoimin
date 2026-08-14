import HoiminOracle.BoundedCandidateDiscoveryCases
import Lean.Data.Json

open HoiminOracle.BoundedCandidateDiscovery

namespace HoiminOracle.BoundedCandidateDiscovery.Executable

private def producerName : Producer → String
  | .token => "token"
  | .ast => "ast"
  | .annotation => "annotation"

private def candidateJson (item : Candidate) : Lean.Json := Lean.Json.mkObj [
  ("identity", Lean.toJson item.identity),
  ("order_key", Lean.toJson item.orderKey),
  ("producer", .str (producerName item.producer)),
  ("eligible", Lean.toJson item.eligible),
  ("emission_index", Lean.toJson item.emissionIndex),
  ("path", .str item.path),
  ("span_start", Lean.toJson item.spanStart),
  ("span_length", Lean.toJson item.spanLength),
  ("original", .str item.original),
  ("replacement", .str item.replacement),
  ("operator", .str item.operatorKey),
  ("line", Lean.toJson item.line),
  ("column", Lean.toJson item.column)
]

private def candidatesJson (items : List Candidate) : Lean.Json :=
  .arr (items.toArray.map candidateJson)

private def naturalsJson (items : List Nat) : Lean.Json :=
  .arr (items.toArray.map Lean.toJson)

private def caseJson (item : OracleCase) : Lean.Json :=
  let expected := observe item
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("limit", Lean.toJson item.limit),
    ("token", candidatesJson item.token),
    ("ast", candidatesJson item.ast),
    ("annotation", candidatesJson item.annotation),
    ("targets", .arr (item.targets.toArray.map candidatesJson)),
    ("expected_identities", naturalsJson expected.identities),
    ("expected_truncated", Lean.toJson expected.truncated),
    ("expected_sequences", naturalsJson expected.sequences),
    ("expected_targets_read", Lean.toJson expected.targetsRead),
    ("expected_spool_finished", Lean.toJson expected.spoolFinished),
    ("expected_candidate_limit_diagnostic", Lean.toJson expected.candidateLimitDiagnostic),
    ("expected_exit_code", Lean.toJson expected.exitCode)
  ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "bounded candidate discovery audit missed a broken family"
    return .error 2
  unless cases.all caseSafe do
    IO.eprintln "bounded candidate discovery cases violate the closed contract"
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
        IO.eprintln s!"stale Lean bounded discovery corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean bounded discovery corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"lookahead_detected={lookaheadSensitivity}"
  IO.println s!"ordering_detected={orderingSensitivity}"
  IO.println s!"eligibility_detected={eligibilitySensitivity}"
  IO.println s!"duplicate_detected={duplicateSensitivity}"
  IO.println s!"producer_overflow_detected={producerOverflowSensitivity}"
  IO.println s!"merge_order_detected={mergeOrderSensitivity}"
  IO.println s!"global_capacity_detected={globalCapacitySensitivity}"
  IO.println s!"terminal_spool_detected={terminalSensitivity}"
  IO.println s!"public_projection_detected={publicProjectionSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if cases.all caseSafe then 0 else 2

private def printStats : IO UInt32 := do
  IO.println s!"fixed_cases={cases.length}"
  IO.println s!"strict_cases={(cases.filter fun item => item.mode == "strict").length}"
  IO.println s!"internal_cases={(cases.filter fun item => item.mode == "internal-fixture").length}"
  IO.println s!"model_only_cases={(cases.filter fun item => item.mode == "model-only").length}"
  return 0

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | ["--stats"] => printStats
  | _ =>
      IO.eprintln "usage: generate_bounded_candidate_discovery --output PATH | --check PATH | --sensitivity | --cases | --stats"
      return 2

end HoiminOracle.BoundedCandidateDiscovery.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.BoundedCandidateDiscovery.Executable.main args
