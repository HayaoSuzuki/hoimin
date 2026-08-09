import Std

namespace HoiminOracle.ShutdownAudit

inductive StopCause
  | interrupt | forcedInterrupt | deadline | processFailure | infrastructureFailure
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, Hashable

inductive ComponentState
  | absent | pending | complete | failed | detached
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, Hashable

inductive ProcessState
  | notStarted | running | terminationRequested | exited | reaped
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, Hashable

inductive Event
  | boot | startProcess | firstInterrupt | secondInterrupt | deadlineReached
  | processExited | processFailed | infrastructureFailed
  | requestTermination | reapProcess
  | startOutputDrain | outputDrained | outputFailed
  | startBlocking | blockingCompleted | detachBlocking
  | startCleanup | cleanupCompleted | cleanupFailed
  | startSessionFinish | sessionFinished | sessionFailed
  | startReport | reportWritten | reportFailed
  | startMetrics | metricsWritten | metricsFailed | metricsSkipped
  | returnSuccess | returnFailure
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, Hashable

structure State where
  cause : Option StopCause := none
  deadlineOrdinal : Option Nat := none
  process : ProcessState := .notStarted
  output : ComponentState := .absent
  blocking : ComponentState := .absent
  workspace : ComponentState := .absent
  session : ComponentState := .absent
  report : ComponentState := .absent
  metrics : ComponentState := .absent
  blockingOwnershipTransferred : Bool := false
  primaryError : Option StopCause := none
  appendedErrors : List StopCause := []
  processStarts : Nat := 0
  terminationRequests : Nat := 0
  reaps : Nat := 0
  cleanupDispatches : Nat := 0
  sessionDispatches : Nat := 0
  reportDispatches : Nat := 0
  metricsDispatches : Nat := 0
  sessionCompleteFlag : Bool := false
  reportCompleteFlag : Bool := false
  exitCode : Option Nat := none
  returned : Bool := false
  causeOverwrites : Nat := 0
  primaryOverwrites : Nat := 0
  regressions : Nat := 0
  orderingViolations : Nat := 0
  forcedWaits : Nat := 0
  lostOwnership : Bool := false
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq, Hashable

def State.initial : State := {}

def atMostOne (value : Nat) : Bool := value == 0 || value == 1

def safe (state : State) : Bool :=
  atMostOne state.processStarts && atMostOne state.terminationRequests &&
  atMostOne state.reaps && atMostOne state.cleanupDispatches &&
  atMostOne state.sessionDispatches && atMostOne state.reportDispatches &&
  atMostOne state.metricsDispatches &&
  (!(state.blocking == .detached) || state.blockingOwnershipTransferred) &&
  (!(state.process == .reaped) || state.reaps == 1) &&
  (!(state.exitCode == some 130) || state.cause == some .interrupt ||
    state.cause == some .forcedInterrupt) &&
  state.causeOverwrites == 0 && state.primaryOverwrites == 0 &&
  state.regressions == 0 && state.orderingViolations == 0 &&
  state.forcedWaits == 0 && !state.lostOwnership &&
  (!(state.sessionCompleteFlag) || state.session == .complete) &&
  (!(state.reportCompleteFlag) || state.report == .complete) &&
  (!(state.exitCode == some 130) || !(state.blocking == .pending)) &&
  (!state.returned || state.exitCode.isSome)

inductive Rejection
  | invalidPhase | duplicate | missingOwnership | alreadyReturned
  deriving Repr, DecidableEq, BEq

structure Verdict where
  state : State
  rejection : Option Rejection := none
  deriving Repr, DecidableEq, BEq

structure Observation where
  event : Event
  rejection : Option Rejection
  cause : Option StopCause
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
  exitCode : Option Nat
  returned : Bool
  deriving Repr, DecidableEq, BEq

def accept (state : State) : Verdict := { state }

def reject (state : State) (reason : Rejection) : Verdict :=
  { state, rejection := some reason }

