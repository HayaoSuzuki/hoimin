import HoiminOracle.ProcessOutputCases
import Lean.Data.Json

open HoiminOracle.ProcessOutput

namespace HoiminOracle.ProcessOutput.Executable

private def optionJson (value : Option String) : Lean.Json :=
  match value with
  | some value => .str value
  | none => .null

private def processTermination? : ProcessOutcome → Option String
  | .known termination => some (terminationName termination)
  | .failed => none

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("execution", .str (executionName item.execution)),
    ("process", .str (processName item.process)),
    ("output", .str (outputName item.output)),
    ("expected_fatal", Lean.toJson item.expected.fatal),
    ("expected_failure_source", optionJson (item.expected.failure.map failureSourceName)),
    ("expected_status", optionJson (item.expected.status.map statusName)),
    ("expected_termination", optionJson (item.expected.termination.map terminationName)),
    ("expected_output_incomplete", Lean.toJson item.expected.outputIncomplete),
    ("expected_diagnostic", Lean.toJson item.expected.diagnostic),
    ("expected_continue", Lean.toJson item.expected.continueRun),
    ("input_termination", optionJson (processTermination? item.process)) ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def drainSourceName : DrainSource → String
  | .stdout => "stdout"
  | .stderr => "stderr"
  | .collector => "collector"

private def drainJson (stdout stderr collector : OutputOutcome) : Lean.Json :=
  let failure := selectDrainError stdout stderr collector
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)),
    ("stdout", .str (outputName stdout)), ("stderr", .str (outputName stderr)),
    ("collector", .str (outputName collector)),
    ("expected_source", optionJson (failure.map fun item => drainSourceName item.1)),
    ("expected_error", optionJson (failure.map fun item => outputName item.2)) ]

def renderDrainCorpus : String :=
  String.join (drainAssignments.map fun (stdout, stderr, collector) =>
    (drainJson stdout stderr collector).compress ++ "\n")

private def resumeJson (item : ResumeCase) : Lean.Json :=
  let status (value : Option HoiminOracle.SessionAudit.Status) :=
    optionJson (value.map HoiminOracle.SessionAudit.statusName)
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str item.id),
    ("termination", .str (terminationName item.termination)),
    ("close_timed_out", Lean.toJson item.closeTimedOut),
    ("expected_stored", status item.expected.stored),
    ("expected_reused", status item.expected.reused),
    ("expected_replacement_error", optionJson item.expected.replacementError),
    ("expected_stored_after_replacement", status item.expected.storedAfterReplacement) ]

def renderResumeCorpus : String :=
  String.join (resumeCases.map fun item => (resumeJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless allSafe do
    IO.eprintln "process output decision table is unsafe"
    return .error 2
  unless sensitivityPasses && drainErrorSensitivity && resumeSensitivity do
    IO.eprintln "process output audit missed a broken family"
    return .error 2
  unless cases.length == 63 && cases.all caseSafe do
    IO.eprintln "process output cases violate the closed contract"
    return .error 2
  unless drainAssignments.length == 27 do
    IO.eprintln "process output drain cases do not cover the closed table"
    return .error 2
  unless resumeCases.length == 12 do
    IO.eprintln "process output resume cases do not cover the closed table"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) (content : String) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      IO.FS.writeFile path content
      return 0

private def checkCorpus (path : System.FilePath) (content : String) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == content then return 0
        IO.eprintln s!"stale process output corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check process output corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"broken_families_detected={sensitivityPasses}"
  IO.println s!"drain_error_detected={drainErrorSensitivity}"
  IO.println s!"resume_error_detected={resumeSensitivity}"
  return if sensitivityPasses && drainErrorSensitivity && resumeSensitivity then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if cases.length == 63 && cases.all caseSafe then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path renderCorpus
  | ["--check", path] => checkCorpus path renderCorpus
  | ["--output-drain", path] => writeCorpus path renderDrainCorpus
  | ["--check-drain", path] => checkCorpus path renderDrainCorpus
  | ["--output-resume", path] => writeCorpus path renderResumeCorpus
  | ["--check-resume", path] => checkCorpus path renderResumeCorpus
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | _ =>
      IO.eprintln "usage: generate_process_output (--output | --check | --output-drain | --check-drain | --output-resume | --check-resume) PATH | --sensitivity | --cases"
      return 2

end HoiminOracle.ProcessOutput.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ProcessOutput.Executable.main args
