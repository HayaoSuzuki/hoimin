import HoiminOracle.SchemaMigrationModel

namespace HoiminOracle.SchemaMigration

set_option maxHeartbeats 100000 in
theorem initial_invariant (version : Version) (legacyRow : Bool) :
    Invariant (initial version legacyRow) := by
  simp [Invariant, initial]

set_option maxHeartbeats 100000 in
theorem step_preserves (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event) := by
  obtain ⟨leftHolds, rightHolds⟩ := holds
  cases event <;>
    simp [step, Invariant, observe, begin, migrate, commit, fail, releaseFailed,
      setPhase, phase, nextVersion?] at leftHolds rightHolds ⊢ <;> grind

set_option maxHeartbeats 100000 in
theorem run_preserves (state : State) (trace : List Event)
    (holds : Invariant state) : Invariant (run state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (step state event) (step_preserves state event holds)

set_option maxHeartbeats 100000 in
theorem successful_actor_has_current_schema
    (state : State) (actor : Actor) (holds : Invariant state)
    (succeeded : phase state actor = .succeeded) :
    state.version = .current := by
  cases actor with
  | left => exact holds.1 succeeded
  | right => exact holds.2 succeeded

set_option maxHeartbeats 100000 in
theorem step_preserves_legacy_data (state : State) (event : Event) :
    (step state event).legacyRow = state.legacyRow := by
  cases event <;>
    simp [step, observe, begin, migrate, commit, fail, releaseFailed,
      setPhase, phase, nextVersion?] <;> grind

set_option maxHeartbeats 100000 in
theorem run_preserves_legacy_data (state : State) (trace : List Event) :
    (run state trace).legacyRow = state.legacyRow := by
  induction trace generalizing state with
  | nil => rfl
  | cons event rest induction =>
      rw [run, induction, step_preserves_legacy_data]

set_option maxHeartbeats 100000 in
theorem failure_rolls_back_committed_state (state : State) (actor : Actor) :
    (step state (.fail actor)).version = state.version := by
  simp [step, fail, releaseFailed, setPhase, phase] <;> grind

set_option maxHeartbeats 100000 in
theorem failure_preserves_legacy_data (state : State) (actor : Actor) :
    (step state (.fail actor)).legacyRow = state.legacyRow := by
  exact step_preserves_legacy_data state (.fail actor)

set_option maxHeartbeats 100000 in
theorem lock_has_at_most_one_owner
    (state : State) (ownerOne ownerTwo : Actor)
    (firstOwns : state.lock = some ownerOne) (secondOwns : state.lock = some ownerTwo) :
    ownerOne = ownerTwo := by
  simp_all

set_option maxHeartbeats 100000 in
theorem step_preserves_future_version
    (state : State) (event : Event) (future : state.version = .future) :
    (step state event).version = .future := by
  cases event <;>
    simp [step, observe, begin, migrate, commit, fail, releaseFailed,
      setPhase, phase, nextVersion?, future] <;> grind

set_option maxHeartbeats 100000 in
theorem run_preserves_future_version
    (state : State) (trace : List Event) (future : state.version = .future) :
    (run state trace).version = .future := by
  induction trace generalizing state with
  | nil => exact future
  | cons event rest induction =>
      exact induction (step state event) (step_preserves_future_version state event future)

end HoiminOracle.SchemaMigration
