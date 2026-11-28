import HoiminOracle.CgroupLifecycleModel
import Lean.Data.Json

namespace HoiminOracle.CgroupLifecycle

private inductive Action where
  | terminate (root : Root) (failure : Bool) (accountFault : Option Root)
  | close (faults : CloseFaults) (accountFault : Option Root)
  | overlap (rootFails : Bool) (faults : CloseFaults)

private structure Case where
  initial : State
  action : Action

private def initialStates : List State :=
  let statuses : List Status := [.missing, .inactive, .active]
  statuses.flatMap fun first => statuses.map fun second => ⟨first, second, false, false⟩

private def faults : List CloseFaults :=
  [false, true].flatMap fun first =>
    [false, true].flatMap fun second => [false, true].map fun run => ⟨first, second, run⟩

private def accountFaults : List (Option Root) := [none, some .first, some .second]

private def cases : List Case :=
  (initialStates.flatMap fun state =>
    ([.first, .second] : List Root).flatMap fun root =>
      [false, true].flatMap fun failure =>
        accountFaults.map fun fault => ⟨state, .terminate root failure fault⟩) ++
  (initialStates.flatMap fun state =>
    faults.flatMap fun failure => accountFaults.map fun fault => ⟨state, .close failure fault⟩) ++
  (([.inactive, .active] : List Status).flatMap fun sibling =>
    [false, true].flatMap fun rootFails =>
      faults.map fun failure => ⟨⟨.active, sibling, false, false⟩, .overlap rootFails failure⟩)

private def rootName : Root → String
  | .first => "first"
  | .second => "second"

private def targetName : Target → String
  | .root root => rootName root
  | .run => "run"

private def statusName : Status → String
  | .missing => "missing"
  | .inactive => "inactive"
  | .active => "active"

private def stateJson (state : State) : Lean.Json :=
  .mkObj [("first", .str (statusName state.first)), ("second", .str (statusName state.second)),
    ("closed", .bool state.closed), ("cleaned", .bool state.cleaned)]

private def callJson (call : Call) : Lean.Json :=
  .mkObj [("target", .str (targetName call.target)), ("state", stateJson call.state),
    ("held_roots", Lean.toJson (call.heldRoots.map rootName)), ("close_held", .bool call.closeHeld)]

private def outcomeJson (outcome : Outcome) : Lean.Json :=
  .mkObj [("after", stateJson outcome.after), ("calls", Lean.toJson (outcome.calls.map callJson)),
    ("cleanup_error", outcome.cleanupError.elim .null (.str ∘ targetName)),
    ("accounting_error", .bool outcome.accountingError)]

private def faultsJson (fault : CloseFaults) : Lean.Json :=
  .mkObj [("first", .bool fault.first), ("second", .bool fault.second), ("run", .bool fault.run)]

private def actionJson : Action → Lean.Json
  | .terminate root fail fault =>
    .mkObj [("kind", .str "terminate"), ("root", .str (rootName root)), ("fail", .bool fail),
      ("account_fault", fault.elim .null (.str ∘ rootName))]
  | .close faults fault =>
    .mkObj [("kind", .str "close"), ("faults", faultsJson faults),
      ("account_fault", fault.elim .null (.str ∘ rootName))]
  | .overlap rootFails faults =>
    .mkObj [("kind", .str "overlap"), ("root_fails", .bool rootFails), ("faults", faultsJson faults)]

private def observeFirst (item : Case) : Outcome :=
  match item.action with
  | .terminate root fail fault => terminate item.initial root fail fault
  | .close faults fault => close item.initial faults fault
  | .overlap rootFails faults =>
    laterClose item.initial (terminate item.initial .first rootFails none) faults

private def observeRetry (item : Case) (after : State) : Outcome :=
  match item.action with
  | .terminate root .. => terminate after root false none
  | _ => close after {} none

private def observeOverlap (item : Case) : Lean.Json :=
  match item.action with
  | .overlap rootFails _ =>
    let terminated := terminate item.initial .first rootFails none
    let pending := setStatus item.initial .first .inactive
    .mkObj [("before_release", stateJson { pending with closed := true }),
      ("root_attempt", outcomeJson terminated)]
  | _ => .null

def corpus : String := String.join (cases.zipIdx.map fun (item, index) =>
  let first := observeFirst item
  (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str s!"cleanup-{index}"), ("initial", stateJson item.initial),
    ("action", actionJson item.action), ("first", outcomeJson first),
    ("retry", outcomeJson (observeRetry item first.after)), ("overlap", observeOverlap item)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Outcome)) :=
    [ ("omit_closed_publication", fun item =>
        let result := observeFirst item
        { result with after := { result.after with closed := false } })
    , ("keep_accounting_active_in_callback", fun item =>
        let result := observeFirst item
        let calls := result.calls.map fun call => { call with state := item.initial }
        { result with calls })
    , ("forget_surviving_root_accounting", fun item =>
        let result := observeFirst item
        if result.cleanupError.isSome then
          { result with after := pauseAll result.after }
        else result)
    , ("disregard_root_error_when_recursive_cleanup_succeeds", fun item =>
        let result := observeFirst item
        if result.after.cleaned then { result with cleanupError := none } else result)
    , ("forget_accounting_error", fun item => { observeFirst item with accountingError := false })
    , ("call_close_without_root_locks", fun item =>
        let result := observeFirst item
        let calls := result.calls.map fun call =>
          if call.closeHeld then { call with heldRoots := [] } else call
        { result with calls })
    , ("lose_captured_paths_while_waiting", fun item =>
        match item.action with
        | .overlap fail faults => close (terminate item.initial .first fail none).after faults none
        | _ => observeFirst item) ]
  for (name, variant) in variants do
    if cases.any (fun item => decide (variant item ≠ observeFirst item))
    then IO.println s!"cgroup lifecycle sensitivity: {name}"
    else
      IO.eprintln s!"cgroup lifecycle missed variant: {name}"
      return 1
  IO.println s!"cgroup_lifecycle_cases={cases.length}"
  return 0
where
  pauseAll (state : State) : State :=
    (present state).foldl (fun result root => setStatus result root .inactive) state

end HoiminOracle.CgroupLifecycle