def settled : ComponentState → Bool
  | .complete | .failed | .detached => true
  | .absent | .pending => false

def processTerminal : ProcessState → Bool
  | .notStarted | .reaped => true
  | .running | .terminationRequested | .exited => false

def installError (state : State) (failure : StopCause) : State :=
  match state.primaryError with
  | none =>
      { state with
        cause := match state.cause with | none => some failure | cause => cause
        deadlineOrdinal := match state.deadlineOrdinal with
          | none => some 0
          | ordinal => ordinal
        primaryError := some failure }
  | some _ => { state with appendedErrors := state.appendedErrors ++ [failure] }

def readyForReport (state : State) : Bool :=
  processTerminal state.process && settled state.workspace && settled state.session

def readyForSuccess (state : State) : Bool :=
  processTerminal state.process &&
  (state.output == .absent || state.output == .complete) &&
  state.workspace == .complete && state.session == .complete &&
  state.report == .complete &&
  (state.metrics == .absent || state.metrics == .complete) &&
  state.sessionCompleteFlag && state.reportCompleteFlag &&
  state.primaryError.isNone

def observe (event : Event) (verdict : Verdict) : Observation :=
  let state := verdict.state
  {
    event
    rejection := verdict.rejection
    cause := state.cause
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
    exitCode := state.exitCode
    returned := state.returned
  }

