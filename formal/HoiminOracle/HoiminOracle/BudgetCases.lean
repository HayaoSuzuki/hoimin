import HoiminOracle.BudgetProofs
import Lean.Data.Json

namespace HoiminOracle.BudgetAudit

structure OracleStep where
  event : String
  verdict : String
  errorCode : Option String
  allocated : Option Nat
  active : List Entry
  released : List Nat
  totals : Limits
  deriving Repr, DecidableEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String := "strict"
  limits : Limits
  maxId : Nat
  schedule : List String
  expected : List OracleStep
  deriving Repr, DecidableEq

private structure NamedEvent where
  name : String
  event : Event

private structure CaseSpec where
  id : String
  limits : Limits
  maxId : Nat := 2
  schedule : List NamedEvent

private def named (name : String) (event : Event) : NamedEvent := { name, event }

private def observe (name : String) (verdict : Verdict) : OracleStep where
  event := name
  verdict := if verdict.rejection.isSome then "rejected" else "accepted"
  errorCode := verdict.errorCode?
  allocated := verdict.allocated
  active := verdict.state.active
  released := verdict.state.released
  totals := {
    memory := total verdict.state .memory
    copy := total verdict.state .copy
    processes := total verdict.state .processes
  }

private def runSchedule : State → List NamedEvent → List OracleStep
  | _, [] => []
  | state, item :: rest =>
      let verdict := step state item.event
      observe item.name verdict :: runSchedule verdict.state rest

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  limits := spec.limits
  maxId := spec.maxId
  schedule := spec.schedule.map NamedEvent.name
  expected := runSchedule (State.initial spec.limits spec.maxId) spec.schedule

private def limits (memory copy processes : Nat) : Limits :=
  { memory, copy, processes }

private def specs : List CaseSpec := [
  { id := "budget_kinds_are_independent"
    limits := limits 1 1 1
    schedule := [
      named "reserve:memory:1" (.reserve .memory 1),
      named "reserve:copy:1" (.reserve .copy 1),
      named "reserve:processes:1" (.reserve .processes 1)
    ] },
  { id := "limit_precedes_identifier_exhaustion"
    limits := limits 1 1 1
    maxId := 0
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "reserve:copy:1" (.reserve .copy 1)
    ] },
  { id := "last_identifier_then_exhaustion"
    limits := limits 2 2 2
    maxId := 0
    schedule := [
      named "reserve:copy:0" (.reserve .copy 0),
      named "reserve:copy:0" (.reserve .copy 0)
    ] },
  { id := "empty_cleanup_is_accepted"
    limits := limits 1 1 1
    schedule := [named "release:" (.release [])] },
  { id := "singleton_cleanup_releases_exactly_one"
    limits := limits 2 2 2
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "release:0" (.release [0])
    ] },
  { id := "reversed_two_identifier_cleanup"
    limits := limits 2 2 2
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "reserve:memory:1" (.reserve .memory 1),
      named "release:1,0" (.release [1, 0])
    ] },
  { id := "duplicate_cleanup_is_atomic"
    limits := limits 2 2 2
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "release:0,0" (.release [0, 0])
    ] },
  { id := "mixed_unknown_cleanup_is_atomic"
    limits := limits 2 2 2
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "release:0,2" (.release [0, 2])
    ] },
  { id := "mixed_released_cleanup_is_atomic"
    limits := limits 2 2 2
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "reserve:copy:1" (.reserve .copy 1),
      named "release:0" (.release [0]),
      named "release:1,0" (.release [1, 0])
    ] },
  { id := "second_cleanup_is_rejected"
    limits := limits 1 1 1
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "release:0" (.release [0]),
      named "release:0" (.release [0])
    ] },
  { id := "zero_amount_reservation_uses_an_identifier"
    limits := limits 0 0 0
    schedule := [named "reserve:memory:0" (.reserve .memory 0)] },
  { id := "cleanup_after_allocator_exhaustion"
    limits := limits 2 2 2
    maxId := 0
    schedule := [
      named "reserve:copy:1" (.reserve .copy 1),
      named "reserve:copy:0" (.reserve .copy 0),
      named "release:0" (.release [0])
    ] }
]

def cases : List OracleCase := specs.map toOracleCase

private def optionalNatJson : Option Nat → Lean.Json
  | none => .null
  | some value => Lean.toJson value

private def limitsJson (value : Limits) : Lean.Json := Lean.Json.mkObj [
  ("memory", Lean.toJson value.memory),
  ("copy", Lean.toJson value.copy),
  ("processes", Lean.toJson value.processes)
]

private def kindName : Kind → String
  | .memory => "memory"
  | .copy => "copy"
  | .processes => "processes"

private def entryJson (entry : Entry) : Lean.Json := Lean.Json.mkObj [
  ("id", Lean.toJson entry.id),
  ("kind", .str (kindName entry.kind)),
  ("amount", Lean.toJson entry.amount)
]

private def entriesJson (entries : List Entry) : Lean.Json :=
  .arr (entries.toArray.map entryJson)

private def natsJson (values : List Nat) : Lean.Json :=
  .arr (values.toArray.map Lean.toJson)

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def optionalStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

def oracleStepJson (item : OracleStep) : Lean.Json := Lean.Json.mkObj [
  ("event", .str item.event),
  ("verdict", .str item.verdict),
  ("error_code", optionalStringJson item.errorCode),
  ("allocated", optionalNatJson item.allocated),
  ("active", entriesJson item.active),
  ("released", natsJson item.released),
  ("totals", limitsJson item.totals)
]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("limits", limitsJson item.limits),
  ("max_id", Lean.toJson item.maxId),
  ("schedule", stringsJson item.schedule),
  ("expected", .arr (item.expected.toArray.map oracleStepJson))
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

end HoiminOracle.BudgetAudit
