namespace HoiminOracle.GitLifecycle

inductive Child | first | second
  deriving DecidableEq, BEq

inductive Failure
  | primary (child : Child)
  | kill (child : Child)
  | reap (child : Child) (code : Nat)
  | missingEvidence
  | deadline
  deriving BEq

structure Owner where
  active : Bool := false
  killFailed : Bool := false
  firstReap : Option Nat := none

structure State where
  owners : Child → Owner := fun _ => {}
  history : List Failure := []

inductive Event
  | start (child : Child) (primary killFailed : Bool)
  | failedWait (child : Child) (code : Nat)
  | reaped (child : Child)
  | abandoned (child : Child)

def setOwner (state : State) (child : Child) (owner : Owner) : State :=
  { state with owners := fun id => if id = child then owner else state.owners id }

def record (state : State) (failures : List Failure) : State :=
  { state with history := state.history ++ failures }

def flags (child : Child) (primary killFailed : Bool) : List Failure :=
  (if primary then [.primary child] else []) ++ (if killFailed then [.kill child] else [])

def step (state : State) : Event → State
  | .start child primary killFailed =>
    record (setOwner state child ⟨true, killFailed, none⟩) (flags child primary killFailed)
  | .failedWait child code =>
    let owner := state.owners child
    if owner.firstReap.isSome then state
    else record (setOwner state child { owner with firstReap := some code }) [.reap child code]
  | .reaped child =>
    setOwner state child { (state.owners child) with active := false }
  | .abandoned child =>
    record (setOwner state child { (state.owners child) with active := false }) [.missingEvidence]

def activeCount (state : State) : Nat :=
  (state.owners .first).active.toNat + (state.owners .second).active.toNat

def drain (state : State) : List Failure :=
  state.history ++ if activeCount state = 0 then [] else [.deadline]

def cleanup (state : State) (event : Event) : List Failure :=
  match event with
  | .reaped child =>
    let owner := state.owners child
    flags child false owner.killFailed ++ (owner.firstReap.map (Failure.reap child)).toList
  | _ => []

theorem failed_wait_preserves_ownership (state : State) (child : Child) (code : Nat) :
    activeCount (step state (.failedWait child code)) = activeCount state := by
  cases child <;> simp [step, activeCount, record, setOwner] <;> split <;> simp_all

theorem later_wait_error_keeps_history (state : State) (child : Child) (earlier : Nat)
    (seen : (state.owners child).firstReap = some earlier) (later : Nat) :
    step state (.failedWait child later) = state := by
  simp [step, seen]

theorem history_is_append_only (state : State) (event : Event) :
    ∃ added, (step state event).history = state.history ++ added := by
  cases event with
  | start child primary killFailed => exact ⟨flags child primary killFailed, rfl⟩
  | failedWait child code =>
    by_cases seen : (state.owners child).firstReap.isSome = true
    · exact ⟨[], by simp [step, seen]⟩
    · exact ⟨[.reap child code], by simp [step, seen, record, setOwner]⟩
  | reaped => exact ⟨[], by simp [step, setOwner]⟩
  | abandoned => exact ⟨[.missingEvidence], rfl⟩

theorem history_persists (initial : State) (events : List Event) :
    ∃ added, (events.foldl step initial).history = initial.history ++ added := by
  induction events generalizing initial with
  | nil => exact ⟨[], by simp⟩
  | cons event rest ih =>
    obtain ⟨first, initialStep⟩ := history_is_append_only initial event
    obtain ⟨later, subsequent⟩ := ih (step initial event)
    exact ⟨first ++ later, by simp only [List.foldl_cons, subsequent, initialStep, List.append_assoc]⟩

theorem drain_success_iff (state : State) :
    drain state = [] ↔ activeCount state = 0 ∧ state.history = [] := by
  unfold drain
  split <;> simp_all

end HoiminOracle.GitLifecycle
