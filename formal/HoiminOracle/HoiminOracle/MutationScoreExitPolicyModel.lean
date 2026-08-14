import Std

namespace HoiminOracle.MutationScoreExitPolicy

inductive Status
  | killed
  | survived
  | timeout
  | outOfMemory
  | processLimit
  | error
  | notRun
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Counts where
  killed : Nat := 0
  survived : Nat := 0
  timeout : Nat := 0
  outOfMemory : Nat := 0
  processLimit : Nat := 0
  error : Nat := 0
  notRun : Nat := 0
  deriving Repr, DecidableEq, BEq

def record (counts : Counts) : Status → Counts
  | .killed => { counts with killed := counts.killed + 1 }
  | .survived => { counts with survived := counts.survived + 1 }
  | .timeout => { counts with timeout := counts.timeout + 1 }
  | .outOfMemory => { counts with outOfMemory := counts.outOfMemory + 1 }
  | .processLimit => { counts with processLimit := counts.processLimit + 1 }
  | .error => { counts with error := counts.error + 1 }
  | .notRun => { counts with notRun := counts.notRun + 1 }

def summarize (statuses : List Status) : Counts :=
  statuses.foldl record {}

def countStatus (counts : Counts) : Status → Nat
  | .killed => counts.killed
  | .survived => counts.survived
  | .timeout => counts.timeout
  | .outOfMemory => counts.outOfMemory
  | .processLimit => counts.processLimit
  | .error => counts.error
  | .notRun => counts.notRun

def total (counts : Counts) : Nat :=
  counts.killed + counts.survived + counts.timeout + counts.outOfMemory +
    counts.processLimit + counts.error + counts.notRun

def decidable (counts : Counts) : Nat := counts.killed + counts.survived

def inconclusive (counts : Counts) : Nat :=
  counts.timeout + counts.outOfMemory + counts.processLimit + counts.error +
    counts.notRun

structure ExactFraction where
  numerator : Nat
  denominator : Nat
  deriving Repr, DecidableEq, BEq

def reduceFraction (numerator denominator : Nat) : ExactFraction :=
  let divisor := Nat.gcd numerator denominator
  { numerator := numerator / divisor
    denominator := denominator / divisor }

def exactScore (counts : Counts) : Option ExactFraction :=
  if decidable counts = 0 then none
  else some (reduceFraction counts.killed (decidable counts))

structure ExitPolicy where
  infrastructureError : Bool := false
  baselineFailed : Bool := false
  incomplete : Bool := false
  survivors : Bool := false
  interrupted : Bool := false
  deriving Repr, DecidableEq, BEq

def policyFromCounts (counts : Counts) : ExitPolicy where
  infrastructureError := counts.error > 0
  incomplete := counts.timeout > 0 || counts.outOfMemory > 0 ||
    counts.processLimit > 0 || counts.notRun > 0
  survivors := counts.survived > 0

structure RunFlags where
  infrastructureError : Bool := false
  baselineFailed : Bool := false
  incomplete : Bool := false
  interrupted : Bool := false
  deriving Repr, DecidableEq, BEq

def composePolicy (counts : Counts) (flags : RunFlags) : ExitPolicy :=
  let summary := policyFromCounts counts
  { infrastructureError := flags.infrastructureError || summary.infrastructureError
    baselineFailed := flags.baselineFailed
    incomplete := flags.incomplete || summary.incomplete
    survivors := summary.survivors
    interrupted := flags.interrupted }

def exitCode (policy : ExitPolicy) : Nat :=
  if policy.interrupted then 130
  else if policy.infrastructureError then 2
  else if policy.baselineFailed then 3
  else if policy.incomplete then 4
  else if policy.survivors then 1
  else 0

def complete (policy : ExitPolicy) : Bool :=
  !policy.infrastructureError && !policy.baselineFailed && !policy.incomplete &&
    !policy.interrupted

structure Observation where
  counts : Counts
  score : Option ExactFraction
  policy : ExitPolicy
  complete : Bool
  exitCode : Nat
  deriving Repr, DecidableEq, BEq

def observe (statuses : List Status) (flags : RunFlags) : Observation :=
  let counts := summarize statuses
  let policy := composePolicy counts flags
  { counts := counts
    score := exactScore counts
    policy := policy
    complete := complete policy
    exitCode := exitCode policy }

end HoiminOracle.MutationScoreExitPolicy