def step (state : State) (event : Event) : Verdict :=
  if state.returned then
    reject state .alreadyReturned
  else
    match event with
    | .boot =>
        if state == State.initial then
          accept { state with
            workspace := .pending
            session := .pending }
        else
          reject state .duplicate
    | .startProcess =>
        if state.cause.isSome then reject state .invalidPhase
        else if state.process == .notStarted && state.processStarts == 0 then
          accept { state with
            process := .running
            processStarts := 1 }
        else reject state .duplicate
    | .firstInterrupt =>
        match state.cause with
        | none => accept { state with
            cause := some .interrupt
            deadlineOrdinal := some 0
            primaryError := match state.primaryError with
              | none => some .interrupt
              | failure => failure }
        | some _ => accept state
    | .secondInterrupt =>
        let blocked := state.blocking == .pending
        accept { state with
          cause := some .forcedInterrupt
          deadlineOrdinal := match state.deadlineOrdinal with
            | none => some 0
            | ordinal => ordinal
          blocking := if blocked then .detached else state.blocking
          blockingOwnershipTransferred :=
            if blocked then true else state.blockingOwnershipTransferred
          primaryError := match state.primaryError with
            | none => some .interrupt
            | failure => failure
          exitCode := some 130
          returned := true }
    | .deadlineReached =>
        match state.cause with
        | none => accept { state with
            cause := some .deadline
            deadlineOrdinal := some 0
            primaryError := match state.primaryError with
              | none => some .deadline
              | failure => failure }
        | some _ => accept state
    | .processExited =>
        if state.process == .running || state.process == .terminationRequested then
          accept { state with process := .exited }
        else reject state .invalidPhase
    | .processFailed =>
        if state.process == .running || state.process == .terminationRequested then
          accept (installError { state with process := .exited } .processFailure)
        else reject state .invalidPhase
    | .infrastructureFailed => accept (installError state .infrastructureFailure)
    | .requestTermination =>
        if state.cause.isSome && state.process == .running &&
            state.terminationRequests == 0 then
          accept { state with
            process := .terminationRequested
            terminationRequests := 1 }
        else reject state .invalidPhase
    | .reapProcess =>
        if state.process == .exited && state.reaps == 0 then
          accept { state with
            process := .reaped
            reaps := 1 }
        else reject state .invalidPhase
    | .startOutputDrain =>
        if state.output == .absent then accept { state with output := .pending }
        else reject state .duplicate
    | .outputDrained =>
        if state.output == .pending then accept { state with output := .complete }
        else reject state .invalidPhase
    | .outputFailed =>
        if state.output == .pending then
          accept (installError { state with output := .failed } .infrastructureFailure)
        else reject state .invalidPhase
    | .startBlocking =>
        if state.blocking == .absent then accept { state with blocking := .pending }
        else reject state .duplicate
    | .blockingCompleted =>
        if state.blocking == .pending then accept { state with blocking := .complete }
        else reject state .invalidPhase
    | .detachBlocking =>
        if state.blocking == .pending then accept { state with
          blocking := .detached
          blockingOwnershipTransferred := true }
        else reject state .missingOwnership
    | .startCleanup =>
        if state.workspace == .pending && state.cleanupDispatches == 0 then
          accept { state with cleanupDispatches := 1 }
        else reject state .duplicate
    | .cleanupCompleted =>
        if state.workspace == .pending && state.cleanupDispatches == 1 then
          accept { state with workspace := .complete }
        else reject state .invalidPhase
    | .cleanupFailed =>
        if state.workspace == .pending && state.cleanupDispatches == 1 then
          accept (installError { state with workspace := .failed }
            .infrastructureFailure)
        else reject state .invalidPhase
    | .startSessionFinish =>
        if settled state.workspace && state.session == .pending &&
            state.sessionDispatches == 0 then
          accept { state with sessionDispatches := 1 }
        else reject state .invalidPhase
    | .sessionFinished =>
        if state.session == .pending && state.sessionDispatches == 1 then
          accept { state with
            session := .complete
            sessionCompleteFlag := state.cause.isNone }
        else reject state .invalidPhase
    | .sessionFailed =>
        if state.session == .pending && state.sessionDispatches == 1 then
          accept (installError { state with session := .failed }
            .infrastructureFailure)
        else reject state .invalidPhase
    | .startReport =>
        if readyForReport state && state.report == .absent &&
            state.reportDispatches == 0 then
          accept { state with
            report := .pending
            reportDispatches := 1 }
        else reject state .invalidPhase
    | .reportWritten =>
        if state.report == .pending && state.reportDispatches == 1 then
          accept { state with
            report := .complete
            reportCompleteFlag := state.cause.isNone }
        else reject state .invalidPhase
    | .reportFailed =>
        if state.report == .pending && state.reportDispatches == 1 then
          accept (installError { state with report := .failed }
            .infrastructureFailure)
        else reject state .invalidPhase
    | .startMetrics =>
        if settled state.report && state.metrics == .absent &&
            state.metricsDispatches == 0 then
          accept { state with
            metrics := .pending
            metricsDispatches := 1 }
        else reject state .invalidPhase
    | .metricsWritten =>
        if state.metrics == .pending && state.metricsDispatches == 1 then
          accept { state with metrics := .complete }
        else reject state .invalidPhase
    | .metricsFailed =>
        if state.metrics == .pending && state.metricsDispatches == 1 then
          accept (installError { state with metrics := .failed }
            .infrastructureFailure)
        else reject state .invalidPhase
    | .metricsSkipped =>
        if state.metrics == .pending && state.metricsDispatches == 1 then
          accept { state with metrics := .complete }
        else reject state .invalidPhase
    | .returnSuccess =>
        if readyForSuccess state then
          accept { state with
            exitCode := some 0
            returned := true }
        else reject state .invalidPhase
    | .returnFailure =>
        match state.primaryError with
        | none => reject state .invalidPhase
        | some primary =>
            if state.process == .running || state.process == .terminationRequested ||
                state.process == .exited || state.blocking == .pending then
              reject state .invalidPhase
            else
              let code := match state.cause.getD primary with
                | .interrupt | .forcedInterrupt => 130
                | .deadline => 4
                | .processFailure | .infrastructureFailure => 3
              accept { state with
                exitCode := some code
                returned := true }

def run : State → List Event → State
  | state, [] => state
  | state, event :: rest => run (step state event).state rest

