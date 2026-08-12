import Std

namespace HoiminOracle.TopBudgetProjection

inductive TimeoutMode
  | auto
  | fixed (ticks : Nat)
  deriving Repr, DecidableEq, BEq

structure Input where
  selected : Nat
  jobs : Nat
  plannedTotalTimeout : Nat
  baseline : Nat
  timeoutMode : TimeoutMode
  remaining : Nat
  durationMax : Nat
  ticksPerSecond : Nat
  deriving Repr, DecidableEq, BEq

structure Projection where
  selected : Nat
  jobs : Nat
  plannedTotalTimeout : Nat
  baseline : Nat
  effectiveMutantTimeout : Nat
  remaining : Nat
  waves : Nat
  projectedCapacity : Nat
  shortfall : Bool
  deriving Repr, DecidableEq, BEq

def waves (selected jobs : Nat) : Nat :=
  selected / jobs + if selected % jobs = 0 then 0 else 1

def saturatingAdd (maximum left right : Nat) : Nat :=
  min maximum (left + right)

def saturatingMul (maximum left right : Nat) : Nat :=
  min maximum (left * right)

def autoTimeout (maximum ticksPerSecond baseline : Nat) : Nat :=
  max (min maximum (5 * ticksPerSecond))
    (saturatingAdd maximum
      (saturatingMul maximum baseline 2) ticksPerSecond)

def effectiveTimeout (input : Input) : Nat :=
  match input.timeoutMode with
  | .auto => autoTimeout input.durationMax input.ticksPerSecond input.baseline
  | .fixed ticks => ticks

def project (input : Input) : Projection :=
  let waveCount := waves input.selected input.jobs
  let timeout := effectiveTimeout input
  let capacity := saturatingMul input.durationMax timeout waveCount
  { selected := input.selected
    jobs := input.jobs
    plannedTotalTimeout := input.plannedTotalTimeout
    baseline := input.baseline
    effectiveMutantTimeout := timeout
    remaining := input.remaining
    waves := waveCount
    projectedCapacity := capacity
    shortfall := capacity > input.remaining }

end HoiminOracle.TopBudgetProjection
