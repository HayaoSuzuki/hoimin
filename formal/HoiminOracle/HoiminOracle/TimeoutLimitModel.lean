import Std

namespace HoiminOracle.TimeoutLimit

inductive MutantTimeout
  | auto
  | fixed (nanoseconds : Nat)
  deriving Repr, DecidableEq, BEq

structure Input where
  analyzer : Nat
  baseline : Nat
  mutant : MutantTimeout
  total : Nat
  maximum : Nat
  minimumAuto : Nat
  oneSecond : Nat
  deriving Repr, DecidableEq, BEq

def autoTimeout (input : Input) : Nat :=
  max input.minimumAuto (2 * input.baseline + input.oneSecond)

def effectiveMutantTimeout (input : Input) : Nat :=
  match input.mutant with
  | .auto => autoTimeout input
  | .fixed nanoseconds => nanoseconds

def durationValid (maximum value : Nat) : Bool :=
  value > 0 && value ≤ maximum

def invalidField (input : Input) : Option String :=
  if !durationValid input.maximum input.analyzer then some "analyzer_timeout"
  else if !durationValid input.maximum input.baseline then some "baseline_timeout"
  else if !durationValid input.maximum input.total then some "total_timeout"
  else match input.mutant with
    | .auto =>
        if !durationValid input.maximum (autoTimeout input)
        then some "baseline_timeout"
        else none
    | .fixed nanoseconds =>
        if !durationValid input.maximum nanoseconds
        then some "mutant_timeout"
        else none

def accepts (input : Input) : Bool :=
  durationValid input.maximum input.analyzer &&
    durationValid input.maximum input.baseline &&
    durationValid input.maximum (effectiveMutantTimeout input) &&
    durationValid input.maximum input.total

end HoiminOracle.TimeoutLimit
