import Std

namespace BuiltinAnnotation

inductive Fact | builtin | shadowed | unknown
  deriving Repr, BEq, DecidableEq

def eligible : Fact → Bool
  | .builtin => true
  | .shadowed | .unknown => false

-- Defect: a matching bare spelling is accepted without checking its binding.
def brokenEligible (_ : Fact) : Bool := true

theorem requires_builtin (fact : Fact) (h : eligible fact = true) :
    fact = .builtin := by cases fact <;> simp_all [eligible]

theorem shadowed_is_rejected : eligible .shadowed = false := rfl
theorem unknown_is_rejected : eligible .unknown = false := rfl

example : eligible .shadowed != brokenEligible .shadowed := by decide
example : eligible .unknown != brokenEligible .unknown := by decide

end BuiltinAnnotation
