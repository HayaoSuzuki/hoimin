import HoiminOracle.FinishTransactionModel
import Lean.Data.Json

namespace HoiminOracle.FinishTransaction

private structure Case where
  before : State
  fault : Fault
  complete : Bool

private def rowName : Row → String
  | .missing => "missing"
  | .unfinished => "unfinished"
  | .resumable => "resumable"
  | .complete => "complete"
  | .completeUnfinished => "complete_unfinished"

private def faultName : Fault → String
  | .none => "none"
  | .beginBusy => "begin_busy"
  | .updateAbort => "update_abort"
  | .updateIgnore => "update_ignore"
  | .commitReject => "commit_reject"

private def outcomeName : Outcome → String
  | .finished => "finished"
  | .ownerRequired => "owner_required"
  | .beginError => "begin_error"
  | .updateError => "update_error"
  | .stateError => "state_error"
  | .commitError => "commit_error"

private def Case.id (item : Case) :=
  s!"{item.before.owned}-{rowName item.before.row}-{faultName item.fault}-{item.complete}"

private def cases : List Case :=
  [false, true].flatMap fun owned =>
    ([.missing, .unfinished, .resumable, .complete, .completeUnfinished] : List Row).flatMap fun row =>
      ([.none, .beginBusy, .updateAbort, .updateIgnore, .commitReject] : List Fault).flatMap fun fault =>
        [false, true].map fun complete => ⟨⟨row, owned⟩, fault, complete⟩

private def observe (item : Case) : Observation := finish item.before item.fault item.complete

private def stateJson (state : State) : Lean.Json :=
  .mkObj [("row", .str (rowName state.row)), ("owned", .bool state.owned)]

private def observationJson (observation : Observation) : Lean.Json :=
  .mkObj [("outcome", .str (outcomeName observation.outcome)),
    ("commit_reached", .bool observation.commitReached), ("after", stateJson observation.after)]

def corpus : String :=
  String.join (cases.map fun item =>
    let first := observe item
    let retry := finish first.after .none (!item.complete)
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("before", stateJson item.before), ("fault", .str (faultName item.fault))
      , ("complete", .bool item.complete), ("first", observationJson first)
      , ("retry", observationJson retry) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("check_database_before_owner", fun item =>
        if item.fault == .beginBusy then ⟨.beginError, false, item.before⟩ else observe item)
    , ("reject_resumable_row", fun item =>
        let result := observe item
        if item.before.row == .resumable && result.outcome == .finished
        then ⟨.stateError, false, item.before⟩ else result)
    , ("accept_complete_without_finished_flag", fun item =>
        if item.before.row == .completeUnfinished && item.before.owned && item.fault == .none then
          ⟨.finished, true, ⟨finishedRow item.complete, false⟩⟩
        else observe item)
    , ("ignore_updated_row_count", fun item =>
        let result := observe item
        if item.fault == .updateIgnore &&
            (finish item.before .none item.complete).outcome == .finished
        then ⟨.finished, true, ⟨finishedRow item.complete, false⟩⟩ else result)
    , ("hide_update_error", fun item =>
        let result := observe item
        if result.outcome == .updateError then { result with outcome := .stateError } else result)
    , ("publish_on_failed_commit", fun item =>
        let result := observe item
        if result.outcome == .commitError
        then { result with after := { result.after with row := finishedRow item.complete } } else result)
    , ("release_on_failed_commit", fun item =>
        let result := observe item
        if result.outcome == .commitError
        then { result with after := { result.after with owned := false } } else result)
    , ("retain_on_success", fun item =>
        let result := observe item
        if result.outcome == .finished
        then { result with after := { result.after with owned := true } } else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"finish transaction sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"finish transaction missed variant: {name}"
      return 1
  IO.println s!"finish_transaction_cases={cases.length}"
  return 0

end HoiminOracle.FinishTransaction
