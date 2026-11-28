import Std

namespace HoiminOracle.ProcessOutput

inductive ExecutionKind
  | baseline | mutant | probe
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Termination
  | exitSuccess
  | exitFailure
  | timeout
  | outOfMemory
  | processLimit
  | cancelled
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive ProcessOutcome
  | known (termination : Termination)
  | failed
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive OutputOutcome
  | complete
  | closeTimedOut
  | failed
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive DrainSource
  | stdout | stderr | collector
  deriving Repr, DecidableEq, BEq

-- Ordered evidence from the three output tasks. A definite failure takes
-- precedence over a timeout, with task order breaking ties of the same kind.
def drainEvidence (stdout stderr collector : OutputOutcome) : List (DrainSource × OutputOutcome) :=
  [(.stdout, stdout), (.stderr, stderr), (.collector, collector)]

def selectDrainError (stdout stderr collector : OutputOutcome) : Option (DrainSource × OutputOutcome) :=
  let evidence := drainEvidence stdout stderr collector
  (evidence.find? fun item => item.2 == .failed).orElse
    (fun _ => evidence.find? fun item => item.2 == .closeTimedOut)

inductive Status
  | killed
  | survived
  | timeout
  | outOfMemory
  | processLimit
  | notRun
  | error
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive FailureSource
  | process | output
  deriving Repr, DecidableEq, BEq

structure Decision where
  failure : Option FailureSource := none
  status : Option Status
  termination : Option Termination
  outputIncomplete : Bool
  diagnostic : Bool
  continueRun : Bool
  deriving Repr, DecidableEq, BEq

def Decision.fatal (decision : Decision) : Bool := decision.failure.isSome

def classify : Termination → Status
  | .exitSuccess => .survived
  | .exitFailure => .killed
  | .timeout => .timeout
  | .outOfMemory => .outOfMemory
  | .processLimit => .processLimit
  | .cancelled => .notRun

def decideOutcome : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | _, .failed, _ =>
      { failure := some .process, status := none, termination := none,
        outputIncomplete := false, diagnostic := false, continueRun := false }
  | .mutant, .known termination, .complete =>
      { status := some (classify termination), termination := some termination,
        outputIncomplete := false, diagnostic := false, continueRun := true }
  | _, .known termination, .complete =>
      { status := none, termination := some termination,
        outputIncomplete := false, diagnostic := false, continueRun := true }
  | .mutant, .known termination, .closeTimedOut =>
      { status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | _, .known _, _ =>
      { failure := some .output, status := none, termination := none,
        outputIncomplete := false, diagnostic := false, continueRun := false }

def executions : List ExecutionKind := [.baseline, .mutant, .probe]

def processOutcomes : List ProcessOutcome := [
  .known .exitSuccess,
  .known .exitFailure,
  .known .timeout,
  .known .outOfMemory,
  .known .processLimit,
  .known .cancelled,
  .failed
]

def outputOutcomes : List OutputOutcome := [.complete, .closeTimedOut, .failed]

def drainAssignments : List (OutputOutcome × OutputOutcome × OutputOutcome) :=
  outputOutcomes.flatMap fun stdout => outputOutcomes.flatMap fun stderr =>
    outputOutcomes.map fun collector => (stdout, stderr, collector)

def drainErrorSensitivity : Bool :=
  let evidence := drainEvidence .closeTimedOut .failed .complete
  let broken := evidence.find? fun item => item.2 != .complete
  selectDrainError .closeTimedOut .failed .complete == some (.stderr, .failed) &&
    selectDrainError .closeTimedOut .failed .complete != broken

def assignments : List (ExecutionKind × ProcessOutcome × OutputOutcome) :=
  executions.flatMap fun execution =>
    processOutcomes.flatMap fun process =>
      outputOutcomes.map fun output => (execution, process, output)

def safeDecision (execution : ExecutionKind) (process : ProcessOutcome)
    (output : OutputOutcome) : Bool :=
  let decision := decideOutcome execution process output
  let degradable := execution == .mutant && process matches .known _ &&
    output == .closeTimedOut
  let normal := output == .complete && process matches .known _
  let expectedFailure :=
    match process with
    | .failed => some FailureSource.process
    | .known _ => if degradable || normal then none else some .output
  (decision.failure == expectedFailure) &&
    (decision.outputIncomplete == degradable) &&
    (decision.diagnostic == degradable) &&
    (decision.continueRun == !decision.fatal) &&
    (degradable → decision.status == some .error) &&
    (decision.termination.isSome == !decision.fatal)

def allSafe : Bool := assignments.all fun entry =>
  safeDecision entry.1 entry.2.1 entry.2.2

def brokenDegradeBaseline : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | _, .known termination, .closeTimedOut =>
      { status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenLoseTermination : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known _, .closeTimedOut =>
      { status := some .error, termination := none,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenClassifyTermination : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known termination, .closeTimedOut =>
      { status := some (classify termination), termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenSwallowProcessFailure : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .failed, .closeTimedOut =>
      { status := some .error, termination := none,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenStopAfterError : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known termination, .closeTimedOut =>
      { status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := false }
  | execution, process, output => decideOutcome execution process output

def brokenPreferOutput : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | execution, .failed, .failed =>
      { decideOutcome execution .failed .failed with failure := some .output }
  | execution, process, output => decideOutcome execution process output

def sensitivityPasses : Bool :=
  brokenDegradeBaseline .baseline (.known .exitSuccess) .closeTimedOut !=
      decideOutcome .baseline (.known .exitSuccess) .closeTimedOut &&
    brokenDegradeBaseline .probe (.known .exitSuccess) .closeTimedOut !=
      decideOutcome .probe (.known .exitSuccess) .closeTimedOut &&
    brokenLoseTermination .mutant (.known .timeout) .closeTimedOut !=
      decideOutcome .mutant (.known .timeout) .closeTimedOut &&
    brokenClassifyTermination .mutant (.known .exitSuccess) .closeTimedOut !=
      decideOutcome .mutant (.known .exitSuccess) .closeTimedOut &&
    brokenSwallowProcessFailure .mutant .failed .closeTimedOut !=
      decideOutcome .mutant .failed .closeTimedOut &&
    brokenStopAfterError .mutant (.known .exitFailure) .closeTimedOut !=
      decideOutcome .mutant (.known .exitFailure) .closeTimedOut &&
    brokenPreferOutput .probe .failed .failed != decideOutcome .probe .failed .failed

end HoiminOracle.ProcessOutput
