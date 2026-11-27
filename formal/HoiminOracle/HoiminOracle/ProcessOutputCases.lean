import HoiminOracle.ProcessOutputProofs
import HoiminOracle.SessionCases

namespace HoiminOracle.ProcessOutput

structure OracleCase where
  schema : Nat := 2
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
  | .probe => "probe"

def failureSourceName : FailureSource → String
  | .process => "process"
  | .output => "output"

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
  item.schema == 2 && item.mode == "strict" &&
    item.expected == decideOutcome item.execution item.process item.output &&
    safeDecision item.execution item.process item.output

private def sessionStatus : Status → SessionAudit.Status
  | .killed => .killed
  | .survived => .survived
  | .timeout => .timeout
  | .outOfMemory => .outOfMemory
  | .processLimit => .processLimit
  | .notRun => .notRun
  | .error => .error

structure ResumeObservation where
  stored : Option SessionAudit.Status
  reused : Option SessionAudit.Status
  replacementError : Option String
  storedAfterReplacement : Option SessionAudit.Status
  deriving DecidableEq, BEq

private def observeResume (status : Status) : ResumeObservation :=
  let resumed := SessionAudit.run SessionAudit.State.initial [
    .open .h0, .begin .h0 .r0 .f0, .persist .h0 .r0 .m0 (sessionStatus status) .p0 .valid,
    .finish .h0 .r0 false, .drop .h0, .open .h1, .load .h1 .f0 ]
  let lookup : SessionAudit.Event := .lookup .h1 .r0 .m0
  let stored := (SessionAudit.step resumed lookup).storedResult.map SessionAudit.ResultRow.status
  let replacement : SessionAudit.Event := .persist .h1 .r0 .m0 .survived .p1 .valid
  let attempted := SessionAudit.step resumed replacement
  { stored
    reused := stored.filter SessionAudit.Status.isDeterminate
    replacementError := attempted.rejection.map (SessionAudit.rejectionCode replacement)
    storedAfterReplacement :=
      (SessionAudit.step attempted.state lookup).storedResult.map SessionAudit.ResultRow.status }

structure ResumeCase where
  id : String
  termination : Termination
  closeTimedOut : Bool
  expected : ResumeObservation
  deriving DecidableEq, BEq

def resumeCases : List ResumeCase := cases.filterMap fun item => do
  let termination ← item.expected.termination
  let status ← item.expected.status
  return {
    id := item.id
    termination
    closeTimedOut := item.output == .closeTimedOut
    expected := observeResume status
  }

def resumeSensitivity : Bool :=
  observeResume .error != observeResume (classify .exitSuccess) &&
    observeResume .notRun != observeResume .survived

theorem timeout_result_reruns_after_reopen (termination : Termination) :
    ((decideOutcome .mutant (.known termination) .closeTimedOut).status.map observeResume) =
      some {
        stored := some .error
        reused := none
        replacementError := none
        storedAfterReplacement := some .survived
      } := by
  rfl

theorem resume_sensitivity_passes : resumeSensitivity = true := by native_decide

end HoiminOracle.ProcessOutput
