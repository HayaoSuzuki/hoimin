import Std

namespace DeclarationAudit

inductive Scope where
  | module | classBody | function
  deriving DecidableEq, BEq, Repr

inductive Value where
  | missing | known | other
  deriving DecidableEq, BEq, Repr

-- The annotated name is lexically local in function scope, even without a value.
def resolvesKnown (scope : Scope) (fallback : Bool) : Value → Bool
  | .known => true
  | .other => false
  | .missing => if scope == .function then false else fallback

-- The modeled RHS, when present, evaluates to an unrelated user callable.
def annotate (hasValue : Bool) (value : Value) : Value :=
  if hasValue then .other else value

def brokenAlwaysStore (_hasValue : Bool) (_value : Value) : Value := .other
def brokenIgnoreRhs (_hasValue : Bool) (value : Value) : Value := value
def brokenFallbackInFunction (scope : Scope) (fallback : Bool) (value : Value) : Bool :=
  if value == .missing then fallback else resolvesKnown scope fallback value

def repeatDeclaration : Nat → Value → Value
  | 0, value => value
  | n + 1, value => repeatDeclaration n (annotate false value)

theorem declaration_preserves_value (value : Value) : annotate false value = value := by rfl

theorem declaration_preserves_resolution (scope : Scope) (fallback : Bool) (value : Value) :
    resolvesKnown scope fallback (annotate false value) = resolvesKnown scope fallback value := by rfl

theorem repeated_declarations_preserve (n : Nat) (value : Value) :
    repeatDeclaration n value = value := by
  induction n with
  | zero => rfl
  | succ n ih => simpa [repeatDeclaration, annotate] using ih

theorem function_missing_does_not_fall_back (fallback : Bool) :
    resolvesKnown .function fallback (annotate false .missing) = false := by rfl

theorem rhs_rebinding_rejects (scope : Scope) (fallback : Bool) (value : Value) :
    resolvesKnown scope fallback (annotate true value) = false := by rfl

end DeclarationAudit
