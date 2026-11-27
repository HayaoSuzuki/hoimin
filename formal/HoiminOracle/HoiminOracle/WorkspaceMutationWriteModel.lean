import HoiminOracle.WorkspaceMutationModel

namespace HoiminOracle.WorkspaceMutationWrite

open HoiminOracle.WorkspaceMutation (Bytes File splice)

structure Mutation where
  start : Nat
  length : Nat
  replacement : Bytes

structure Worker where
  generation : Nat
  file : File
  deriving DecidableEq

structure State where
  worker : Option Worker
  next : Nat
  deriving DecidableEq

inductive Action where
  | apply (limit : Option Nat)
  | reset (limit : Option Nat)
  | create
  deriving DecidableEq

inductive Outcome where
  | applied | writeFailed | hashMismatch | reset | restoreFailed | created | missing
  deriving DecidableEq

structure Result where
  state : State
  outcome : Outcome
  deriving DecidableEq

def originalFile (snapshot : Bytes) : File := ⟨snapshot, false⟩

def initial (snapshot : Bytes) (readOnly : Bool) : State := ⟨some ⟨0, ⟨snapshot, readOnly⟩⟩, 1⟩

def write (payload : Bytes) (limit : Option Nat) : Bytes :=
  payload.take (limit.getD payload.length)

def applyWith (matchesSource : Bytes → Bytes → Bool) (snapshot : Bytes) (mutation : Mutation)
    (state : State) (limit : Option Nat) : Result :=
  match state.worker with
  | none => ⟨state, .missing⟩
  | some worker =>
    if !matchesSource worker.file.bytes snapshot then ⟨state, .hashMismatch⟩
    else
      let payload := splice worker.file.bytes mutation.start mutation.length mutation.replacement
      let written := write payload limit
      let kind := if written.length = payload.length then .applied else .writeFailed
      ⟨{ state with worker := some { worker with file := ⟨written, false⟩ } }, kind⟩

def apply := applyWith (fun actual expected => decide (actual = expected))

def reset (snapshot : Bytes) (state : State) (limit : Option Nat) : Result :=
  match state.worker with
  | none => ⟨state, .missing⟩
  | some worker =>
    if worker.file = originalFile snapshot then ⟨state, .reset⟩
    else if snapshot.length ≤ limit.getD snapshot.length then
      ⟨{ state with worker := some { worker with file := originalFile snapshot } }, .reset⟩
    else ⟨{ state with worker := none }, .restoreFailed⟩

def step (snapshot : Bytes) (mutation : Mutation) (state : State) : Action → Result
  | .apply limit => apply snapshot mutation state limit
  | .reset limit => reset snapshot state limit
  | .create =>
    if state.worker.isSome then ⟨state, .missing⟩
    else ⟨⟨some ⟨state.next, originalFile snapshot⟩, state.next + 1⟩, .created⟩

def history (transition : State → Action → Result) (state : State) : List Action → List Result
  | [] => []
  | action :: rest =>
    let result := transition state action
    result :: history transition result.state rest

def State.charge (state : State) (snapshot : Bytes) : Nat :=
  if state.worker.isSome then snapshot.length else 0

theorem written_payload_respects_limit (payload : Bytes) (limit : Nat) :
    (write payload (some limit)).length ≤ limit := by
  simp [write, Nat.min_le_left]

theorem short_write_keeps_exact_prefix (snapshot : Bytes) (mutation : Mutation) (generation next : Nat)
    (readOnly : Bool) (limit : Nat)
    (exceeds : limit < (splice snapshot mutation.start mutation.length mutation.replacement).length) :
    apply snapshot mutation ⟨some ⟨generation, ⟨snapshot, readOnly⟩⟩, next⟩ (some limit) =
      ⟨⟨some ⟨generation, ⟨(splice snapshot mutation.start mutation.length mutation.replacement).take limit,
        false⟩⟩, next⟩, .writeFailed⟩ := by
  simp [apply, applyWith, write, Nat.min_eq_left (Nat.le_of_lt exceeds), Nat.ne_of_lt exceeds]

theorem apply_preserves_generation (snapshot : Bytes) (mutation : Mutation) (state : State)
    (limit : Option Nat) :
    (apply snapshot mutation state limit).state.worker.map Worker.generation = state.worker.map Worker.generation := by
  cases worker : state.worker with
  | none => simp [apply, applyWith, worker]
  | some active => by_cases h : active.file.bytes = snapshot <;> simp [apply, applyWith, worker, h]

theorem apply_preserves_copy_charge (snapshot : Bytes) (mutation : Mutation) (state : State)
    (limit : Option Nat) : (apply snapshot mutation state limit).state.charge snapshot = state.charge snapshot := by
  cases worker : state.worker with
  | none => simp [apply, applyWith, worker]
  | some active => by_cases h : active.file.bytes = snapshot <;> simp [apply, applyWith, worker, h, State.charge]

theorem changed_prefix_cannot_be_reapplied (snapshot : Bytes) (mutation : Mutation) (worker : Worker)
    (next : Nat) (different : worker.file.bytes ≠ snapshot) (limit : Option Nat) :
    apply snapshot mutation ⟨some worker, next⟩ limit = ⟨⟨some worker, next⟩, .hashMismatch⟩ := by
  simp [apply, applyWith, different]

theorem matching_prefix_allows_unlimited_retry (snapshot : Bytes) (mutation : Mutation) (worker : Worker)
    (next : Nat) (matching : worker.file.bytes = snapshot) :
    (apply snapshot mutation ⟨some worker, next⟩ none).outcome = .applied := by
  simp [apply, applyWith, matching, write]

theorem failed_restore_releases_copy_charge (snapshot : Bytes) (state : State) (limit : Option Nat)
    (failed : (reset snapshot state limit).outcome = .restoreFailed) :
    (reset snapshot state limit).state.charge snapshot = 0 := by
  cases active : state.worker with
  | none => simp [reset, active] at failed
  | some worker =>
    by_cases matched : worker.file = originalFile snapshot
    · simp [reset, active, matched] at failed
    · by_cases fits : snapshot.length ≤ limit.getD snapshot.length
      · simp [reset, active, matched, fits] at failed
      · simp [reset, active, matched, fits, State.charge]

theorem matching_reset_can_skip_all_writes (snapshot : Bytes) (generation next : Nat) (limit : Option Nat) :
    reset snapshot ⟨some ⟨generation, originalFile snapshot⟩, next⟩ limit =
      ⟨⟨some ⟨generation, originalFile snapshot⟩, next⟩, .reset⟩ := by simp [reset]

end HoiminOracle.WorkspaceMutationWrite
