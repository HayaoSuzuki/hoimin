import HoiminOracle.WorkerGrantPublicationModel
import HoiminOracle.WorkspaceCapacityModel
import Lean.Data.Json

namespace HoiminOracle.WorkerGrantPublication

private def grantName : Grant → String
  | .a => "a"
  | .b => "b"
  | .wrongPreflight => "wrong_preflight"
  | .wrongAllowance => "wrong_allowance"

private def verdictName : Verdict → String
  | .holdingLock => "holding_lock"
  | .queued => "queued"
  | .published => "published"
  | .copying => "copying"
  | .ready => "ready"
  | .copyFailed => "copy_failed"
  | .released => "released"
  | .retained => "retained"
  | .preflightMismatch => "preflight_mismatch"
  | .outOfRange => "out_of_range"
  | .allowanceMismatch => "allowance_mismatch"
  | .reservationMismatch => "reservation_mismatch"
  | .duplicate => "duplicate"
  | .disabled => "disabled"

private structure Case where
  id : String
  perWorker : Nat
  actions : List Action

private structure Competition where
  name : String
  request : Option Request
  admitted : Bool := false

private def competingGrant (leader : Grant) : Grant := if leader = .a then .b else .a

private def competitors (grant : Grant) (leader sibling : Nat) : List Competition :=
  let foreign := competingGrant grant
  [ ⟨"later-sibling", none, false⟩
  , ⟨"queued-sibling", some ⟨grant, sibling, false⟩, true⟩
  , ⟨"queued-sibling-fails", some ⟨grant, sibling, true⟩, true⟩
  , ⟨"duplicate-leader", some ⟨grant, leader, false⟩, false⟩
  , ⟨"foreign-sibling", some ⟨foreign, sibling, false⟩, false⟩
  , ⟨"foreign-duplicate", some ⟨foreign, leader, false⟩, false⟩
  , ⟨"wrong-preflight-and-range", some ⟨.wrongPreflight, 2, false⟩, false⟩
  , ⟨"wrong-range-and-allowance", some ⟨.wrongAllowance, 2, false⟩, false⟩
  , ⟨"wrong-allowance", some ⟨.wrongAllowance, sibling, false⟩, false⟩ ]

private def matrix : List Case :=
  [0, 4].flatMap fun bytes => [0, 1].flatMap fun leader => [false, true].flatMap fun leadFails =>
    let grant := if leader = 0 then .a else .b
    let sibling := 1 - leader
    (competitors grant leader sibling).map fun race =>
      let start := [.start ⟨grant, leader, leadFails⟩] ++ (race.request.toList.map Action.queue) ++ [.publish]
      let later := if race.request.isNone then [.start ⟨grant, sibling, false⟩] else []
      let hasSibling := race.request.isNone || race.admitted
      let siblingFails := race.request.any Request.fail
      let finishLeader := [.finish leader]
      let finishSibling := if hasSibling then [.finish sibling] else []
      let finish := if leadFails then finishLeader ++ finishSibling else finishSibling ++ finishLeader
      let release := (if leadFails then [] else [.release leader]) ++
        (if hasSibling && !siblingFails then [.release sibling] else [])
      ⟨s!"{bytes}-{leader}-{leadFails}-{race.name}", bytes, start ++ later ++ finish ++ release ++
        [.start ⟨competingGrant grant, leader, false⟩, .start ⟨grant, leader, false⟩, .finish leader, .release leader]⟩

private def readySiblingCases : List Case :=
  [4, HoiminOracle.WorkspaceCapacity.blockBytes + 2].flatMap fun bytes => [0, 1].map fun leader =>
    let grant := if leader = 0 then .a else .b
    let sibling := 1 - leader
    ⟨s!"{bytes}-{leader}-failure-with-ready-sibling", bytes,
      [.start ⟨grant, leader, true⟩, .queue ⟨grant, sibling, false⟩, .publish,
        .finish sibling, .finish leader, .start ⟨competingGrant grant, leader, false⟩,
        .start ⟨grant, leader, false⟩, .finish leader, .release leader, .release sibling]⟩

private def retainedCases : List Case :=
  [0, 4].flatMap fun bytes => [0, 1].map fun leader =>
    let grant := if leader = 0 then .a else .b
    let sibling := 1 - leader
    ⟨s!"{bytes}-{leader}-retained-root", bytes,
      [.start ⟨grant, leader, false⟩, .publish, .finish leader, .retain leader,
        .start ⟨grant, leader, false⟩, .start ⟨grant, sibling, false⟩, .finish sibling, .release sibling]⟩

