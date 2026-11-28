import Std

namespace HoiminOracle.PersistTransaction

inductive Admission where
  | openRun | completedRun | noOwner
  deriving DecidableEq, BEq

inductive Image where
  | empty | inconclusive | determinate | first | retry
  deriving DecidableEq, BEq

inductive Draft where
  | first | retry

def Draft.image : Draft → Image
  | .first => .first
  | .retry => .retry

inductive Write where
  | deleteDiagnostics | deleteResult | deleteCandidate
  | insertCandidate | insertResult | insertFirstDiagnostic | insertSecondDiagnostic
  deriving DecidableEq, BEq

inductive Fault where
  | none | beginBusy | abort (atWrite : Write) | candidateOverflow | invalidDiagnostic | commitReject
  deriving DecidableEq, BEq

inductive Outcome where
  | persisted | ownerRequired | beginError | runComplete | duplicate
  | writeAborted (atWrite : Write) | candidateOverflow | invalidDiagnostic | commitError
  deriving DecidableEq, BEq

structure Observation where
  outcome : Outcome
  beforeCommit : Option Image
  after : Image
  committedWrites : List Write
  deriving DecidableEq

def plan (stored : Image) : List Write :=
  (if stored == .inconclusive then [.deleteDiagnostics, .deleteResult, .deleteCandidate] else []) ++
    [.insertCandidate, .insertResult, .insertFirstDiagnostic, .insertSecondDiagnostic]

def stopsAt (fault : Fault) (write : Write) : Bool :=
  match fault, write with
  | .abort failing, _ => write == failing
  | .candidateOverflow, .insertCandidate | .invalidDiagnostic, .insertSecondDiagnostic => true
  | _, _ => false

def writeRefusal (fault : Fault) (write : Write) : Outcome :=
  match fault with
  | .candidateOverflow => .candidateOverflow
  | .invalidDiagnostic => .invalidDiagnostic
  | _ => .writeAborted write

def refusal (stored : Image) (outcome : Outcome) : Observation :=
  ⟨outcome, none, stored, []⟩

def writeResult (stored : Image) (draft : Draft) (fault : Fault) : Observation :=
  let writes := plan stored
  match writes.find? (stopsAt fault) with
  | some write => refusal stored (writeRefusal fault write)
  | none =>
    if fault == .commitReject then ⟨.commitError, some stored, stored, []⟩
    else ⟨.persisted, some stored, draft.image, writes⟩

def refusalReason (admission : Admission) (stored : Image) (fault : Fault) : Option Outcome :=
  match admission, fault, stored with
  | .noOwner, _, _ => some .ownerRequired
  | _, .beginBusy, _ => some .beginError
  | .completedRun, _, _ => some .runComplete
  | _, _, .determinate | _, _, .first | _, _, .retry => some .duplicate
  | .openRun, _, _ => none

def persist (admission : Admission) (stored : Image) (draft : Draft) (fault : Fault) : Observation :=
  match refusalReason admission stored fault with
  | some reason => refusal stored reason
  | none => writeResult stored draft fault

private theorem write_refusal_cannot_persist (fault : Fault) (write : Write) :
    writeRefusal fault write ≠ .persisted := by
  cases fault <;> simp [writeRefusal]

private theorem failed_writes_rollback (stored : Image) (draft : Draft) (fault : Fault)
    (failed : (writeResult stored draft fault).outcome ≠ .persisted) :
    (writeResult stored draft fault).after = stored ∧
      (writeResult stored draft fault).committedWrites = [] := by
  cases stage : (plan stored).find? (stopsAt fault) <;>
    by_cases rejected : (fault == .commitReject) = true <;>
    simp only [writeResult, stage, rejected, if_pos, refusal] at failed ⊢ <;> simp_all

theorem faults_leave_committed_image_unchanged (admission : Admission) (stored : Image)
    (draft : Draft) (fault : Fault) (failed : (persist admission stored draft fault).outcome ≠ .persisted) :
    (persist admission stored draft fault).after = stored ∧
      (persist admission stored draft fault).committedWrites = [] := by
  cases gate : refusalReason admission stored fault with
  | some reason => simp [persist, gate, refusal]
  | none =>
    simp only [persist, gate] at failed ⊢
    exact failed_writes_rollback stored draft fault failed

theorem reader_before_commit_sees_only_the_committed_image (admission : Admission)
    (stored : Image) (draft : Draft) (fault : Fault) (seen : Image)
    (observed : (persist admission stored draft fault).beforeCommit = some seen) :
    seen = stored := by
  cases gate : refusalReason admission stored fault with
  | some reason => simp [persist, gate, refusal] at observed
  | none =>
    cases stage : (plan stored).find? (stopsAt fault) <;>
      by_cases rejected : (fault == .commitReject) = true <;>
      simp only [persist, gate, writeResult, stage, rejected, if_pos, refusal] at observed <;>
      simp_all

private theorem accepted_writes_replace_image (stored : Image) (draft : Draft) (fault : Fault)
    (accepted : (writeResult stored draft fault).outcome = .persisted) :
    (writeResult stored draft fault).after = draft.image := by
  cases stage : (plan stored).find? (stopsAt fault) <;>
    by_cases rejected : (fault == .commitReject) = true <;>
    simp only [writeResult, stage, rejected, if_pos, refusal] at accepted ⊢ <;>
    simp_all [write_refusal_cannot_persist]

private theorem early_refusal_cannot_persist (admission : Admission) (stored : Image)
    (fault : Fault) (reason : Outcome) (gate : refusalReason admission stored fault = some reason) :
    reason ≠ .persisted := by
  cases admission <;> cases fault <;> cases stored <;>
    simp [refusalReason] at gate <;> cases gate <;> decide

theorem first_determinate_result_survives_a_retry (stored : Image)
    (fault : Fault) (accepted : (persist .openRun stored .first fault).outcome = .persisted) :
    persist .openRun (persist .openRun stored .first fault).after .retry .none =
      refusal .first .duplicate := by
  have written : (persist .openRun stored .first fault).after = .first := by
    cases gate : refusalReason .openRun stored fault with
    | some reason =>
      simp only [persist, gate, refusal] at accepted
      exact False.elim (early_refusal_cannot_persist .openRun stored fault reason gate accepted)
    | none =>
      simp only [persist, gate] at accepted ⊢
      exact accepted_writes_replace_image stored .first fault accepted
  rw [written]
  rfl

theorem second_diagnostic_refusal_prevents_partial_replacement (stored : Image)
    (replaceable : stored = .empty ∨ stored = .inconclusive) (draft : Draft) :
    persist .openRun stored draft .invalidDiagnostic = refusal stored .invalidDiagnostic := by
  rcases replaceable with rfl | rfl <;> rfl

end HoiminOracle.PersistTransaction
