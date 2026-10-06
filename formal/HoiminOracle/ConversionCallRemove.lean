import Std
namespace ConversionCallRemove
abbrev Action (S α : Type) := S → α × S
def original (argument : Action S α) (convert : α → Action S β) (s : S) : β × S :=
  let (value, next) := argument s
  convert value next
def removed (argument : Action S α) (s : S) : α × S := argument s
theorem argument_once (argument : Action S α) (s : S) : removed argument s = argument s := rfl
theorem value_preserved (argument : Action S α) (s : S) :
    (removed argument s).1 = (argument s).1 := rfl
theorem state_preserved (argument : Action S α) (s : S) :
    (removed argument s).2 = (argument s).2 := rfl
theorem identity_conversion (argument : Action S α) (s : S) :
    original argument (fun a s => (a,s)) s = removed argument s := by
  simp [original, removed]
-- Argument adds one event; conversion adds another. Removed call retains only one.
example : removed (fun n : Nat => ("7", n+1)) 0 = ("7",1) := rfl
example : original (fun n : Nat => ("7", n+1)) (fun _ n => (7,n+1)) 0 = (7,2) := rfl
example : (removed (fun n : Nat => ("7", n+1)) 0).2 ≠ 2 := by decide
end ConversionCallRemove
