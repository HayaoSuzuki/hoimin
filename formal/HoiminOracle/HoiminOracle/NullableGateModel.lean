import Std
namespace NullableGate

inductive Tree where
  | atom (supported : Bool)
  | one (child : Tree)
  | pair (left right : Tree)
  deriving DecidableEq, BEq, Repr

def clean : Tree → Bool
  | .atom supported => supported
  | .one child => clean child
  | .pair left right => clean left && clean right

def eligible (trusted : Bool) (tree : Tree) : Bool := trusted && clean tree

def brokenTuple : Tree → Bool
  | .atom supported => supported
  | .one child => brokenTuple child
  | .pair _ _ => true

def brokenName (_trusted : Bool) (tree : Tree) : Bool := clean tree

inductive ContainsBlocked : Tree → Prop where
  | atom : ContainsBlocked (.atom false)
  | one {child} : ContainsBlocked child → ContainsBlocked (.one child)
  | left {l r} : ContainsBlocked l → ContainsBlocked (.pair l r)
  | right {l r} : ContainsBlocked r → ContainsBlocked (.pair l r)

theorem blocked_descendant_rejects {tree : Tree} (blocked : ContainsBlocked tree) :
    clean tree = false := by
  induction blocked with
  | atom => rfl
  | one _ ih => exact ih
  | left _ ih => simp [clean, ih]
  | right _ ih => simp [clean, ih]

theorem eligible_requires_trusted (trusted : Bool) (tree : Tree)
    (accepted : eligible trusted tree = true) : trusted = true := by
  simpa [eligible] using (Bool.and_eq_true_iff.mp accepted).1

theorem blocked_descendant_never_eligible (trusted : Bool) {tree : Tree}
    (blocked : ContainsBlocked tree) : eligible trusted tree = false := by
  simp [eligible, blocked_descendant_rejects blocked]

theorem safe_pair_preserved : eligible true (.pair (.atom true) (.atom true)) = true := by rfl

end NullableGate
