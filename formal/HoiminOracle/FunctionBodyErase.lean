import Std
namespace FunctionBodyErase

def eligible (async special valued suspension noop : Bool) : Bool :=
  !(async || special || valued || suspension || noop)

theorem suspension_excluded (a s v n : Bool) : eligible a s v true n = false := by
  simp [eligible]
theorem value_return_excluded (a s y n : Bool) : eligible a s true y n = false := by
  simp [eligible]
theorem noops_excluded (a s v y : Bool) : eligible a s v y true = false := by
  simp [eligible]

structure FunctionSource where
  headerAndDocstring : List Nat
  body : List Nat
  suffix : List Nat

def erase (source : FunctionSource) : FunctionSource := { source with body := [0] }
theorem preserves_header (source : FunctionSource) :
    (erase source).headerAndDocstring = source.headerAndDocstring := rfl
theorem preserves_suffix (source : FunctionSource) :
    (erase source).suffix = source.suffix := rfl

-- Lexical scope: only immediately evaluated nested headers contribute here.
def nestedSuspension (header _body : Bool) : Bool := header
example : nestedSuspension false true = false := by decide
example : nestedSuspension true false = true := by decide
example : eligible false false false false false = true := by decide
-- Broken blanket skip of nested definitions misses suspension in defaults.
example : false ≠ nestedSuspension true false := by decide
end FunctionBodyErase
