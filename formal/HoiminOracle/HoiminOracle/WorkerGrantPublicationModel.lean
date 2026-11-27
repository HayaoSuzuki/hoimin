namespace HoiminOracle.WorkerGrantPublication

inductive Grant where
  | a | b | wrongPreflight | wrongAllowance
  deriving DecidableEq

structure Request where
  grant : Grant
  worker : Nat
  fail : Bool := false
  deriving DecidableEq

inductive Slot where
  | vacant | copying (fail : Bool) | ready | retained
  deriving DecidableEq

inductive Binding where
  | unbound
  | held (leader : Request) (queued : Option Request)
  | published (grant : Grant)
  deriving DecidableEq

structure State where
  binding : Binding := .unbound
  first : Slot := .vacant
  second : Slot := .vacant
  deriving DecidableEq

def State.slot (state : State) : Nat → Slot
  | 0 => state.first
  | 1 => state.second
  | _ => .vacant

def State.update (state : State) (worker : Nat) (slot : Slot) : State :=
  match worker with
  | 0 => { state with first := slot }
  | 1 => { state with second := slot }
  | _ => state

inductive Action where
  | start (request : Request)
  | queue (request : Request)
  | publish
  | finish (worker : Nat)
  | release (worker : Nat)
  | retain (worker : Nat)
  deriving DecidableEq

inductive Verdict where
  | holdingLock | queued | published | copying | ready | copyFailed | released | retained
  | preflightMismatch | outOfRange | allowanceMismatch | reservationMismatch | duplicate | disabled
  deriving DecidableEq

structure Result where
  state : State
  verdict : Verdict
  queued : Option Verdict := none
  deriving DecidableEq

def baseError (request : Request) : Option Verdict :=
  if request.grant = .wrongPreflight then some .preflightMismatch
  else if 2 ≤ request.worker then some .outOfRange
  else if request.grant = .wrongAllowance then some .allowanceMismatch
  else none

private def startBound (state : State) (reservation : Grant) (request : Request) : Result :=
  match baseError request with
  | some failure => ⟨state, failure, none⟩
  | none =>
    if request.grant ≠ reservation then ⟨state, .reservationMismatch, none⟩
    else if state.slot request.worker ≠ .vacant then ⟨state, .duplicate, none⟩
    else ⟨state.update request.worker (.copying request.fail), .copying, none⟩

def step (state : State) : Action → Result
  | .start request => match state.binding with
    | .unbound => match baseError request with
      | some failure => ⟨state, failure, none⟩
      | none => ⟨{ state with binding := .held request none }, .holdingLock, none⟩
    | .held .. => ⟨state, .disabled, none⟩
    | .published grant => startBound state grant request
  | .queue request => match state.binding with
    | .held leader none => ⟨{ state with binding := .held leader (some request) }, .queued, none⟩
    | _ => ⟨state, .disabled, none⟩
  | .publish => match state.binding with
    | .held leader queued =>
      let published := { state with binding := .published leader.grant }
      let leading := published.update leader.worker (.copying leader.fail)
      match queued with
      | none => ⟨leading, .published, none⟩
      | some request =>
        let result := startBound leading leader.grant request
        ⟨result.state, .published, some result.verdict⟩
    | _ => ⟨state, .disabled, none⟩
  | .finish worker => match state.slot worker with
    | .copying false => ⟨state.update worker .ready, .ready, none⟩
    | .copying true => ⟨state.update worker .vacant, .copyFailed, none⟩
    | _ => ⟨state, .disabled, none⟩
  | .release worker =>
    if state.slot worker = .ready then ⟨state.update worker .vacant, .released, none⟩
    else ⟨state, .disabled, none⟩
  | .retain worker =>
    if state.slot worker = .ready then ⟨state.update worker .retained, .retained, none⟩
    else ⟨state, .disabled, none⟩

def runWith (transition : State → Action → Result) (state : State) : List Action → List Result
  | [] => []
  | action :: rest =>
    let result := transition state action
    result :: runWith transition result.state rest

def Valid (state : State) : Prop := match state.binding with
  | .published _ => True
  | _ => state.first = .vacant ∧ state.second = .vacant

private theorem update_binding (state : State) (worker : Nat) (slot : Slot) :
    (state.update worker slot).binding = state.binding := by
  rcases worker with _ | _ | worker <;> rfl

