import Lean
open Lean

namespace HoiminOracle.SourceOrder
inductive Order where
 | ab | ba
 deriving Repr, DecidableEq, BEq
inductive Kind where
 | source | imports
 deriving Repr, DecidableEq, BEq

def verdict : Order → String
 | .ab => "killed"
 | .ba => "survived"
def compatible (old current : Order) : Bool := old == current
def resumedVerdict (old current : Order) : String :=
 if compatible old current then verdict old else verdict current
def brokenCompatible (kind : Kind) (old current : Order) : Bool :=
 match kind with
 | .source => true
 | .imports => compatible old current
def brokenVerdict (kind : Kind) (old current : Order) : String :=
 if brokenCompatible kind old current then verdict old else verdict current

set_option maxHeartbeats 10000 in
theorem compatible_verdict (old current : Order) (h : compatible old current = true) :
 verdict old = verdict current := by cases old <;> cases current <;> revert h <;> decide
set_option maxHeartbeats 10000 in
theorem correct_resume (old current : Order) : resumedVerdict old current = verdict current := by
 cases old <;> cases current <;> decide
set_option maxHeartbeats 10000 in
example : brokenVerdict .source .ab .ba != verdict .ba := by decide
set_option maxHeartbeats 10000 in
example : compatible .ab .ab = true := by decide

def roots : Order → List String
 | .ab => ["a","b"]
 | .ba => ["b","a"]

def render (kind : Kind) (old current : Order) : Json := Json.mkObj [
 ("mode",toJson "strict"),
 ("kind",toJson (if kind == .source then "source" else "imports")),
 ("old",toJson (roots old)),("current",toJson (roots current)),
 ("initial_status",toJson (verdict old)),("status",toJson (verdict current)),
 ("reuse",toJson (compatible old current)),
 ("termination_exit",toJson (if compatible old current then (none : Option Nat) else some (if current == .ab then 1 else 0))),
 ("executed",toJson (if compatible old current then (0:Nat) else 1)),
 ("exit",toJson (4:Nat)),("complete",toJson false),
 ("baseline_exit",toJson (0:Nat)),("other_status",toJson "not_run")]

def corpus : String := String.join <|
 [Kind.source,Kind.imports].flatMap fun kind =>
  [Order.ab,Order.ba].flatMap fun old =>
   [Order.ab,Order.ba].map fun current => (render kind old current).compress ++ "\n"

def sensitivity : Bool :=
 brokenVerdict .source .ab .ba != verdict .ba &&
 brokenVerdict .source .ba .ab != verdict .ab

end HoiminOracle.SourceOrder
