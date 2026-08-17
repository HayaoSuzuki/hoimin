import HoiminOracle.ProcessOutputProofs

namespace HoiminOracle.ProcessOutput

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String := "strict"
  execution : ExecutionKind
  process : ProcessOutcome
  output : OutputOutcome
  expected : Decision
  deriving Repr, DecidableEq

def executionName : ExecutionKind → String
  | .baseline => "baseline"
  | .mutant => "mutant"

def terminationName : Termination → String
  | .exitSuccess => "exit_success"
  | .exitFailure => "exit_failure"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .cancelled => "cancelled"

def processName : ProcessOutcome → String
  | .known termination => s!"known_{terminationName termination}"
  | .failed => "failed"

def outputName : OutputOutcome → String
  | .complete => "complete"
  | .closeTimedOut => "close_timed_out"
  | .failed => "failed"

def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .notRun => "not_run"
  | .error => "error"

def cases : List OracleCase := assignments.map fun entry =>
  let execution := entry.1
  let process := entry.2.1
  let output := entry.2.2
  { id := s!"{executionName execution}_{processName process}_{outputName output}"
    execution := execution
    process := process
    output := output
    expected := decideOutcome execution process output }

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 && item.mode == "strict" &&
    item.expected == decideOutcome item.execution item.process item.output &&
    safeDecision item.execution item.process item.output

end HoiminOracle.ProcessOutput
