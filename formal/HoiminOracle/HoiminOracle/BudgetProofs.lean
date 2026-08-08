import HoiminOracle.BudgetModel

namespace HoiminOracle.BudgetAudit

def auditInitial : State :=
  State.initial { memory := 2, copy := 2, processes := 2 }

theorem totalEntries_filter_le
    (entries : List Entry) (predicate : Entry → Bool) (kind : Kind) :
    totalEntries (entries.filter predicate) kind ≤ totalEntries entries kind := by
  induction entries with
  | nil => simp [totalEntries]
  | cons entry rest induction =>
      simp only [List.filter]
      split
      · simp only [totalEntries]
        omega
      · simp only [totalEntries]
        omega

theorem validateRelease_none (state : State) (ids : List Nat)
    (accepted : validateRelease state ids = none) :
    ids.Nodup ∧
      (∀ id ∈ ids, id ∉ state.released) ∧
      (∀ id ∈ ids, id ∈ activeIds state) := by
  unfold validateRelease hasDuplicates at accepted
  grind

theorem step_preserves_safe (state : State) (event : Event)
    (holds : safe state = true) :
    safe (step state event).state = true := by
  have holdsSafe := holds
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  cases event with
  | reserve kind amount =>
      simp only [step]
      unfold reserve
      split
      · exact holdsSafe
      · split
        · exact holdsSafe
        · split
          · exact holdsSafe
          · cases kind <;>
              simp_all [safe, Bool.and_eq_true, decide_eq_true_eq, total, totalEntries, available, limit, activeIds,
                knownIds, hasDuplicates, disjointIds, frontierValid] <;>
              split <;>
              simp_all [Bool.and_eq_true, decide_eq_true_eq] <;>
              grind
  | release ids =>
      simp only [step]
      cases accepted : validateRelease state ids with
      | some reason =>
          simp [release, accepted]
          exact holdsSafe
      | none =>
          have valid := validateRelease_none state ids accepted
          have memoryDecreases := totalEntries_filter_le state.active
            (fun entry => !(entry.id ∈ ids)) .memory
          have copyDecreases := totalEntries_filter_le state.active
            (fun entry => !(entry.id ∈ ids)) .copy
          have processesDecrease := totalEntries_filter_le state.active
            (fun entry => !(entry.id ∈ ids)) .processes
          simp [release, accepted, safe, Bool.and_eq_true, decide_eq_true_eq,
            total, activeIds, knownIds, hasDuplicates, disjointIds,
            frontierValid] at *
          split at * <;>
            simp_all [Bool.and_eq_true, decide_eq_true_eq] <;>
            grind

theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state := by
  cases event with
  | reserve kind amount =>
      simp only [step, reserve] at rejected ⊢
      split
      · simp [reject]
      · cases next : state.nextId with
        | none => simp_all [reject]
        | some id =>
            simp only [next] at rejected ⊢
            split
            · simp [reject]
            · have amountFits : ¬available state kind < amount := by omega
              have idFits : ¬state.maxId < id := by omega
              simp [amountFits, idFits] at rejected
  | release ids =>
      cases validation : validateRelease state ids with
      | none => simp [step, release, validation] at rejected
      | some reason => simp [step, release, validation, reject]

theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) :
    Invariant (step state event).state := by
  exact step_preserves_safe state event holds

theorem run_preserves_safe (state : State) (trace : List Event)
    (holds : safe state = true) :
    safe (run state trace) = true := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (step state event).state
        (step_preserves_safe state event holds)

theorem run_preserves_invariant (state : State) (trace : List Event)
    (holds : Invariant state) :
    Invariant (run state trace) := by
  exact run_preserves_safe state trace holds

def ExactRelease (before : State) (ids : List Nat) (after : State) : Prop :=
  after.limits = before.limits ∧
    after.active = before.active.filter (fun entry => !(entry.id ∈ ids)) ∧
    after.released = ids ++ before.released ∧
    after.nextId = before.nextId ∧
    after.maxId = before.maxId

theorem successful_release_is_exact (state : State) (ids : List Nat)
    (accepted : (release state ids).rejection = none) :
    ExactRelease state ids (release state ids).state := by
  unfold release at accepted ⊢
  split <;> simp_all [ExactRelease, reject]

theorem reachable_is_safe (trace : List Event) :
    safe (run auditInitial trace) = true := by
  exact run_preserves_safe auditInitial trace (by decide)

end HoiminOracle.BudgetAudit
