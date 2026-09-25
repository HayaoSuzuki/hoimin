import Lean
open Lean
namespace HoiminOracle.ResumeDiagnostic
structure Row where
  name : String
  compatible : Bool
  complete : Bool
  budget : Nat
  deriving Repr, BEq

def eligible (row : Row) : Bool := row.compatible && !row.complete && decide (row.budget ≤ 2)
def historyReason (eligibleNow decreased matchingComplete incomplete any : Bool) : String :=
  if eligibleNow then "candidate_changed"
  else if decreased then "budget_decreased"
  else if matchingComplete then "matching_run_complete"
  else if incomplete then "fingerprint_mismatch"
  else if any then "no_incomplete_run"
  else "no_prior_run"
def reason (rows : List Row) : String :=
  historyReason (rows.any eligible)
    (rows.any fun row => row.compatible && !row.complete && decide (2 < row.budget))
    (rows.any fun row => row.compatible && row.complete)
    (rows.any fun row => !row.complete) (!rows.isEmpty)

def selected (rows : List Row) : Option String :=
  (rows.reverse.find? eligible).map (·.name)
set_option maxHeartbeats 10000 in
example : selected [⟨"old", true, false, 1⟩] = some "old" := by decide


set_option maxHeartbeats 10000 in
theorem history_change_wins (b c i a : Bool) :
    historyReason true b c i a = "candidate_changed" := by rfl
set_option maxHeartbeats 10000 in
theorem decreased_wins (c i a : Bool) :
    historyReason false true c i a = "budget_decreased" := by rfl
set_option maxHeartbeats 10000 in
theorem completed_match_wins (i a : Bool) :
    historyReason false false true i a = "matching_run_complete" := by rfl
set_option maxHeartbeats 10000 in
theorem incomplete_wins (a : Bool) :
    historyReason false false false true a = "fingerprint_mismatch" := by rfl

def outcome (phase : String) (rows : List Row) : Option String × Option String :=
  if phase == "lost_eligibility" then (none, some "candidate_changed")
  else if phase == "none_then_eligible" then (none, some (reason rows))
  else match selected rows with
    | some run => (some run, none)
    | none => (none, some (reason rows))

set_option maxHeartbeats 10000 in
theorem owned_candidate_wins (rows : List Row) (run : String) (h : selected rows = some run) :
    outcome "normal" rows = (some run, none) := by simp [outcome, h]
set_option maxHeartbeats 10000 in
theorem lost_candidate_is_fresh (rows : List Row) :
    outcome "lost_eligibility" rows = (none, some "candidate_changed") := by rfl

-- Each enabled history category creates one row, except eligible creates two
-- distinguishable rows so the adapter checks newest selection in both orders.
def rowsFor (e b c i a reverse : Bool) : List Row :=
  let rows :=
    (if e then [⟨"eligible_older", true, false, 1⟩, ⟨"eligible_newer", true, false, 2⟩] else []) ++
    (if b then [⟨"budget", true, false, 3⟩] else []) ++
    (if c then [⟨"matching_complete", true, true, 2⟩] else []) ++
    (if i then [⟨"other_incomplete", false, false, 2⟩] else []) ++
    (if a then [⟨"other_complete", false, true, 2⟩] else [])
  if reverse then rows.reverse else rows

def render (phase : String) (e b c i a reverse : Bool) : Json :=
  let rows := rowsFor e b c i a reverse
  let observed := outcome phase rows
  Json.mkObj [
    ("schema", toJson (1 : Nat)),
    ("id", toJson s!"{phase}-{e}-{b}-{c}-{i}-{a}-{reverse}"),
    ("mode", toJson (if phase == "normal" then "strict" else "model-only")),
    ("phase", toJson phase), ("eligible", toJson e), ("budget_decreased", toJson b),
    ("matching_complete", toJson c), ("other_incomplete", toJson i),
    ("other_complete", toJson a), ("reverse", toJson reverse),
    ("rows", toJson (rows.map fun row => Json.mkObj [
      ("name", toJson row.name), ("compatible", toJson row.compatible),
      ("complete", toJson row.complete), ("budget", toJson row.budget)])),
    ("selected_run", toJson observed.1), ("reason", toJson observed.2)]

def strictCases : List Json :=
  [false,true].flatMap fun e => [false,true].flatMap fun b =>
  [false,true].flatMap fun c => [false,true].flatMap fun i =>
  [false,true].flatMap fun a => [false,true].map fun reverse =>
  render "normal" e b c i a reverse

def raceCases : List Json :=
  ([false,true].flatMap fun b => [false,true].flatMap fun c =>
   [false,true].flatMap fun i => [false,true].map fun a =>
   render "none_then_eligible" true b c i a false) ++
  [render "lost_eligibility" false false false false false false]

def corpus : String := String.join ((strictCases ++ raceCases).map (fun row => row.compress ++ "\n"))

-- Deliberately broken models: always-fresh, first-eligible, no-history priority,
-- mismatch-before-matching-complete, complete-before-budget, and stale-race mismatch.
def sensitivity : Bool :=
  selected (rowsFor true false false false false false) != none &&
  selected (rowsFor true false false false false false) != some "eligible_older" &&
  reason [] != "fingerprint_mismatch" &&
  reason (rowsFor false false true true false false) != "fingerprint_mismatch" &&
  reason (rowsFor false true true false false false) != "matching_run_complete" &&
  (outcome "none_then_eligible" (rowsFor true false false false false false)).2 != some "fingerprint_mismatch"

set_option maxHeartbeats 10000 in
example : sensitivity = true := by decide
end HoiminOracle.ResumeDiagnostic
