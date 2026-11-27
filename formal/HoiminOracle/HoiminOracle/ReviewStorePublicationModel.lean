namespace HoiminOracle.ReviewStorePublication

inductive Worker | first | second
  deriving DecidableEq

inductive Project | first | second
  deriving DecidableEq

inductive Phase | idle | created | transactionFirst | transactionSecond | transactionThird | publish
  deriving DecidableEq

def Phase.next : Phase → Phase
  | .idle => .created
  | .created => .transactionFirst
  | .transactionFirst => .transactionSecond
  | .transactionSecond => .transactionThird
  | .transactionThird => .publish
  | .publish => .idle

inductive Published
  | absent
  | foreignFile
  | ready (owner : Project) (history : List Worker)
  deriving DecidableEq

structure State where
  published : Published := .absent
  first : Phase := .idle
  second : Phase := .idle
  deriving DecidableEq

def State.phase (state : State) : Worker → Phase
  | .first => state.first
  | .second => state.second

def State.setPhase (state : State) (phase : Phase) : Worker → State
  | .first => { state with first := phase }
  | .second => { state with second := phase }

@[simp] theorem State.set_phase_keeps_public (state : State) (phase : Phase) (worker : Worker) :
    (state.setPhase phase worker).published = state.published := by
  cases worker <;> rfl

def State.privateCount (state : State) : Nat :=
  (if state.first = .idle then 0 else 1) + (if state.second = .idle then 0 else 1)

inductive Action
  | advance (worker : Worker)
  | fail (worker : Worker) (unwind : Bool)
  | installForeign

inductive Reply
  | reached (phase : Phase)
  | opened (revision : Nat)
  | rejectedForeign
  | rejectedProject
  | failed
  | unwound
  | installedForeign
  | invalidSchedule
  deriving DecidableEq

def projectFor (shared : Bool) : Worker → Project
  | .first => .first
  | .second => if shared then .first else .second

def publish (project : Project) : Published → Published
  | .absent => .ready project []
  | existing => existing

def reopen (state : State) (worker : Worker) (project : Project) : State × Reply :=
  match state.published with
  | .absent => (state, .invalidSchedule)
  | .foreignFile => (state, .rejectedForeign)
  | .ready owner history =>
    if project = owner then
      ({ state with published := .ready owner (history ++ [worker]) }, .opened (history.length + 1))
    else (state, .rejectedProject)

def advance (state : State) (worker : Worker) (project : Project) : State × Reply :=
  match state.phase worker with
  | .idle =>
    if state.published = .absent then (state.setPhase .created worker, .reached .created)
    else reopen state worker project
  | .publish =>
    reopen { state.setPhase .idle worker with published := publish project state.published } worker project
  | phase => (state.setPhase phase.next worker, .reached phase.next)

def step (shared : Bool) (state : State) : Action → State × Reply
  | .advance worker => advance state worker (projectFor shared worker)
  | .fail worker unwind =>
    if state.phase worker = .idle then (state, .invalidSchedule)
    else (state.setPhase .idle worker, if unwind then .unwound else .failed)
  | .installForeign =>
    if state.published = .absent then ({ state with published := .foreignFile }, .installedForeign)
    else (state, .invalidSchedule)

structure Observation where
  reply : Reply
  published : Published
  privateCount : Nat
  deriving DecidableEq

def replayWith (attempt : State → Action → State × Reply) (state : State) :
    List Action → List Observation
  | [] => []
  | action :: rest =>
    let (after, reply) := attempt state action
    ⟨reply, after.published, after.privateCount⟩ :: replayWith attempt after rest

theorem publication_never_replaces_existing_file (project : Project) (existing : Published)
    (present : existing ≠ .absent) : publish project existing = existing := by
  cases existing <;> simp_all [publish]

private theorem reopen_never_reaches (state : State) (worker : Worker)
    (project : Project) (phase : Phase) : (reopen state worker project).2 ≠ .reached phase := by
  cases found : state.published with
  | absent | foreignFile => simp [reopen, found]
  | ready owner history => by_cases same : project = owner <;> simp [reopen, found, same]

theorem entering_private_phase_preserves_public_file (state : State) (worker : Worker) (project : Project)
    (phase : Phase) (entering : (advance state worker project).2 = .reached phase) :
    (advance state worker project).1.published = state.published := by
  cases phaseAt : state.phase worker with
  | idle =>
    by_cases absent : state.published = .absent
    · simp [advance, phaseAt, absent]
    · exact (reopen_never_reaches state worker project phase (by
        simpa [advance, phaseAt, absent] using entering)).elim
  | publish =>
    exact (reopen_never_reaches
      { state.setPhase .idle worker with published := publish project state.published }
      worker project phase (by simpa [advance, phaseAt] using entering)).elim
  | created | transactionFirst | transactionSecond | transactionThird =>
    simp [advance, phaseAt]

theorem failure_preserves_public_file (shared : Bool) (state : State) (worker : Worker) (unwind : Bool) :
    (step shared state (.fail worker unwind)).1.published = state.published := by
  by_cases idle : state.phase worker = .idle <;> simp [step, idle]

theorem admitted_winner_history_is_extended (state : State) (worker : Worker) (project : Project)
    (history : List Worker) (winner : state.published = .ready project history) :
    (reopen state worker project).1.published = .ready project (history ++ [worker]) := by
  simp [reopen, winner]

end HoiminOracle.ReviewStorePublication