private def cases : List Case := matrix ++ readySiblingCases ++ retainedCases

private def actionJson (action : Action) : Lean.Json :=
  let requestJson (kind : String) (request : Request) := Lean.Json.mkObj
    [("kind", .str kind), ("grant", .str (grantName request.grant)),
      ("worker", Lean.toJson request.worker), ("fail", Lean.toJson request.fail)]
  match action with
  | .start request => requestJson "start" request
  | .queue request => requestJson "queue" request
  | .publish => Lean.Json.mkObj [("kind", .str "publish")]
  | .finish worker => Lean.Json.mkObj [("kind", .str "finish"), ("worker", Lean.toJson worker)]
  | .release worker => Lean.Json.mkObj [("kind", .str "release"), ("worker", Lean.toJson worker)]
  | .retain worker => Lean.Json.mkObj [("kind", .str "retain"), ("worker", Lean.toJson worker)]

private def observation (perWorker : Nat) (result : Result) : Lean.Json :=
  let state := result.state
  let locked := match state.binding with | .held .. => true | _ => false
  let reservation := match state.binding with | .published grant => some (grantName grant) | _ => none
  let copying := [0, 1].filter fun worker => match state.slot worker with | .copying _ => true | _ => false
  let ready := [0, 1].filter fun worker => decide (state.slot worker = .ready)
  let block := HoiminOracle.WorkspaceCapacity.blockBytes
  let pendingBytes := if perWorker > block then block else 0
  let diskBytes := (state.occupied.length - copying.length) * perWorker + copying.length * pendingBytes
  Lean.Json.mkObj
    [ ("outcome", .str (verdictName result.verdict)), ("queued_outcome", Lean.toJson (result.queued.map verdictName))
    , ("locked", Lean.toJson locked), ("reservation", Lean.toJson reservation)
    , ("registered", Lean.toJson (if locked then none else some state.occupied))
    , ("copying", Lean.toJson copying), ("ready", Lean.toJson ready)
    , ("allowance", Lean.toJson (state.allowance perWorker)), ("charged", Lean.toJson (state.charged perWorker))
    , ("worker_roots", Lean.toJson state.occupied.length), ("disk_bytes", Lean.toJson diskBytes) ]

def corpus : String :=
  String.join (cases.map fun item =>
    let expected := runWith step {} item.actions
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("per_worker", Lean.toJson item.perWorker), ("actions", .arr (item.actions.map actionJson).toArray)
      , ("expected", .arr (expected.map (observation item.perWorker)).toArray) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (State → Action → Result)) :=
    [ ("release_lock_before_explicit_publish", fun state action =>
        let result := step state action
        if result.verdict = .holdingLock then step result.state .publish else result)
    , ("accept_competing_reservation", fun state action =>
        match state.binding, action with
        | .held leader (some request), .publish =>
          if request.grant = competingGrant leader.grant then
            step { state with binding := .held leader (some { request with grant := leader.grant }) } action
          else step state action
        | _, _ => step state action)
    , ("accept_duplicate_under_lock", fun state action =>
        let result := step state action
        if result.queued = some .duplicate then { result with queued := some .copying } else result)
    , ("range_check_precedes_preflight", fun state action =>
        let result := step state action
        if result.queued = some .preflightMismatch then { result with queued := some .outOfRange } else result)
    , ("failure_releases_sibling", fun state action =>
        let result := step state action
        if result.verdict = .copyFailed then { result with state := { result.state with first := .vacant, second := .vacant } }
        else result)
    , ("release_retained_worker", fun state action => match action with
        | .retain worker => step state (.release worker)
        | _ => step state action)
    , ("unbind_after_last_worker", fun state action =>
        let result := step state action
        if (result.verdict = .released || result.verdict = .copyFailed) && result.state.occupied.isEmpty then
          { result with state := { result.state with binding := .unbound } }
        else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (runWith variant {} item.actions ≠ runWith step {} item.actions)) then
      IO.println s!"worker grant publication sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"worker grant publication missed variant: {name}"
      return 1
  IO.println s!"worker_grant_publication_cases={cases.length} transitions={(cases.map fun item => item.actions.length).sum} max_depth={(cases.map fun item => item.actions.length).foldl max 0}"
  return 0

end HoiminOracle.WorkerGrantPublication
