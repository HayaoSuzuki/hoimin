namespace HoiminOracle.GitAdmission

structure State where
  deadline : Option Nat
  now : Nat := 0
  requestStopped : Bool := false
  lifecycleStopped : Bool := false

inductive Event
  | stopRequest
  | stopLifecycle
  | advance (ticks : Nat)
  | trySpawn

def step (state : State) : Event → State
  | .stopRequest => { state with requestStopped := true }
  | .stopLifecycle => { state with lifecycleStopped := true }
  | .advance ticks => { state with now := state.now + ticks }
  | .trySpawn => state

def AdmissionAllowed (state : State) : Prop :=
  state.lifecycleStopped = false ∧
    (match state.deadline with
    | none => True
    | some deadline => state.requestStopped = false ∧ state.now < deadline)

def allowed (state : State) : Bool :=
  !state.lifecycleStopped && state.deadline.all fun deadline =>
    !state.requestStopped && state.now < deadline

theorem allowed_iff (state : State) : allowed state = true ↔ AdmissionAllowed state := by
  cases configured : state.deadline <;> simp [allowed, AdmissionAllowed, configured]

theorem denial_persists (state : State) (event : Event) (denied : ¬ AdmissionAllowed state) :
    ¬ AdmissionAllowed (step state event) := by
  cases event with
  | stopRequest => cases configured : state.deadline <;> simp_all [AdmissionAllowed, step]
  | stopLifecycle => simp [AdmissionAllowed, step]
  | trySpawn => exact denied
  | advance ticks =>
    intro ⟨live, within⟩
    apply denied
    refine ⟨live, ?_⟩
    cases configured : state.deadline with
    | none => exact True.intro
    | some deadline =>
      simp [step, configured] at within
      exact ⟨within.1, by have bound := within.2; omega⟩

theorem denial_persists_through_events (initial : State) (events : List Event)
    (denied : ¬ AdmissionAllowed initial) :
    ¬ AdmissionAllowed (events.foldl step initial) := by
  induction events generalizing initial with
  | nil => exact denied
  | cons event rest ih => exact ih (step initial event) (denial_persists initial event denied)

theorem deadline_is_exclusive (state : State) (deadline : Nat)
    (configured : state.deadline = some deadline) (expired : deadline ≤ state.now) :
    allowed state = false := by
  simp [allowed, configured, Nat.not_lt_of_ge expired]

end HoiminOracle.GitAdmission
