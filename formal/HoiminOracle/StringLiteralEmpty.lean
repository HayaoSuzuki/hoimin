import Std
namespace StringLiteralEmpty

def erase (_value : String) : String := ""
def eligible (single nonempty runtime : Bool) : Bool := single && nonempty && runtime

theorem result_empty (value : String) : erase value = "" := rfl
theorem changed_nonempty (value : String) (h : value ≠ "") : erase value ≠ value := by
  intro same
  exact h same.symm

theorem empty_excluded (single runtime : Bool) : eligible single false runtime = false := by
  simp [eligible]
theorem nonruntime_excluded (single nonempty : Bool) : eligible single nonempty false = false := by
  simp [eligible]
theorem concatenated_excluded (nonempty runtime : Bool) : eligible false nonempty runtime = false := by
  simp [eligible]

example : erase "ready" = "" := rfl
example : eligible true true true = true := by decide
-- A broken identity edit retains the content that should disappear.
example : "ready" ≠ erase "ready" := by decide
end StringLiteralEmpty