private theorem startBound_binding (state : State) (grant : Grant) (request : Request) :
    (startBound state grant request).state.binding = state.binding := by
  cases failure : baseError request with
  | some error => simp [startBound, failure]
  | none =>
    simp only [startBound, failure]
    split
    · rfl
    · split <;> simp [update_binding]

theorem published_grant_is_immutable (state : State) (grant : Grant) (action : Action)
    (published : state.binding = .published grant) : (step state action).state.binding = .published grant := by
  cases action with
  | start _ => simp [step, published, startBound_binding]
  | queue _ | publish => simp [step, published]
  | finish _ => simp only [step]; split <;> simp_all [update_binding]
  | release _ | retain _ => simp only [step]; split <;> simp_all [update_binding]

theorem step_preserves_visibility (state : State) (action : Action) (valid : Valid state) :
    Valid (step state action).state := by
  cases binding : state.binding with
  | published grant => simp [Valid, published_grant_is_immutable state grant action binding]
  | unbound =>
    obtain ⟨first, second⟩ := show state.first = .vacant ∧ state.second = .vacant by simpa [Valid, binding] using valid
    cases action with
    | start _ => simp only [step, binding]; split <;> simp_all [Valid]
    | queue _ | publish => simpa [step, binding] using valid
    | finish worker | release worker | retain worker =>
      rcases worker with _ | _ | worker <;> simpa [step, State.slot, first, second] using valid
  | held leader queued =>
    obtain ⟨first, second⟩ := show state.first = .vacant ∧ state.second = .vacant by simpa [Valid, binding] using valid
    cases action with
    | start _ => simpa [step, binding] using valid
    | queue _ => cases queued <;> simp_all [step, Valid]
    | publish => cases queued <;> simp [step, binding, Valid, startBound_binding, update_binding]
    | finish worker | release worker | retain worker =>
      rcases worker with _ | _ | worker <;> simpa [step, State.slot, first, second] using valid

theorem history_preserves_visibility (state : State) (history : List Action) (valid : Valid state) :
    ∀ result ∈ runWith step state history, Valid result.state := by
  induction history generalizing state with
  | nil => simp [runWith]
  | cons action rest ih =>
    intro result member
    simp [runWith] at member
    have next := step_preserves_visibility state action valid
    rcases member with rfl | later
    · exact next
    · exact ih (step state action).state next result later

def State.occupied (state : State) : List Nat := [0, 1].filter (fun worker => decide (state.slot worker ≠ .vacant))

def State.charged (state : State) (perWorker : Nat) : Nat := state.occupied.length * perWorker

def State.allowance (state : State) (perWorker : Nat) : Nat := match state.binding with
  | .published _ => 2 * perWorker
  | _ => 0

theorem charge_fits_published_allowance (state : State) (perWorker : Nat) (valid : Valid state) :
    state.charged perWorker ≤ state.allowance perWorker := by
  cases binding : state.binding
  case published =>
    have atMostTwo := List.length_filter_le (p := fun worker => decide (state.slot worker ≠ .vacant)) (l := [0, 1])
    simpa [State.charged, State.occupied, State.allowance, binding] using (Nat.mul_le_mul_right perWorker atMostTwo)
  all_goals
    have ⟨first, second⟩ := show state.first = .vacant ∧ state.second = .vacant by simpa [Valid, binding] using valid
    simp [State.charged, State.occupied, State.slot, first, second, State.allowance, binding]

theorem failed_copy_releases_first_only (grant : Grant) (sibling : Slot) :
    step ⟨.published grant, .copying true, sibling⟩ (.finish 0) =
      ⟨⟨.published grant, .vacant, sibling⟩, .copyFailed, none⟩ := rfl

theorem failed_copy_releases_second_only (grant : Grant) (sibling : Slot) :
    step ⟨.published grant, sibling, .copying true⟩ (.finish 1) =
      ⟨⟨.published grant, sibling, .vacant⟩, .copyFailed, none⟩ := rfl

theorem retaining_managed_root_keeps_charge (grant : Grant) (sibling : Slot) (perWorker : Nat) :
    (step ⟨.published grant, .ready, sibling⟩ (.retain 0)).state.charged perWorker =
      State.charged ⟨.published grant, .ready, sibling⟩ perWorker := by
  cases sibling <;> rfl

end HoiminOracle.WorkerGrantPublication