def brokenCauseStep (state : State) (event : Event) : Verdict :=
  if event == .deadlineReached && state.cause.isSome && !state.returned then
    accept { state with
      cause := some .deadline
      causeOverwrites := state.causeOverwrites + 1 }
  else step state event

def brokenDuplicateStep (state : State) (event : Event) : Verdict :=
  if event == .startCleanup && state.cleanupDispatches == 1 && !state.returned then
    accept { state with cleanupDispatches := state.cleanupDispatches + 1 }
  else step state event

def brokenOrderingStep (state : State) (event : Event) : Verdict :=
  if event == .startReport && !state.returned then
    accept { state with
      report := .pending
      reportDispatches := state.reportDispatches + 1
      orderingViolations := state.orderingViolations + 1 }
  else step state event

def brokenPrecedenceStep (state : State) (event : Event) : Verdict :=
  if event == .cleanupFailed && state.primaryError.isSome && !state.returned then
    let verdict := step state event
    accept { verdict.state with
      primaryError := some .infrastructureFailure
      primaryOverwrites := state.primaryOverwrites + 1 }
  else step state event

def brokenForcedWaitStep (state : State) (event : Event) : Verdict :=
  if event == .secondInterrupt && state.blocking == .pending && !state.returned then
    accept { state with
      cause := some .forcedInterrupt
      primaryError := match state.primaryError with
        | none => some .interrupt
        | failure => failure
      forcedWaits := state.forcedWaits + 1 }
  else step state event

def brokenRegressionStep (state : State) (event : Event) : Verdict :=
  if event == .firstInterrupt && state.session == .complete && !state.returned then
    let verdict := step state event
    accept { verdict.state with
      session := .pending
      regressions := state.regressions + 1 }
  else step state event

def brokenDetachStep (state : State) (event : Event) : Verdict :=
  if event == .detachBlocking && state.blocking == .pending && !state.returned then
    accept { state with
      blocking := .detached
      blockingOwnershipTransferred := false
      lostOwnership := true }
  else step state event

def causeWitness : List Event :=
  [.boot, .startProcess, .firstInterrupt, .deadlineReached]

def duplicateWitness : List Event :=
  [.boot, .firstInterrupt, .startCleanup, .startCleanup]

def orderingWitness : List Event :=
  [.boot, .startProcess, .firstInterrupt, .startReport, .reportWritten]

def precedenceWitness : List Event :=
  [.boot, .startProcess, .processFailed, .startCleanup, .cleanupFailed]

def forcedWaitWitness : List Event :=
  [.boot, .startBlocking, .firstInterrupt, .secondInterrupt]

def regressionWitness : List Event :=
  [.boot, .startCleanup, .cleanupCompleted, .startSessionFinish,
    .sessionFinished, .firstInterrupt]

def detachWitness : List Event :=
  [.boot, .startBlocking, .firstInterrupt, .detachBlocking]

private def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

example : safe (run State.initial causeWitness) = true := by decide
example : safe (runWith brokenCauseStep State.initial causeWitness) = false := by decide
example : safe (run State.initial duplicateWitness) = true := by decide
example : safe (runWith brokenDuplicateStep State.initial duplicateWitness) = false := by decide
example : safe (run State.initial orderingWitness) = true := by decide
example : safe (runWith brokenOrderingStep State.initial orderingWitness) = false := by decide
example : safe (run State.initial precedenceWitness) = true := by decide
example : safe (runWith brokenPrecedenceStep State.initial precedenceWitness) = false := by decide
example : safe (run State.initial forcedWaitWitness) = true := by decide
example : safe (runWith brokenForcedWaitStep State.initial forcedWaitWitness) = false := by decide
example : safe (run State.initial regressionWitness) = true := by decide
example : safe (runWith brokenRegressionStep State.initial regressionWitness) = false := by decide
example : safe (run State.initial detachWitness) = true := by decide
example : safe (runWith brokenDetachStep State.initial detachWitness) = false := by decide

end HoiminOracle.ShutdownAudit
