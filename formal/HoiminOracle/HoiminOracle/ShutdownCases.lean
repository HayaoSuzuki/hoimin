import HoiminOracle.ShutdownProofs
import Lean.Data.Json

namespace HoiminOracle.ShutdownAudit

inductive Mode | strict | internalFixture | modelOnly
  deriving Repr, DecidableEq, BEq

inductive Scenario
  | normalCompletion | firstInterruptRunning | totalTimeoutReapsDescendant
  | secondInterruptBlockedFinish | totalTimeoutBlockedFinish
  | processExitVsCancellation | cleanupFailureAfterProcessFailure
  | outputEofVsExpiry | cleanupCompletionVsExpiry
  | sessionFinishVsLateCancellation | reportWriteVsLateError
  | blockingCompletionVsDetach
  deriving Repr, DecidableEq, BEq

structure TerminalObservation where
  cause : Option StopCause
  exitCode : Option Nat
  process : ProcessState
  output : ComponentState
  blocking : ComponentState
  workspace : ComponentState
  session : ComponentState
  report : ComponentState
  metrics : ComponentState
  primaryError : Option StopCause
  appendedErrors : List StopCause
  dispatches : Nat × Nat × Nat × Nat
  sessionCompleteFlag : Bool
  reportCompleteFlag : Bool
  returned : Bool
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : Mode
  scenario : Scenario
  schedule : List Event
  expected : TerminalObservation
  deriving Repr, DecidableEq

def terminalObservation (state : State) : TerminalObservation where
  cause := state.cause
  exitCode := state.exitCode
  process := state.process
  output := state.output
  blocking := state.blocking
  workspace := state.workspace
  session := state.session
  report := state.report
  metrics := state.metrics
  primaryError := state.primaryError
  appendedErrors := state.appendedErrors
  dispatches := (state.cleanupDispatches, state.sessionDispatches,
    state.reportDispatches, state.metricsDispatches)
  sessionCompleteFlag := state.sessionCompleteFlag
  reportCompleteFlag := state.reportCompleteFlag
  returned := state.returned

private structure CaseSpec where
  id : String
  mode : Mode
  scenario : Scenario
  schedule : List Event

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  mode := spec.mode
  scenario := spec.scenario
  schedule := spec.schedule
  expected := terminalObservation (run State.initial spec.schedule)

private def specs : List CaseSpec := [
  { id := "normal_completion", mode := .strict, scenario := .normalCompletion
    schedule := [.boot, .startProcess, .processExited, .startOutputDrain,
      .outputDrained, .reapProcess, .startCleanup, .cleanupCompleted,
      .startSessionFinish, .sessionFinished, .startReport, .reportWritten,
      .startMetrics, .metricsWritten, .returnSuccess] },
  { id := "first_interrupt_running", mode := .strict,
    scenario := .firstInterruptRunning
    schedule := [.boot, .startProcess, .firstInterrupt, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .startSessionFinish, .sessionFinished,
      .startReport, .reportWritten, .startMetrics, .metricsSkipped,
      .returnFailure] },
  { id := "total_timeout_reaps_descendant", mode := .strict,
    scenario := .totalTimeoutReapsDescendant
    schedule := [.boot, .startProcess, .deadlineReached, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .startSessionFinish, .sessionFinished,
      .startReport, .reportWritten, .startMetrics, .metricsSkipped,
      .returnFailure] },
  { id := "second_interrupt_blocked_finish", mode := .internalFixture,
    scenario := .secondInterruptBlockedFinish
    schedule := [.boot, .startProcess, .firstInterrupt, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .startSessionFinish, .startBlocking,
      .secondInterrupt] },
  { id := "total_timeout_blocked_finish", mode := .internalFixture,
    scenario := .totalTimeoutBlockedFinish
    schedule := [.boot, .startProcess, .deadlineReached, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .startSessionFinish, .startBlocking,
      .detachBlocking, .returnFailure] },
  { id := "process_exit_vs_cancellation", mode := .internalFixture,
    scenario := .processExitVsCancellation
    schedule := [.boot, .startProcess, .processExited, .firstInterrupt,
      .startOutputDrain, .outputDrained, .reapProcess, .startCleanup,
      .cleanupCompleted, .startSessionFinish, .sessionFinished, .startReport,
      .reportWritten, .returnFailure] },
  { id := "cleanup_failure_after_process_failure", mode := .internalFixture,
    scenario := .cleanupFailureAfterProcessFailure
    schedule := [.boot, .startProcess, .processFailed, .startOutputDrain,
      .outputFailed, .reapProcess, .startCleanup, .cleanupFailed,
      .startSessionFinish, .sessionFailed, .startReport, .reportFailed,
      .returnFailure] },
  { id := "output_eof_vs_expiry", mode := .internalFixture,
    scenario := .outputEofVsExpiry
    schedule := [.boot, .startProcess, .firstInterrupt, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .returnFailure] },
  { id := "cleanup_completion_vs_expiry", mode := .internalFixture,
    scenario := .cleanupCompletionVsExpiry
    schedule := [.boot, .startProcess, .firstInterrupt, .requestTermination,
      .processExited, .startOutputDrain, .outputDrained, .reapProcess,
      .startCleanup, .cleanupCompleted, .deadlineReached, .startSessionFinish,
      .sessionFinished, .returnFailure] },
  { id := "session_finish_vs_late_cancellation", mode := .modelOnly,
    scenario := .sessionFinishVsLateCancellation
    schedule := [.boot, .startCleanup, .cleanupCompleted, .startSessionFinish,
      .sessionFinished, .firstInterrupt, .startReport, .reportWritten,
      .returnFailure] },
  { id := "report_write_vs_late_error", mode := .modelOnly,
    scenario := .reportWriteVsLateError
    schedule := [.boot, .startCleanup, .cleanupCompleted, .startSessionFinish,
      .sessionFinished, .startReport, .reportWritten, .infrastructureFailed,
      .returnFailure] },
  { id := "blocking_completion_vs_detach", mode := .modelOnly,
    scenario := .blockingCompletionVsDetach
    schedule := [.boot, .startBlocking, .firstInterrupt, .blockingCompleted,
      .detachBlocking, .returnFailure] }
]

