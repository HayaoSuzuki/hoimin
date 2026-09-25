import Std

namespace HoiminOracle.PreparedAnnotationImport

-- Runtime permission from issue 605 stays separate from conservative inference.
def identity (visible injected : Bool) : Bool := !(visible && injected)
def allowed (sv dv a b : Bool) : Bool := identity sv a && identity dv b
def eligible (sv dv ordinary : Bool) : Bool := (!sv || ordinary) && (!dv || ordinary)

set_option maxHeartbeats 10000 in
theorem runtime_sound (sv dv a b : Bool) :
    allowed sv dv a b = true → identity sv a = true ∧ identity dv b = true := by
  cases sv <;> cases dv <;> cases a <;> cases b <;> decide

set_option maxHeartbeats 10000 in
theorem static_sound (sv dv ordinary a b : Bool)
    (hordinary : ordinary = true → a = false ∧ b = false)
    (hemit : eligible sv dv ordinary = true) : allowed sv dv a b = true := by
  cases sv <;> cases dv <;> cases ordinary <;> cases a <;> cases b <;>
    simp_all [eligible, allowed, identity]

-- One-sided provenance and lexical capture are deliberately broken alternatives.
def brokenSourceOnly (sv _dv a _b : Bool) : Bool := identity sv a
def brokenDestinationOnly (_sv dv _a b : Bool) : Bool := identity dv b
def brokenLexicalCapture (a b : Bool) : Bool := allowed true true a b

def sensitivity : Bool :=
  (brokenSourceOnly true true false true != allowed true true false true) &&
  (brokenDestinationOnly true true true false != allowed true true true false) &&
  (brokenLexicalCapture true true != allowed false false true true)

set_option maxHeartbeats 10000 in
example : sensitivity = true := by decide

set_option maxHeartbeats 10000 in
example : allowed true true false false = true ∧ eligible true true false = false := by decide

end HoiminOracle.PreparedAnnotationImport
