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
    ("expected_status", optionJson (item.expected.status.map statusName)),
    ("expected_termination", optionJson (item.expected.termination.map terminationName)),
    ("expected_output_incomplete", Lean.toJson item.expected.outputIncomplete),
    ("expected_diagnostic", Lean.toJson item.expected.diagnostic),
    ("expected_continue", Lean.toJson item.expected.continueRun),
    ("input_termination", optionJson (processTermination? item.process)) ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless allSafe do
    IO.eprintln "process output decision table is unsafe"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "process output audit missed a broken family"
    return .error 2
  unless cases.length == 42 && cases.all caseSafe do
    IO.eprintln "process output cases violate the closed contract"
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
        IO.eprintln s!"stale process output corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check process output corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"broken_families_detected={sensitivityPasses}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if cases.length == 42 && cases.all caseSafe then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | _ =>
      IO.eprintln "usage: generate_process_output --output PATH | --check PATH | --sensitivity | --cases"
      return 2

end HoiminOracle.ProcessOutput.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ProcessOutput.Executable.main args
