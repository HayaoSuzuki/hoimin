import Std

namespace HoiminOracle.FinishTransaction

inductive Row where
  | missing | unfinished | resumable | complete | completeUnfinished
  deriving DecidableEq, BEq

inductive Fault where
  | none | beginBusy | updateAbort | updateIgnore | commitReject
  deriving DecidableEq, BEq

inductive Outcome where
  | finished | ownerRequired | beginError | updateError | stateError | commitError
  deriving DecidableEq, BEq

structure State where
  row : Row
  owned : Bool
  deriving DecidableEq

structure Observation where
  outcome : Outcome
  commitReached : Bool
  after : State
  deriving DecidableEq

def finishedRow (complete : Bool) : Row :=
  if complete then .complete else .resumable

def finish (before : State) (fault : Fault) (complete : Bool) : Observation :=
  if !before.owned then ⟨.ownerRequired, false, before⟩
  else
    match fault, before.row with
    | .beginBusy, _ => ⟨.beginError, false, before⟩
    | _, .missing | _, .complete | _, .completeUnfinished => ⟨.stateError, false, before⟩
    | .none, _ => ⟨.finished, true, ⟨finishedRow complete, false⟩⟩
    | .updateAbort, _ => ⟨.updateError, false, before⟩
    | .updateIgnore, _ => ⟨.stateError, false, before⟩
    | .commitReject, _ => ⟨.commitError, true, before⟩

theorem owner_required_before_database_access (row : Row) (fault : Fault) (complete : Bool) :
    finish ⟨row, false⟩ fault complete = ⟨.ownerRequired, false, ⟨row, false⟩⟩ := by
  simp [finish]

theorem complete_rows_are_refused_regardless_of_finished_flag (fault : Fault) (complete : Bool)
    (began : fault ≠ .beginBusy) :
    (finish ⟨.complete, true⟩ fault complete).outcome = .stateError ∧
      (finish ⟨.completeUnfinished, true⟩ fault complete).outcome = .stateError := by
  cases fault <;> simp_all [finish]

theorem refusal_preserves_row_and_ownership (before : State) (fault : Fault) (complete : Bool)
    (refused : (finish before fault complete).outcome ≠ .finished) :
    (finish before fault complete).after = before := by
  obtain ⟨row, owned⟩ := before
  cases owned <;> cases row <;> cases fault <;> simp_all [finish]

theorem accepted_finish_sets_flags_and_releases_ownership
    (before : State) (fault : Fault) (complete : Bool)
    (accepted : (finish before fault complete).outcome = .finished) :
    finish before fault complete = ⟨.finished, true, ⟨finishedRow complete, false⟩⟩ := by
  obtain ⟨row, owned⟩ := before
  cases owned <;> cases row <;> cases fault <;> simp_all [finish]

theorem retry_after_failed_commit_uses_new_request (before : State) (first retry : Bool)
    (rejected : (finish before .commitReject first).outcome = .commitError) :
    finish (finish before .commitReject first).after .none retry =
      ⟨.finished, true, ⟨finishedRow retry, false⟩⟩ := by
  obtain ⟨row, owned⟩ := before
  cases owned <;> cases row <;> simp_all [finish]

theorem immediate_repeat_after_success_requires_ownership
    (before : State) (fault retryFault : Fault) (complete retry : Bool)
    (accepted : (finish before fault complete).outcome = .finished) :
    (finish (finish before fault complete).after retryFault retry).outcome = .ownerRequired := by
  rw [accepted_finish_sets_flags_and_releases_ownership before fault complete accepted]
  simp [finish]

end HoiminOracle.FinishTransaction
