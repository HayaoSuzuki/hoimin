namespace HoiminOracle.ReviewRecord

inductive Candidate | first | second
  deriving DecidableEq

inductive Source | fresh | stale | unavailable
  deriving DecidableEq

inductive Mode | record | rollback | busy | failWrite | failNewline
  deriving DecidableEq

structure Request where
  candidate : Candidate
  expected : Nat
  marker : Nat
  mode : Mode := .record
  source : Source := .fresh

structure Event where
  revision : Nat
  marker : Nat
  deriving DecidableEq

structure Store where
  present : Bool := false
  first : List Event := []
  second : List Event := []
  deriving DecidableEq

def Store.history (store : Store) : Candidate → List Event
  | .first => store.first
  | .second => store.second

def Store.revision (store : Store) (candidate : Candidate) : Nat :=
  ((store.history candidate).getLast?.map Event.revision).getD 0

def Store.commit (store : Store) (candidate : Candidate) (event : Event) : Store :=
  match candidate with
  | .first => { store with present := true, first := store.first ++ [event] }
  | .second => { store with present := true, second := store.second ++ [event] }

inductive Reply
  | recorded (revision : Nat)
  | savedOutput (revision : Nat)
  | notCurrent
  | revisionConflict
  | rolledBack
  | busy
  deriving DecidableEq

inductive Output | empty | document | line
  deriving DecidableEq

structure Observation where
  store : Store
  reply : Reply
  output : Output
  deriving DecidableEq

def step (store : Store) (request : Request) : Observation :=
  if request.source ≠ .fresh then ⟨store, .notCurrent, .empty⟩
  else
    let prepared := { store with present := true }
    if request.mode = .busy then ⟨prepared, .busy, .empty⟩
    else if store.revision request.candidate ≠ request.expected then
      ⟨prepared, .revisionConflict, .empty⟩
    else if request.mode = .rollback then ⟨prepared, .rolledBack, .empty⟩
    else
      let revision := request.expected + 1
      let committed := store.commit request.candidate ⟨revision, request.marker⟩
      match request.mode with
      | .failWrite => ⟨committed, .savedOutput revision, .empty⟩
      | .failNewline => ⟨committed, .savedOutput revision, .document⟩
      | _ => ⟨committed, .recorded revision, .line⟩

def replayWith (attempt : Store → Request → Observation) (store : Store) : List Request → List Observation
  | [] => []
  | request :: rest =>
    let observed := attempt store request
    observed :: replayWith attempt observed.store rest

def replay := replayWith step

theorem rollback_keeps_every_history (store : Store) (request : Request) (candidate : Candidate) :
    (step store { request with mode := .rollback }).store.history candidate = store.history candidate := by
  cases candidate <;> by_cases fresh : request.source = .fresh <;> by_cases same : store.revision request.candidate = request.expected <;>
    simp [step, fresh, same, Store.history]

theorem output_failure_keeps_committed_event (store : Store) (request : Request)
    (fresh : request.source = .fresh) (same : store.revision request.candidate = request.expected) :
    (step store { request with mode := .failWrite }).store = (step store { request with mode := .record }).store ∧
      (step store { request with mode := .failNewline }).store = (step store { request with mode := .record }).store := by
  simp [step, fresh, same]

theorem retry_after_output_failure_conflicts (store : Store) (request : Request)
    (fresh : request.source = .fresh) (same : store.revision request.candidate = request.expected)
    (failure : request.mode = .failWrite ∨ request.mode = .failNewline) :
    let saved := (step store request).store
    step saved { request with mode := .record } = ⟨saved, .revisionConflict, .empty⟩ := by
  rcases failure with failed | failed <;>
    simp [step, fresh, same, failed, Store.commit] <;>
    cases request.candidate <;> simp [Store.revision, Store.history]

theorem commit_preserves_other_candidate (store : Store) (candidate other : Candidate) (event : Event)
    (separate : candidate ≠ other) :
    (store.commit candidate event).history other = store.history other := by
  cases candidate <;> cases other <;> simp_all [Store.commit, Store.history]

end HoiminOracle.ReviewRecord
