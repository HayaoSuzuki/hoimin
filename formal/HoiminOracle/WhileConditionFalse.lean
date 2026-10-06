import Std
namespace WhileConditionFalse
-- One loop entry; the false branch terminates and runs else.
def enter (test : S → Bool × S) (body otherwise : S → S) (s : S) : S :=
  let (ok, next) := test s
  if ok then body next else otherwise next
def forcedFalse (s : S) : Bool × S := (false, s)
theorem else_runs (body otherwise : S → S) (s : S) :
    enter forcedFalse body otherwise s = otherwise s := rfl
theorem no_body_effect (a b otherwise : S → S) (s : S) :
    enter forcedFalse a otherwise s = enter forcedFalse b otherwise s := rfl
theorem state_preserved (s : S) : forcedFalse s = (false, s) := rfl
theorem without_else (body : S → S) (s : S) : enter forcedFalse body id s = s := rfl
-- Broken replacement that still evaluates the condition adds an unwanted effect.
example : enter (fun n : Nat => (false, n + 1)) id id 0 ≠ enter forcedFalse id id 0 := by decide
example : enter forcedFalse (fun n : Nat => n + 10) (fun n => n + 2) 0 = 2 := rfl
end WhileConditionFalse
