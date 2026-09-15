import HoiminOracle.BoundedCandidateDiscoveryProofs
import HoiminOracle.BindingFlowModel

namespace HoiminOracle.PerformanceCost

open HoiminOracle.BindingFlow

inductive Effect | bind | maybeBind | unknown deriving Repr, BEq, DecidableEq
structure Event where
  offset : Nat
  effect : Effect
  deriving Repr, BEq, DecidableEq

def applyEffect (state : Fact) : Effect → Fact
  | .bind => .shadowed
  | .maybeBind => state.meet .shadowed
  | .unknown => .unknown

def resolve (events : List Event) (offset : Nat) : Fact :=
  events.foldl (fun state event =>
    if event.offset ≤ offset then applyEffect state event.effect else state) (.known .builtin)

-- Worst branch of a binary search of N sorted distinct offsets.
def halfSteps (n : Nat) : Nat :=
  if n = 0 then 0 else 1 + halfSteps (n / 2)
termination_by n
decreasing_by omega

def binaryCost (n : Nat) (directions : List Bool) : Nat :=
  if n = 0 then 0 else
    let next := if directions.headD false then n / 2 else (n - 1) / 2
    1 + binaryCost next (directions.drop 1)
termination_by n
decreasing_by split <;> omega

def treeHeight (n : Nat) : Nat := halfSteps (n - 1)
def buildUpdates (bindings : Nat) : Nat := bindings * (treeHeight bindings + 1)
def queryBound (unique references : Nat) : Nat := references * halfSteps unique
def linearVisits (bindings references : Nat) : Nat := bindings * references

def buildTrace : Nat → Nat → Nat
  | 0, _ => 0
  | bindings + 1, height => (height + 1) + buildTrace bindings height

def cloneEntries (aliases annotations : Nat) : Nat := aliases * annotations
def eagerReplacementBytes (replacementLengths : List Nat) : Nat := replacementLengths.sum

def filteredReplacementBytes (selected : Bool) (lengths : List Nat) : Nat :=
  if selected then lengths.sum else 0

-- Historical defective replay: each loop replays its nested body twice.
def replayLoopVisits : Nat → Nat
  | 0 => 1
  | depth + 1 => 2 * replayLoopVisits depth

end HoiminOracle.PerformanceCost
