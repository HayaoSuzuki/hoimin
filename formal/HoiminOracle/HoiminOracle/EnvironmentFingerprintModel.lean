import Lean

namespace HoiminOracle.EnvironmentFingerprint

inductive Value
  | absent | empty | one | zero
  deriving DecidableEq, BEq

def value : Value → Option String
  | .absent => none
  | .empty => some ""
  | .one => some "1"
  | .zero => some "0"

def label : Value → String
  | .absent => "absent"
  | .empty => "empty"
  | .one => "one"
  | .zero => "zero"

def verdict : Value → String
  | .absent | .one => "killed"
  | .empty | .zero => "survived"

def compatible (tracked : Bool) (before after : Value) : Bool :=
  !tracked || value before == value after

def resumed (tracked : Bool) (before after : Value) : String :=
  if compatible tracked before after then verdict before else verdict after

set_option maxHeartbeats 10000 in
theorem tracked_fresh_equivalent (before after : Value) :
    resumed true before after = verdict after := by
  cases before <;> cases after <;> decide

set_option maxHeartbeats 10000 in
theorem presence_encoding_injective (before after : Value) (h : value before = value after) :
    before = after := by
  cases before <;> cases after <;> revert h <;> decide

set_option maxHeartbeats 10000 in
example : compatible true .absent .empty = false := by decide

set_option maxHeartbeats 10000 in
theorem unchanged_tracked_value_reuses (before : Value) : compatible true before before = true := by
  cases before <;> decide

set_option maxHeartbeats 10000 in
theorem untracked_preserves_compatibility (before after : Value) :
    compatible false before after = true := by
  cases before <;> cases after <;> decide

def brokenFlattened (before after : Value) : Bool :=
  (value before).getD "" == (value after).getD ""

def sensitivity : Bool :=
  compatible true .absent .empty != brokenFlattened .absent .empty &&
  compatible true .empty .absent != brokenFlattened .empty .absent &&
  compatible true .one .zero != compatible false .one .zero &&
  compatible true .one .one != false &&
  resumed false .one .zero != verdict .zero

set_option maxHeartbeats 10000 in
example : sensitivity = true := by decide

def values : List Value := [.absent, .empty, .one, .zero]

def cases : List (Bool × Value × Value) :=
  [false, true].flatMap fun tracked => values.flatMap fun before =>
    values.map fun after => (tracked, before, after)

private def observation (status : String) (reuse : Bool) : Lean.Json :=
  Lean.Json.mkObj [
    ("status", Lean.toJson status),
    ("termination_exit", Lean.toJson (if reuse then (none : Option Nat)
      else some (if status == "killed" then 1 else 0))),
    ("executed", Lean.toJson (if reuse then (0 : Nat) else 1)),
    ("exit", Lean.toJson (4 : Nat)),
    ("complete", Lean.toJson false),
    ("baseline_exit", Lean.toJson (0 : Nat)),
    ("other_status", Lean.toJson "not_run")
  ]

private def caseJson (item : Bool × Value × Value) : Lean.Json :=
  let (tracked, before, after) := item
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)),
    ("id", Lean.toJson s!"{if tracked then "tracked" else "untracked"}-{label before}-{label after}"),
    ("mode", Lean.toJson "strict"),
    ("tracked", Lean.toJson tracked),
    ("before", Lean.toJson (value before)),
    ("after", Lean.toJson (value after)),
    ("reuse", Lean.toJson (compatible tracked before after)),
    ("fresh_status", Lean.toJson (verdict after)),
    ("initial", observation (verdict before) false),
    ("resumed", observation (resumed tracked before after) (compatible tracked before after))
  ]

def corpus : String := String.join (cases.map fun item => (caseJson item).compress ++ "\n")

end HoiminOracle.EnvironmentFingerprint