def shutdownCases : List OracleCase := specs.map toOracleCase

def modeName : Mode → String
  | .strict => "strict"
  | .internalFixture => "internal-fixture"
  | .modelOnly => "model-only"

def scenarioName : Scenario → String
  | .normalCompletion => "normal_completion"
  | .firstInterruptRunning => "first_interrupt_running"
  | .totalTimeoutReapsDescendant => "total_timeout_reaps_descendant"
  | .secondInterruptBlockedFinish => "second_interrupt_blocked_finish"
  | .totalTimeoutBlockedFinish => "total_timeout_blocked_finish"
  | .processExitVsCancellation => "process_exit_vs_cancellation"
  | .cleanupFailureAfterProcessFailure => "cleanup_failure_after_process_failure"
  | .outputEofVsExpiry => "output_eof_vs_expiry"
  | .cleanupCompletionVsExpiry => "cleanup_completion_vs_expiry"
  | .sessionFinishVsLateCancellation => "session_finish_vs_late_cancellation"
  | .reportWriteVsLateError => "report_write_vs_late_error"
  | .blockingCompletionVsDetach => "blocking_completion_vs_detach"

def eventName : Event → String
  | .boot => "boot" | .startProcess => "start_process"
  | .firstInterrupt => "first_interrupt" | .secondInterrupt => "second_interrupt"
  | .deadlineReached => "deadline_reached" | .processExited => "process_exited"
  | .processFailed => "process_failed" | .infrastructureFailed => "infrastructure_failed"
  | .requestTermination => "request_termination" | .reapProcess => "reap_process"
  | .startOutputDrain => "start_output_drain" | .outputDrained => "output_drained"
  | .outputFailed => "output_failed" | .startBlocking => "start_blocking"
  | .blockingCompleted => "blocking_completed" | .detachBlocking => "detach_blocking"
  | .startCleanup => "start_cleanup" | .cleanupCompleted => "cleanup_completed"
  | .cleanupFailed => "cleanup_failed" | .startSessionFinish => "start_session_finish"
  | .sessionFinished => "session_finished" | .sessionFailed => "session_failed"
  | .startReport => "start_report" | .reportWritten => "report_written"
  | .reportFailed => "report_failed" | .startMetrics => "start_metrics"
  | .metricsWritten => "metrics_written" | .metricsFailed => "metrics_failed"
  | .metricsSkipped => "metrics_skipped" | .returnSuccess => "return_success"
  | .returnFailure => "return_failure"

private def causeName : StopCause → String
  | .interrupt => "interrupt" | .forcedInterrupt => "forced_interrupt"
  | .deadline => "deadline" | .processFailure => "process_failure"
  | .infrastructureFailure => "infrastructure_failure"

private def processName : ProcessState → String
  | .notStarted => "not_started" | .running => "running"
  | .terminationRequested => "termination_requested" | .exited => "exited"
  | .reaped => "reaped"

private def componentName : ComponentState → String
  | .absent => "absent" | .pending => "pending" | .complete => "complete"
  | .failed => "failed" | .detached => "detached"

private def optionalJson (name : α → String) : Option α → Lean.Json
  | none => .null
  | some value => .str (name value)

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def observationJson (item : TerminalObservation) : Lean.Json := Lean.Json.mkObj [
  ("cause", optionalJson causeName item.cause),
  ("exit_code", match item.exitCode with | none => .null | some n => Lean.toJson n),
  ("process", .str (processName item.process)),
  ("output", .str (componentName item.output)),
  ("blocking", .str (componentName item.blocking)),
  ("workspace", .str (componentName item.workspace)),
  ("session", .str (componentName item.session)),
  ("report", .str (componentName item.report)),
  ("metrics", .str (componentName item.metrics)),
  ("primary_error", optionalJson causeName item.primaryError),
  ("appended_errors", stringsJson (item.appendedErrors.map causeName)),
  ("dispatches", .arr #[Lean.toJson item.dispatches.1,
    Lean.toJson item.dispatches.2.1, Lean.toJson item.dispatches.2.2.1,
    Lean.toJson item.dispatches.2.2.2]),
  ("session_complete_flag", Lean.toJson item.sessionCompleteFlag),
  ("report_complete_flag", Lean.toJson item.reportCompleteFlag),
  ("returned", Lean.toJson item.returned)
]

def shutdownCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema), ("id", .str item.id),
  ("mode", .str (modeName item.mode)), ("scenario", .str (scenarioName item.scenario)),
  ("schedule", stringsJson (item.schedule.map eventName)),
  ("expected", observationJson item.expected)
]

def renderShutdownCorpus : String :=
  String.join (shutdownCases.map fun item => (shutdownCaseJson item).compress ++ "\n")

example : shutdownCases.length = 12 := by native_decide

end HoiminOracle.ShutdownAudit
