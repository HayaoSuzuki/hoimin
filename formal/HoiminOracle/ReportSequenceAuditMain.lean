import HoiminOracle.ReportSequenceCases
import Lean.Data.Json

open HoiminOracle.ReportSequence

namespace HoiminOracle.ReportSequence.Executable

private def statusName := statusString
private def terminationName := terminationString

private def fieldsJson (fields : List (String × String)) : Lean.Json :=
  Lean.Json.mkObj (fields.map fun entry => (entry.1, .str entry.2))

private def expectedJson (expected : Expected) : Lean.Json :=
  Lean.Json.mkObj [
    ("accepted", Lean.toJson expected.accepted),
    ("error_code", expected.errorCode.map Lean.Json.str |>.getD .null),
    ("error_fields", fieldsJson expected.errorFields)
  ]

private def eventJson (event : Event) : Lean.Json :=
  let common := [
    ("run_id", Lean.Json.str (runIdString event.runId)),
    ("sequence", Lean.toJson event.sequence)
  ]
  match event.kind with
  | .runStarted => Lean.Json.mkObj (("kind", .str "run_started") :: common)
  | .diagnostic => Lean.Json.mkObj (("kind", .str "diagnostic") :: common)
  | .runFinished => Lean.Json.mkObj (("kind", .str "run_finished") :: common)
  | .mutantStarted id mutantSequence =>
      Lean.Json.mkObj (("kind", .str "mutant_started") :: common ++ [
        ("mutant_id", .str (mutantIdString id)),
        ("mutant_sequence", Lean.toJson mutantSequence)
      ])
  | .mutantFinished id mutantSequence status termination =>
      Lean.Json.mkObj (("kind", .str "mutant_finished") :: common ++ [
        ("mutant_id", .str (mutantIdString id)),
        ("mutant_sequence", Lean.toJson mutantSequence),
        ("status", .str (statusName status)),
        ("termination", termination.map (Lean.Json.str ∘ terminationName) |>.getD .null)
      ])

private def caseJson (item : Case) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson 1),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("premise", .str item.premise),
    ("prefix", .arr (item.prefixEvents.toArray.map eventJson)),
    ("target", eventJson item.target),
    ("expected", expectedJson item.expected),
    ("probe", item.probe.map eventJson |>.getD .null),
    ("probe_expected", item.probeExpected.map expectedJson |>.getD .null)
  ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def runIds : List RunId := [.first, .second]
private def sequences : List Nat := [0, 1, 2]
private def mutantIds : List MutantId := [.alpha, .beta]
private def mutantSequences : List Nat := [0, 1]

private def simpleEvents : List Event :=
  runIds.flatMap fun runId =>
    sequences.flatMap fun sequence => [
      { runId, sequence, kind := .runStarted },
      { runId, sequence, kind := .diagnostic },
      { runId, sequence, kind := .runFinished }
    ]

private def mutantEvents : List Event :=
  sequences.flatMap fun sequence =>
    mutantIds.flatMap fun id =>
      mutantSequences.flatMap fun mutantSequence => [
        { runId := .first, sequence, kind := .mutantStarted id mutantSequence },
        { runId := .first, sequence
          kind := .mutantFinished id mutantSequence .survived (some .exitZero) }
      ]

private def eventAlphabet : List Event := simpleEvents ++ mutantEvents
private def searchDepth : Nat := 4

private def expandStates (states : List State) : List State :=
  (states.flatMap fun state =>
    eventAlphabet.map fun event => (step state event).state).eraseDups

private def levels : Nat → List State
  | 0 => [initial]
  | depth + 1 => expandStates (levels depth)

private def retainedStateCount : Nat := (levels searchDepth).length
private def transitionCount : Nat :=
  ((List.range (searchDepth + 1)).map fun depth =>
    (levels depth).length * eventAlphabet.length).sum

private def generatedTraceCount : Nat :=
  (List.range (searchDepth + 1)).map (eventAlphabet.length ^ ·) |>.sum

private def modesValid : Bool :=
  cases.all fun item => item.mode == "model-only" || item.mode == "strict"

private def probesPaired : Bool :=
  cases.all fun item => item.probe.isSome == item.probeExpected.isSome

private def uniqueIds : Bool :=
  (cases.map Case.id).eraseDups.length == cases.length

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "report-sequence audit did not distinguish every broken variant"
    return .error 2
  unless modesValid && probesPaired && uniqueIds do
    IO.eprintln "report-sequence cases violate the closed corpus contract"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      let temporary := path.withExtension "jsonl.tmp"
      IO.FS.writeFile temporary renderCorpus
      IO.FS.rename temporary path
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then return 0
        IO.eprintln s!"stale Lean report-sequence corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean report-sequence corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"atomicity_detected={atomicityWitness}"
  IO.println s!"identity_reuse_detected={uniquenessWitness}"
  IO.println s!"equal_sequence_detected={equalityWitness}"
  IO.println s!"precedence_detected={precedenceWitness}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do
    IO.println s!"{item.id}: mode={item.mode} accepted={item.expected.accepted} probe={item.probe.isSome}"
  return 0

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println s!"depth={searchDepth}"
      IO.println s!"event_alphabet={eventAlphabet.length}"
      IO.println s!"generated_traces={generatedTraceCount}"
      IO.println s!"retained_states={retainedStateCount}"
      IO.println s!"transitions={transitionCount}"
      IO.println s!"fixed_cases={cases.length}"
      IO.println s!"strict={(cases.filter fun item => item.mode == "strict").length}"
      IO.println s!"model_only={(cases.filter fun item => item.mode == "model-only").length}"
      IO.println s!"sensitivity={sensitivityPasses}"
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
      IO.eprintln "usage: generate_report_sequence --output PATH | --check PATH | --sensitivity | --cases | --stats"
      return 2

end HoiminOracle.ReportSequence.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ReportSequence.Executable.main args
