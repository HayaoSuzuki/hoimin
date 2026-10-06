import Std
namespace OptionalKeywordDelete

def eraseAt (args : Nat → Option α) (key : Nat) : Nat → Option α :=
  fun i => if i = key then none else args i
def resolve (defaults : Nat → α) (args : Nat → Option α) (key : Nat) : α :=
  (args key).getD (defaults key)
def omitted (default : α) (s : S) : α × S := (default, s)
def eligible (hasDefault explicit positionalOnly : Bool) : Bool :=
  hasDefault && explicit && !positionalOnly

theorem erased_absent (args : Nat → Option α) (key : Nat) : eraseAt args key key = none := by
  simp [eraseAt]
theorem other_unchanged (args : Nat → Option α) (key i : Nat) (h : i ≠ key) :
    eraseAt args key i = args i := by simp [eraseAt, h]
theorem uses_existing_default (defaults : Nat → α) (args : Nat → Option α) (key : Nat) :
    resolve defaults (eraseAt args key) key = defaults key := by simp [resolve, eraseAt]
theorem skips_argument_effects (default : α) (s : S) : (omitted default s).2 = s := rfl
theorem required_excluded (explicit positionalOnly : Bool) : eligible false explicit positionalOnly = false := by
  simp [eligible]
theorem positional_only_excluded (hasDefault explicit : Bool) : eligible hasDefault explicit true = false := by
  simp [eligible]
example : resolve (fun _ => true) (eraseAt (fun _ => some false) 0) 0 = true := by decide
example : resolve (fun _ => true) (fun _ => some false) 0 ≠ true := by decide
end OptionalKeywordDelete
