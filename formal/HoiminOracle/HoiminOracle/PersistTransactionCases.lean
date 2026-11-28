import HoiminOracle.PersistTransactionModel
import Lean.Data.Json

namespace HoiminOracle.PersistTransaction

private structure Case where
  admission : Admission
  stored : Image
  fault : Fault

private def admissionName : Admission → String
  | .openRun => "open_run"
  | .completedRun => "completed_run"
  | .noOwner => "no_owner"

private def imageName : Image → String
  | .empty => "empty"
  | .inconclusive => "inconclusive"
  | .determinate => "determinate"
  | .first => "first"
  | .retry => "retry"

private def writeName : Write → String
  | .deleteDiagnostics => "delete_diagnostics"
  | .deleteResult => "delete_result"
  | .deleteCandidate => "delete_candidate"
  | .insertCandidate => "insert_candidate"
  | .insertResult => "insert_result"
  | .insertFirstDiagnostic => "insert_first_diagnostic"
  | .insertSecondDiagnostic => "insert_second_diagnostic"

private def faultName : Fault → String
  | .none => "none"
  | .beginBusy => "begin_busy"
  | .abort write => writeName write
  | .candidateOverflow => "candidate_overflow"
  | .invalidDiagnostic => "invalid_diagnostic"
  | .commitReject => "commit_reject"

private def outcomeName : Outcome → String
  | .persisted => "persisted"
  | .ownerRequired => "owner_required"
  | .beginError => "begin_error"
  | .runComplete => "run_complete"
  | .duplicate => "duplicate"
  | .writeAborted write => writeName write
  | .candidateOverflow => "candidate_overflow"
  | .invalidDiagnostic => "invalid_diagnostic"
  | .commitError => "commit_error"

private def Case.id (item : Case) :=
  s!"{admissionName item.admission}-{imageName item.stored}-{faultName item.fault}"

private def cases : List Case :=
  ([.openRun, .completedRun, .noOwner] : List Admission).flatMap fun admission =>
    ([.empty, .inconclusive, .determinate] : List Image).flatMap fun stored =>
      let faults := [.none, .beginBusy, .candidateOverflow, .invalidDiagnostic, .commitReject] ++
        (plan .inconclusive).map Fault.abort
      faults.map fun fault => ⟨admission, stored, fault⟩

private def observe (item : Case) : Observation := persist item.admission item.stored .first item.fault

private def observationJson (observation : Observation) : Lean.Json :=
  .mkObj [("outcome", .str (outcomeName observation.outcome)),
    ("before_commit", observation.beforeCommit.elim .null (.str ∘ imageName)),
    ("after", .str (imageName observation.after)),
    ("committed_writes", Lean.toJson (observation.committedWrites.map writeName))]

def corpus : String :=
  String.join (cases.map fun item =>
    let first := observe item
    let retry := persist item.admission first.after .retry .none
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("admission", .str (admissionName item.admission)), ("stored", .str (imageName item.stored))
      , ("fault", .str (faultName item.fault)), ("first", observationJson first)
      , ("retry", observationJson retry) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("claim_busy_before_owner_check", fun item =>
        if item.fault == .beginBusy then refusal item.stored .beginError else observe item)
    , ("overwrite_determinate", fun item =>
        let result := observe item
        if result.outcome == .duplicate then writeResult item.stored .first item.fault else result)
    , ("publish_uncommitted_rows_to_reader", fun item =>
        let result := observe item
        if result.beforeCommit.isSome then { result with beforeCommit := some .first } else result)
    , ("delete_candidate_before_diagnostics", fun item =>
        let result := observe item
        let reordered := [Write.deleteCandidate, .deleteResult, .deleteDiagnostics] ++ plan .empty
        if result.outcome == .persisted && item.stored == .inconclusive
        then { result with committedWrites := reordered } else result)
    , ("ignore_second_diagnostic_error", fun item =>
        let result := observe item
        if result.outcome == .invalidDiagnostic then writeResult item.stored .first .none else result)
    , ("keep_deletions_after_insert_fault", fun item =>
        let result := observe item
        if item.stored == .inconclusive && result.outcome == .writeAborted .insertResult
        then { result with after := .empty } else result)
    , ("publish_failed_commit", fun item =>
        let result := observe item
        if result.outcome == .commitError then { result with after := .first } else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"persist transaction sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"persist transaction missed variant: {name}"
      return 1
  IO.println s!"persist_transaction_cases={cases.length}"
  return 0

end HoiminOracle.PersistTransaction
