import Std

namespace HoiminOracle.PreparedNamespace

-- Runtime identity is observable; static trust additionally needs an ordinary
-- namespace. Class-visible lookup includes first iterables and annotations.
def runtimeBuiltin (classVisible injected : Bool) : Bool :=
  !(classVisible && injected)

def trusted (classVisible ordinary : Bool) : Bool := !classVisible || ordinary

def allowed (sourceVisible destinationVisible ordinary : Bool) : Bool :=
  trusted sourceVisible ordinary && trusted destinationVisible ordinary

set_option maxHeartbeats 10000 in
theorem prepared_is_not_trusted : trusted true false = false := by decide

set_option maxHeartbeats 10000 in
theorem emitted_pair_has_builtin_endpoints (sv dv ordinary a b : Bool)
    (hordinary : ordinary = true → a = false ∧ b = false)
    (hallowed : allowed sv dv ordinary = true) :
    runtimeBuiltin sv a = true ∧ runtimeBuiltin dv b = true := by
  cases sv <;> cases dv <;> cases ordinary <;> cases a <;> cases b <;>
    simp_all [allowed, trusted, runtimeBuiltin]

set_option maxHeartbeats 10000 in
theorem lexical_lookup_skips_preparation (ordinary : Bool) :
    allowed false false ordinary = true := by simp [allowed, trusted]

-- Broken alternatives stay in the model, never in production.
def brokenIgnorePreparation (_sv _dv _ordinary : Bool) : Bool := true
def brokenSourceOnly (sv _dv ordinary : Bool) : Bool := trusted sv ordinary
def brokenCaptureClass (_sv _dv ordinary : Bool) : Bool := allowed true true ordinary

def sensitivity : Bool :=
  (brokenIgnorePreparation true true false != allowed true true false) &&
  (brokenSourceOnly false true false != allowed false true false) &&
  (brokenCaptureClass false false false != allowed false false false)

set_option maxHeartbeats 10000 in
example : sensitivity = true := by decide

-- Preserve the historical destination-only runtime counterexample separately
-- from conservative static trust, including a clean custom namespace.
set_option maxHeartbeats 10000 in
example : runtimeBuiltin true false = true ∧ runtimeBuiltin true true = false ∧
    allowed true true false = false := by decide

end HoiminOracle.PreparedNamespace
