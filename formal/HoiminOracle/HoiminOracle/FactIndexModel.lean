import Std

namespace HoiminOracle.FactIndex

structure Range where
  start : Nat
  stop : Nat
  deriving Repr, DecidableEq, BEq

def Range.contains (outer query : Range) : Prop :=
  outer.start ≤ query.start ∧ query.stop ≤ outer.stop

def linearContains (ranges : List Range) (query : Range) : Prop :=
  ∃ outer ∈ ranges, outer.contains query

abbrev ContainmentIndex := Nat → Option Nat

def indexedContains (index : ContainmentIndex) (query : Range) : Prop :=
  ∃ maximumEnd, index query.start = some maximumEnd ∧ query.stop ≤ maximumEnd

def ValidContainmentIndex
    (ranges : List Range)
    (index : ContainmentIndex) : Prop :=
  (∀ queryStart maximumEnd,
    index queryStart = some maximumEnd →
      (∃ outer ∈ ranges, outer.start ≤ queryStart ∧ outer.stop = maximumEnd) ∧
      (∀ outer ∈ ranges, outer.start ≤ queryStart → outer.stop ≤ maximumEnd)) ∧
  (∀ queryStart,
    index queryStart = none ↔
      ¬ ∃ outer ∈ ranges, outer.start ≤ queryStart)

structure Scope where
  start : Nat
  stop : Nat
  ordinal : Nat
  symbol : Nat
  deriving Repr, DecidableEq, BEq

def Scope.contains (scope : Scope) (offset : Nat) : Prop :=
  scope.start ≤ offset ∧ offset < scope.stop

def noLaterThan (candidate selected : Scope) : Prop :=
  candidate.start < selected.start ∨
    (candidate.start = selected.start ∧ candidate.ordinal ≤ selected.ordinal)

def linearSelect (scopes : List Scope) (offset : Nat) (selected : Scope) : Prop :=
  selected ∈ scopes ∧ selected.contains offset ∧
    ∀ candidate ∈ scopes, candidate.contains offset → noLaterThan candidate selected

structure ScopeSegment where
  start : Nat
  stop : Nat
  selected : Scope
  deriving Repr, DecidableEq, BEq

def ScopeSegment.contains (segment : ScopeSegment) (offset : Nat) : Prop :=
  segment.start ≤ offset ∧ offset < segment.stop

def indexedSelect
    (segments : List ScopeSegment)
    (offset : Nat)
    (selected : Scope) : Prop :=
  ∃ segment ∈ segments, segment.contains offset ∧ segment.selected = selected

def ScopeIndexSound (scopes : List Scope) (segments : List ScopeSegment) : Prop :=
  ∀ offset selected, indexedSelect segments offset selected → linearSelect scopes offset selected

def ScopeIndexComplete (scopes : List Scope) (segments : List ScopeSegment) : Prop :=
  ∀ offset selected, linearSelect scopes offset selected → indexedSelect segments offset selected

def ValidScopeIndex (scopes : List Scope) (segments : List ScopeSegment) : Prop :=
  ScopeIndexSound scopes segments ∧ ScopeIndexComplete scopes segments

def brokenLastEligibleContains (ranges : List Range) (query : Range) : Bool :=
  match ranges.foldl
      (fun latest outer => if outer.start ≤ query.start then some outer else latest)
      none with
  | none => false
  | some outer => decide (query.stop ≤ outer.stop)

def brokenLastStartedScope (scopes : List Scope) (offset : Nat) : Option Nat :=
  (scopes.foldl
    (fun latest scope => if scope.start ≤ offset then some scope else latest)
    none).map Scope.symbol

end HoiminOracle.FactIndex

