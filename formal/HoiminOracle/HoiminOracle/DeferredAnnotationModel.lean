import Std

namespace AnnotationAudit

inductive Event where
  | rebind | restore | observe
  deriving DecidableEq, BEq, Repr

structure State where
  current : Bool
  cached : Option Bool
  deriving DecidableEq, BEq, Repr

def initial : State := ⟨true, none⟩
def value (s : State) : Bool := s.cached.getD s.current
def step (s : State) : Event → State
  | .rebind => { s with current := false }
  | .restore => { s with current := true }
  | .observe => { s with cached := some (value s) }
def run : State → List Event → State
  | s, [] => s
  | s, e :: es => run (step s e) es

theorem step_cached (s : State) (e : Event) (v : Bool)
    (h : s.cached = some v) : (step s e).cached = some v := by
  cases e <;> simp [step, value, h]

theorem run_cached (s : State) (es : List Event) (v : Bool)
    (h : s.cached = some v) : (run s es).cached = some v := by
  induction es generalizing s with
  | nil => exact h
  | cons e es ih => exact ih (step s e) (step_cached s e v h)

theorem first_observation_freezes (s : State) (es : List Event) :
    value (run s (.observe :: es)) = value s := by
  have h := run_cached (step s .observe) es (value s) rfl
  simp [run, value, h]

theorem rebind_before_observation (es : List Event) :
    value (run initial (.rebind :: .observe :: es)) = false := by
  exact first_observation_freezes (step initial .rebind) es

theorem observe_before_rebind (es : List Event) :
    value (run initial (.observe :: .rebind :: es)) = true := by
  exact first_observation_freezes initial (.rebind :: es)

def brokenSnapshot (_events : List Event) : Bool := true

-- Broken evaluation ignores the first-observation cache.
def brokenCurrentOnly (s : State) : Bool := s.current

inductive Provider where
  | typing | abc
  deriving DecidableEq, BEq, Repr
inductive Member where
  | abstractSet | set
  deriving DecidableEq, BEq, Repr
def existsIn : Provider → Member → Bool
  | .typing, _ => true
  | .abc, .set => true
  | _, _ => false
def destination : Provider → Member
  | .typing => .abstractSet
  | .abc => .set

theorem destination_exists (p : Provider) : existsIn p (destination p) = true := by
  cases p <;> rfl

theorem abc_rejects_typing_spelling : existsIn .abc .abstractSet = false := by rfl

end AnnotationAudit
