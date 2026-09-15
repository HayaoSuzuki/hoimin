import Std

namespace CollectionAnnotation

inductive Provenance where
  | builtin | shadowed | unknown
  deriving DecidableEq, Repr

def allowed (p : Provenance) : Bool := p == .builtin

theorem allowed_iff_builtin (p : Provenance) : allowed p = true ↔ p = .builtin := by
  cases p <;> simp [allowed]

-- A spelling-only gate is detected for both non-builtin states.
def brokenAllowed (_ : Provenance) : Bool := true
example : allowed .shadowed = false ∧ brokenAllowed .shadowed = true := by decide
example : allowed .unknown = false ∧ brokenAllowed .unknown = true := by decide

-- Promoted from the archived BuiltinAnnotationModel: only proven builtin
-- provenance is eligible. Python name resolution remains an explicit premise.
theorem requires_builtin (p : Provenance) (h : allowed p = true) :
    p = .builtin := (allowed_iff_builtin p).mp h

theorem shadowed_is_rejected : allowed .shadowed = false := by decide
theorem unknown_is_rejected : allowed .unknown = false := by decide

end CollectionAnnotation
