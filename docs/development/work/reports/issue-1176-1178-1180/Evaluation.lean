import Std

/- Model-only evaluation claims. These do not verify Python, hoimin or an OS.
   A finite observed corpus is a domain; detection says nothing about inputs
   outside it. Strengthening means logical implication, not an arbitrary edit. -/
namespace Evaluation

def Killed {Input Output : Type} (domain : Input → Prop)
    (accept : Input → Output → Prop) (mutant : Input → Output) : Prop :=
  ∃ input, domain input ∧ ¬ accept input (mutant input)

theorem expand_inputs {I O : Type} (small large : I → Prop)
    (accept : I → O → Prop) (mutant : I → O)
    (included : ∀ x, small x → large x)
    (killed : Killed small accept mutant) : Killed large accept mutant := by
  obtain ⟨input, inSmall, rejected⟩ := killed
  exact ⟨input, included input inSmall, rejected⟩

theorem strengthen_property {I O : Type} (domain : I → Prop)
    (weak strong : I → O → Prop) (mutant : I → O)
    (stronger : ∀ x y, strong x y → weak x y)
    (killed : Killed domain weak mutant) : Killed domain strong mutant := by
  obtain ⟨input, present, rejected⟩ := killed
  exact ⟨input, present, fun accepted => rejected (stronger input _ accepted)⟩

/- Interaction witness: old inputs contain only false; the weak property is
   output=false OR input=true. The mutant always returns false; the full property
   requires output=input. Only BOTH broader inputs and stronger property kill it.
   Thus "input shortage OR property shortage" is not an exclusive classification. -/
def narrow (input : Bool) : Prop := input = false
def broad (_ : Bool) : Prop := True
def weak (input output : Bool) : Prop := output = false ∨ input = true
def strong (input output : Bool) : Prop := output = input
def constantFalse (_ : Bool) : Bool := false

theorem strong_implies_weak (input output : Bool) : strong input output → weak input output := by
  cases input <;> cases output <;> simp [strong, weak]

theorem interaction :
    ¬ Killed narrow weak constantFalse ∧
    ¬ Killed broad weak constantFalse ∧
    ¬ Killed narrow strong constantFalse ∧
    Killed broad strong constantFalse := by
  simp [Killed, narrow, broad, weak, strong, constantFalse]

structure Cache where
  stored : Nat
  cached : Option Nat

def read (state : Cache) : Nat := state.cached.getD state.stored
def correctWrite (state : Cache) (value : Nat) : Cache := { state with stored := value, cached := none }
def staleWrite (state : Cache) (value : Nat) : Cache := { state with stored := value }

theorem fresh_reads_agree (old value : Nat) :
    read (correctWrite ⟨old, none⟩ value) = read (staleWrite ⟨old, none⟩ value) := by
  rfl

theorem warm_update_detects_stale (old value : Nat) (different : value ≠ old) :
    read (correctWrite ⟨old, some old⟩ value) ≠ read (staleWrite ⟨old, some old⟩ value) := by
  exact different

def completes (attempts failures : Nat) : Prop := failures < attempts

theorem normal_does_not_distinguish : completes 3 0 ∧ completes 2 0 := by simp [completes]
theorem retry_schedule_distinguishes : completes 3 2 ∧ ¬ completes 2 2 := by simp [completes]

#print axioms expand_inputs
#print axioms strengthen_property
#print axioms strong_implies_weak
#print axioms interaction
#print axioms fresh_reads_agree
#print axioms warm_update_detects_stale
#print axioms normal_does_not_distinguish
#print axioms retry_schedule_distinguishes

end Evaluation
