import Std
namespace AugmentedToAssignment

def update (old amount : Int) : Int := old + amount
def replacement (_old amount : Int) : Int := amount

theorem replacement_forgets_old (a b amount : Int) :
    replacement a amount = replacement b amount := rfl

theorem one_update_from_zero (amount : Int) :
    update 0 amount = replacement 0 amount := by simp [update, replacement]

theorem two_updates (first second : Int) :
    update (update 0 first) second = first + second := by simp [update]

theorem replaced_two_updates (first second : Int) :
    replacement (replacement 0 first) second = second := rfl

theorem loses_nonzero_first_update (first second : Int) (h : first ≠ 0) :
    update (update 0 first) second ≠ replacement (replacement 0 first) second := by
  simp only [update, replacement, Int.zero_add]
  omega

example : update (update 0 3) 5 = 8 := by decide
example : replacement (replacement 0 3) 5 = 5 := by decide
example : update (update 0 3) 5 ≠ replacement (replacement 0 3) 5 := by decide
end AugmentedToAssignment
