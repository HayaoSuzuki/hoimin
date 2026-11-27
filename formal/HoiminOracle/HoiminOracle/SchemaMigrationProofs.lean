import HoiminOracle.SchemaMigrationModel

namespace HoiminOracle.SchemaMigration

theorem admitSnapshot_initializes_iff (version : Version) (applicationId : Int) (catalog : Catalog) :
    admitSnapshot version applicationId catalog = .initialize ↔
      version = .fresh ∧ applicationId = 0 ∧ catalog = .empty := by
  cases version <;> cases catalog <;> by_cases zero : applicationId = 0 <;>
    simp [admitSnapshot, zero]

theorem admitSnapshot_reopens_iff (version : Version) (applicationId : Int) (catalog : Catalog) :
    admitSnapshot version applicationId catalog = .reopen ↔
      version = .current ∧ applicationId = 0 ∧ catalog = .canonical := by
  cases version <;> cases catalog <;> by_cases zero : applicationId = 0 <;>
    simp [admitSnapshot, zero]

theorem admitSnapshot_unsupported_precedes_identity (version : Version) (applicationId : Int)
    (catalog : Catalog) (notFresh : version ≠ .fresh) (notCurrent : version ≠ .current) :
    admitSnapshot version applicationId catalog = .unsupportedVersion := by
  cases version <;> simp_all [admitSnapshot]

set_option maxHeartbeats 100000 in
theorem initial_invariant (version : Version) (legacyRow : Bool) :
    Invariant (initial version legacyRow) := by
  simp [Invariant, initial]

set_option maxHeartbeats 100000 in
theorem step_preserves (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event) := by
  obtain ⟨leftHolds, rightHolds⟩ := holds
  cases event <;>
    simp [step, Invariant, observe, begin, initializeSchema, commit, fail, releaseFailed,
      setPhase, phase] at leftHolds rightHolds ⊢ <;> grind

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
    simp [step, observe, begin, initializeSchema, commit, fail, releaseFailed,
      setPhase, phase] <;> grind

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
    simp [step, observe, begin, initializeSchema, commit, fail, releaseFailed,
      setPhase, phase, future] <;> grind

set_option maxHeartbeats 100000 in
theorem run_preserves_future_version
    (state : State) (trace : List Event) (future : state.version = .future) :
    (run state trace).version = .future := by
  induction trace generalizing state with
  | nil => exact future
  | cons event rest induction =>
      exact induction (step state event) (step_preserves_future_version state event future)

set_option maxHeartbeats 100000 in
theorem step_preserves_unsupported_version
    (state : State) (event : Event)
    (notFresh : state.version ≠ .fresh) (notCurrent : state.version ≠ .current) :
    (step state event).version = state.version := by
  cases event <;>
    simp [step, observe, begin, initializeSchema, commit, fail, releaseFailed,
      setPhase, phase] <;> grind

set_option maxHeartbeats 100000 in
theorem run_preserves_unsupported_version
    (state : State) (trace : List Event)
    (notFresh : state.version ≠ .fresh) (notCurrent : state.version ≠ .current) :
    (run state trace).version = state.version := by
  induction trace generalizing state with
  | nil => rfl
  | cons event rest induction =>
      have unchanged := step_preserves_unsupported_version state event notFresh notCurrent
      rw [run, induction (step state event) (by simpa [unchanged] using notFresh)
        (by simpa [unchanged] using notCurrent), unchanged]

set_option maxHeartbeats 20000 in
private theorem flatten_sql_words (isSpace : Char → Bool) (text : List Char) :
    (text.splitOnP isSpace).flatten = text.filter (fun c => !isSpace c) := by
  induction text with
  | nil => simp
  | cons c rest induction =>
    rw [List.splitOnP_cons_eq_if_modifyHead]
    by_cases separator : isSpace c = true
    · simp [separator, induction]
    · cases words : rest.splitOnP isSpace with
      | nil => exact False.elim (List.splitOnP_ne_nil isSpace rest words)
      | cons first remaining =>
        rw [words] at induction
        simp [separator, ← induction]

set_option maxHeartbeats 20000 in
theorem schemaWords_preserve_nonseparators (isSpace : Char → Bool) (text : List Char) :
    (schemaWords isSpace text).flatten = text.filter (fun c => !isSpace c) := by
  rw [schemaWords, List.flatten_filter_not_isEmpty, flatten_sql_words]

set_option maxHeartbeats 20000 in
private theorem filter_schema_join (words : List (List Char)) :
    ([' '].intercalate words).filter (fun c => !sqliteSchemaSpace c) =
      words.flatten.filter (fun c => !sqliteSchemaSpace c) := by
  induction words with
  | nil => simp
  | cons first rest induction =>
    cases rest <;> simp [List.filter_append, sqliteSchemaSpace] at induction ⊢
    rw [induction]

set_option maxHeartbeats 20000 in
theorem normalizeSchemaSql_preserves_nonseparators (text : List Char) :
    (normalizeSchemaSql sqliteSchemaSpace text).filter (fun c => !sqliteSchemaSpace c) =
      text.filter (fun c => !sqliteSchemaSpace c) := by
  unfold normalizeSchemaSql
  rw [← List.intercalate_eq_intercalateTR, filter_schema_join, schemaWords_preserve_nonseparators]
  simp [List.filter_filter]

set_option maxHeartbeats 20000 in
theorem inserting_schema_nonseparator_changes_comparison (leading trailing : List Char) (c : Char)
    (notSeparator : sqliteSchemaSpace c = false) :
    normalizeSchemaSql sqliteSchemaSpace (leading ++ c :: trailing) ≠
      normalizeSchemaSql sqliteSchemaSpace (leading ++ trailing) := by
  intro same
  have lengths := congrArg (fun text : List Char => (text.filter fun x => !sqliteSchemaSpace x).length) same
  simp [normalizeSchemaSql_preserves_nonseparators, List.filter_append, notSeparator] at lengths

end HoiminOracle.SchemaMigration
