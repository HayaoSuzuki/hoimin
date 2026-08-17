import Std

namespace HoiminOracle.ProcessOutput

inductive ExecutionKind
  | baseline
  | mutant
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

inductive Status
  | killed
  | survived
  | timeout
  | outOfMemory
  | processLimit
  | notRun
  | error
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Decision where
  fatal : Bool
  status : Option Status
  termination : Option Termination
  outputIncomplete : Bool
  diagnostic : Bool
  continueRun : Bool
  deriving Repr, DecidableEq, BEq

def classify : Termination → Status
  | .exitSuccess => .survived
  | .exitFailure => .killed
  | .timeout => .timeout
  | .outOfMemory => .outOfMemory
  | .processLimit => .processLimit
  | .cancelled => .notRun

def decideOutcome : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | _, .failed, _ =>
      { fatal := true, status := none, termination := none,
        outputIncomplete := false, diagnostic := false, continueRun := false }
  | .baseline, .known termination, .complete =>
      { fatal := false, status := none, termination := some termination,
        outputIncomplete := false, diagnostic := false, continueRun := true }
  | .mutant, .known termination, .complete =>
      { fatal := false, status := some (classify termination), termination := some termination,
        outputIncomplete := false, diagnostic := false, continueRun := true }
  | .mutant, .known termination, .closeTimedOut =>
      { fatal := false, status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | _, .known _, _ =>
      { fatal := true, status := none, termination := none,
        outputIncomplete := false, diagnostic := false, continueRun := false }

def executions : List ExecutionKind := [.baseline, .mutant]

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
  (decision.fatal == !(degradable || normal)) &&
    (decision.outputIncomplete == degradable) &&
    (decision.diagnostic == degradable) &&
    (decision.continueRun == !decision.fatal) &&
    (degradable → decision.status == some .error) &&
    (decision.termination.isSome == !decision.fatal)

def allSafe : Bool := assignments.all fun entry =>
  safeDecision entry.1 entry.2.1 entry.2.2

def brokenDegradeBaseline : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | _, .known termination, .closeTimedOut =>
      { fatal := false, status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenLoseTermination : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known _, .closeTimedOut =>
      { fatal := false, status := some .error, termination := none,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenClassifyTermination : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known termination, .closeTimedOut =>
      { fatal := false, status := some (classify termination), termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenSwallowProcessFailure : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .failed, .closeTimedOut =>
      { fatal := false, status := some .error, termination := none,
        outputIncomplete := true, diagnostic := true, continueRun := true }
  | execution, process, output => decideOutcome execution process output

def brokenStopAfterError : ExecutionKind → ProcessOutcome → OutputOutcome → Decision
  | .mutant, .known termination, .closeTimedOut =>
      { fatal := false, status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := false }
  | execution, process, output => decideOutcome execution process output

def sensitivityPasses : Bool :=
  brokenDegradeBaseline .baseline (.known .exitSuccess) .closeTimedOut !=
      decideOutcome .baseline (.known .exitSuccess) .closeTimedOut &&
    brokenLoseTermination .mutant (.known .timeout) .closeTimedOut !=
      decideOutcome .mutant (.known .timeout) .closeTimedOut &&
    brokenClassifyTermination .mutant (.known .exitSuccess) .closeTimedOut !=
      decideOutcome .mutant (.known .exitSuccess) .closeTimedOut &&
    brokenSwallowProcessFailure .mutant .failed .closeTimedOut !=
      decideOutcome .mutant .failed .closeTimedOut &&
    brokenStopAfterError .mutant (.known .exitFailure) .closeTimedOut !=
      decideOutcome .mutant (.known .exitFailure) .closeTimedOut

end HoiminOracle.ProcessOutput
