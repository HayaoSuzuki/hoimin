import Std

namespace DefaultOperatorSelection

-- Abstract membership model; no claim of verified Rust/serde execution.
abbrev Selection := Nat → Bool

def defaults (legacy promoted : Selection) : Selection := fun op => legacy op || promoted op

def select (legacy promoted excluded : Selection) (explicit : Option Selection) : Selection :=
  fun op => (explicit.getD (defaults legacy promoted) op) && !(excluded op)

def reload (saved : Selection) : Selection := saved

theorem excluded_wins (legacy promoted excluded : Selection) (explicit : Option Selection)
    (op : Nat) (h : excluded op = true) : select legacy promoted excluded explicit op = false := by
  simp [select, h]

theorem explicit_overrides (legacy promoted excluded chosen : Selection) (op : Nat) :
    select legacy promoted excluded (some chosen) op = (chosen op && !(excluded op)) := rfl

theorem default_promotes (legacy promoted excluded : Selection) (op : Nat)
    (hp : promoted op = true) (he : excluded op = false) :
    select legacy promoted excluded none op = true := by
  simp [select, defaults, hp, he]

theorem legacy_retained (legacy promoted excluded : Selection) (op : Nat)
    (hl : legacy op = true) (he : excluded op = false) :
    select legacy promoted excluded none op = true := by
  simp [select, defaults, hl, he]

theorem opt_out_recovers_legacy (legacy promoted : Selection)
    (disjoint : ∀ op, legacy op = true → promoted op = false) :
    select legacy promoted promoted none = legacy := by
  funext op
  have h := disjoint op
  cases hl : legacy op <;> cases hp : promoted op <;> simp_all [select, defaults]

theorem saved_selection_frozen (saved : Selection) : reload saved = saved := rfl

-- Fixed witnesses distinguish the intended rule from unioning explicit choices
-- with defaults, or applying exclusions before defaults.
example : (defaults (fun _ => true) (fun _ => false) 0 || (0 == 1)) ≠
    select (fun _ => true) (fun _ => false) (fun _ => false)
      (some (fun op => op == 1)) 0 := by decide
example : ((false && !true) || true) ≠
    select (fun _ => false) (fun _ => true) (fun _ => true) none 0 := by decide

end DefaultOperatorSelection
