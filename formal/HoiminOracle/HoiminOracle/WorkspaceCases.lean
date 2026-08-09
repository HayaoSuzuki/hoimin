import HoiminOracle.WorkspaceProofs
import Lean.Data.Json

namespace HoiminOracle.WorkspaceAudit

structure OracleStep where
  event : String
  verdict : String
  errorCode : Option String
  active : List Slot
  pending : List Slot
  taskOwned : List Slot
  released : Bool
  deriving Repr, DecidableEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  schedule : List String
  expected : List OracleStep
  deriving Repr, DecidableEq

private structure NamedEvent where
  name : String
  events : List Event

private structure CaseSpec where
  id : String
  mode : String
  schedule : List NamedEvent

private def named (name : String) (event : Event) : NamedEvent :=
  { name, events := [event] }

private def composite (name : String) (events : List Event) : NamedEvent :=
  { name, events }

private def taskOwnedSlots (state : State) : List Slot :=
  state.tasks.filterMap taskSlot?

private def runEvents : State → List Event → State × Option Rejection
  | state, [] => (state, none)
  | state, event :: rest =>
      let verdict := step state event
      match rest with
      | [] => (verdict.state, verdict.rejection)
      | _ => runEvents verdict.state rest

private def observe
    (name : String)
    (state : State)
    (rejection : Option Rejection) : OracleStep where
  event := name
  verdict := if rejection.isSome then "rejected" else "accepted"
  errorCode := rejection.map Rejection.code
  active := state.active
  pending := state.pending
  taskOwned := taskOwnedSlots state
  released := state.released

private def runSchedule : State → List NamedEvent → List OracleStep
  | _, [] => []
  | state, item :: rest =>
      let result := runEvents state item.events
      observe item.name result.1 result.2 :: runSchedule result.1 rest

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  mode := spec.mode
  schedule := spec.schedule.map NamedEvent.name
  expected := runSchedule State.initial spec.schedule

private def syncApply (task : TaskId) (worker : Worker) : List Event := [
  .prepare task .apply worker .g0,
  .execute task .active,
  .accept task
]

private def syncReset (task : TaskId) (worker : Worker) : List Event := [
  .prepare task .reset worker .g0,
  .execute task .active,
  .accept task
]

private def specs : List CaseSpec := [
  { id := "public_create_cleanup"
    mode := "strict"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      named "cleanup:success" .cleanupSuccess
    ] },
  { id := "public_duplicate_create_is_atomic"
    mode := "strict"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      named "create:w0:g1" (.seed .w0 .g1)
    ] },
  { id := "public_apply_reset_round_trip"
    mode := "strict"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      composite "sync_apply:w0" (syncApply .t0 .w0),
      composite "sync_reset:w0" (syncReset .t1 .w0)
    ] },
  { id := "owned_apply_returns_generation"
    mode := "internal-fixture"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      named "prepare:t0:apply:w0" (.prepare .t0 .apply .w0 .g0),
      named "execute:t0:active" (.execute .t0 .active),
      named "accept:t0" (.accept .t0)
    ] },
  { id := "cleanup_rejects_inflight_task"
    mode := "model-only"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      named "prepare:t0:apply:w0" (.prepare .t0 .apply .w0 .g0),
      named "cleanup:success" .cleanupSuccess
    ] },
  { id := "owned_reset_returns_generation"
    mode := "internal-fixture"
    schedule := [
      named "preflight" .preflight,
      named "create:w0:g0" (.seed .w0 .g0),
      named "prepare:t0:reset:w0" (.prepare .t0 .reset .w0 .g0),
      named "execute:t0:active" (.execute .t0 .active),
      named "accept:t0" (.accept .t0)
    ] }
]

def cases : List OracleCase := specs.map toOracleCase

private def workerName : Worker → String
  | .w0 => "w0"
  | .w1 => "w1"

private def generationName : Generation → String
  | .g0 => "g0"
  | .g1 => "g1"

private def slotJson (slot : Slot) : Lean.Json := Lean.Json.mkObj [
  ("worker", .str (workerName slot.worker)),
  ("generation", .str (generationName slot.generation))
]

private def slotsJson (slots : List Slot) : Lean.Json :=
  .arr (slots.toArray.map slotJson)

private def optionalStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

def oracleStepJson (item : OracleStep) : Lean.Json := Lean.Json.mkObj [
  ("event", .str item.event),
  ("verdict", .str item.verdict),
  ("error_code", optionalStringJson item.errorCode),
  ("active", slotsJson item.active),
  ("pending", slotsJson item.pending),
  ("task_owned", slotsJson item.taskOwned),
  ("released", Lean.toJson item.released)
]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("schedule", stringsJson item.schedule),
  ("expected", .arr (item.expected.toArray.map oracleStepJson))
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

end HoiminOracle.WorkspaceAudit
