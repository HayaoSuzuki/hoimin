import Lean
open Lean
namespace HoiminOracle.ResumeBudget

def eligible (old current : Nat) (complete compatible : Bool) : Bool :=
 !complete && compatible && decide (old ≤ current)
def reusable (old current : Nat) (complete compatible : Bool) (status : String) : Bool :=
 eligible old current complete compatible && (status == "killed" || status == "survived")

set_option maxHeartbeats 10000 in
theorem no_decrease (old current : Nat) (complete compatible : Bool)
    (h : eligible old current complete compatible = true) : old ≤ current := by
 simp only [eligible, Bool.and_eq_true, decide_eq_true_eq] at h
 exact h.2
set_option maxHeartbeats 10000 in
theorem increase_preserves (old current next : Nat) (complete compatible : Bool)
    (h : eligible old current complete compatible = true) (step : current ≤ next) :
    eligible old next complete compatible = true := by
 simp only [eligible, Bool.and_eq_true, decide_eq_true_eq] at *
 exact ⟨h.1, Nat.le_trans h.2 step⟩
set_option maxHeartbeats 10000 in
example : eligible 2 1 false true = false := by decide
set_option maxHeartbeats 10000 in
example : reusable 1 2 false true "timeout" = false := by decide

def render (old current : Nat) (complete compatible : Bool) (status : String) : Json := Json.mkObj [
 ("schema",toJson (1:Nat)),("id",toJson s!"{old}-{current}-{complete}-{compatible}-{status}"),
 ("mode",toJson "strict"),("old_budget",toJson old),("new_budget",toJson current),
 ("complete",toJson complete),("compatible",toJson compatible),("status",toJson status),
 ("eligible",toJson (eligible old current complete compatible)),
 ("reuse",toJson (reusable old current complete compatible status))]
def corpus : String := String.join <|
 [1,2,3].flatMap fun old => [1,2,3].flatMap fun current =>
 [false,true].flatMap fun complete => [false,true].flatMap fun compatible =>
 ["killed","survived","timeout"].map fun status =>
 (render old current complete compatible status).compress ++ "\n"
def sensitivity : Bool :=
 ((!false && true) != eligible 2 1 false true) &&
 (eligible 1 2 false true != reusable 1 2 false true "timeout") &&
 (decide (1 ≤ 2) != eligible 1 2 true true)

-- The selected run remains incomplete and compatible. Ownership has been
-- acquired, and observed is the budget from the initial candidate read.
def recheckBudget (observed stored requested : Nat) : Option Nat :=
  if observed = stored ∧ observed ≤ requested then some requested else none

theorem stale_budget_cannot_write (observed stored requested : Nat) (stale : observed ≠ stored) :
    recheckBudget observed stored requested = none := by
  simp [recheckBudget, stale]

theorem recheck_budget_never_decreases (observed stored requested : Nat) :
    stored ≤ (recheckBudget observed stored requested).getD stored := by
  unfold recheckBudget
  split
  · rename_i current
    simpa [← current.1] using current.2
  · simp

private def brokenRecheckBudget (observed _stored requested : Nat) : Option Nat :=
  if observed ≤ requested then some requested else none

def recheckSensitivity : Bool :=
  recheckBudget 1 2 1 != brokenRecheckBudget 1 2 1 &&
    recheckBudget 255 256 257 != brokenRecheckBudget 255 256 257

private def renderRecheck (observed stored requested : Nat) : Json :=
  let first := recheckBudget observed stored requested
  let afterFirst := first.getD stored
  let retry := recheckBudget afterFirst afterFirst requested
  Json.mkObj [
    ("schema", toJson (1 : Nat)), ("mode", toJson "internal-fixture"),
    ("observed_budget", toJson observed), ("intervening_budget", toJson stored),
    ("requested_budget", toJson requested),
    ("expected_first", toJson (if first.isSome then "resumed" else "candidate_changed")),
    ("expected_after_first", toJson afterFirst),
    ("expected_retry", toJson (if retry.isSome then "resumed" else "budget_decreased")),
    ("expected_after_retry", toJson (retry.getD afterFirst)) ]

def recheckCorpus : String := String.join <|
  [1, 255].flatMap fun observed => [observed, observed + 1].flatMap fun stored =>
  [observed, observed + 1, observed + 2].map fun requested =>
    (renderRecheck observed stored requested).compress ++ "\n"

end HoiminOracle.ResumeBudget
