import HoiminOracle.SessionDispatchModel
import Lean.Data.Json

namespace HoiminOracle.SessionDispatch

private structure Case where
  command : Command
  waiter : Waiter
  event : Event

private def commandName : Command → String
  | .beginRun => "begin"
  | .resume => "resume"

private def waiterName : Waiter → String
  | .awaited => "awaited"
  | .waiterCancelled => "waiter_cancelled"
  | .queuedCancelled => "queued_cancelled"
  | .dispatcherDropped => "dispatcher_dropped"
  | .unpolled => "unpolled"

private def eventName : Event → String
  | .normal => "normal"
  | .databaseError => "database_error"
  | .budgetChanged => "budget_changed"

private def replyName : Reply → String
  | .started => "started"
  | .resumed => "resumed"
  | .commitError => "commit_error"
  | .recheckError => "recheck_error"
  | .candidateChanged => "candidate_changed"

private def Case.id (item : Case) :=
  s!"{commandName item.command}-{waiterName item.waiter}-{eventName item.event}"

private def cases : List Case :=
  ([.awaited, .waiterCancelled, .queuedCancelled, .dispatcherDropped, .unpolled] : List Waiter).flatMap fun waiter =>
    ([.normal, .databaseError] : List Event).map (fun event => ⟨.beginRun, waiter, event⟩) ++
    ([.normal, .databaseError, .budgetChanged] : List Event).map (fun event => ⟨.resume, waiter, event⟩)

private def observe (item : Case) : Observation := dispatch item.command item.waiter item.event

private def snapshotJson (state : Snapshot) : Lean.Json :=
  .mkObj [("budget", Lean.toJson state.budget), ("locked", .bool state.locked)]

def corpus : String :=
  String.join (cases.map fun item =>
    let observation := observe item
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("command", .str (commandName item.command)), ("waiter", .str (waiterName item.waiter))
      , ("event", .str (eventName item.event)), ("initial", snapshotJson (initial item.command))
      , ("requested_budget", Lean.toJson requestedBudget), ("edited_budget", Lean.toJson editedBudget)
      , ("pending", observation.pending.elim .null snapshotJson)
      , ("settled", snapshotJson observation.settled)
      , ("after_dispatcher_drop", snapshotJson observation.afterDispatcherDrop)
      , ("reply", observation.reply.elim .null (.str ∘ replyName)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("unpolled_call_starts_work", fun item =>
        if item.waiter == .unpolled then dispatch item.command .waiterCancelled item.event else observe item)
    , ("cancelling_waiter_rolls_back_work", fun item =>
        let result := observe item
        if item.waiter == .waiterCancelled then { result with settled := initial item.command } else result)
    , ("queued_cancellation_skips_scheduled_work", fun item =>
        if item.waiter == .queuedCancelled then dispatch item.command .unpolled item.event
        else observe item)
    , ("dispatcher_drop_releases_provisional_lock_early", fun item =>
        let result := observe item
        if item.waiter == .dispatcherDropped then { result with pending := some (initial item.command) }
        else result)
    , ("dispatcher_drop_leaks_completed_lock", fun item =>
        let result := observe item
        if item.waiter == .dispatcherDropped then { result with settled := (finishWork item.command item.event).2 }
        else result)
    , ("resume_claims_before_selection_gate", fun item =>
        let result := observe item
        if item.command == .resume then
          { result with pending := result.pending.map (fun state => { state with locked := true }) }
        else result)
    , ("resume_overwrites_budget_after_candidate_change", fun item =>
        if item.event == .budgetChanged then dispatch item.command item.waiter .normal else observe item)
    , ("publish_begin_before_commit", fun item =>
        let result := observe item
        if item.command == .beginRun && result.pending.isSome then
          { result with pending := some ⟨some requestedBudget, true⟩ }
        else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"session dispatch sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session dispatch missed variant: {name}"
      return 1
  IO.println s!"session_dispatch_cases={cases.length}"
  return 0

end HoiminOracle.SessionDispatch
