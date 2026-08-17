import HoiminOracle.Model
import Lean.Data.Json

namespace HoiminOracle

example : (step State.initial (.complete 99 .ordinary)).errorCode? =
    some "machine.effect.unknown" := by decide

example :
    let state := State.withPending 1 .ordinary
    (step state (.complete 1 .cleanup)).errorCode? =
      some "machine.effect.wrong_completion" := by decide

example :
    let state := State.withPending 1 .ordinary
    let accepted := (step state (.complete 1 .ordinary)).state
    (step accepted (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.duplicate" := by decide

example :
    let state := State.withPending 1 .ordinary
    let stopped := (step state (.stop .cancelled)).state
    (step stopped (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.retired" := by decide

example :
    let state := State.withPending 1 .ordinary
    (step state (.complete 1 .ordinary)).emitted = [.ordinary] := by decide

def brokenAcceptDuplicate (state : State) (id : Nat) (kind : EffectKind) : Verdict :=
  acceptCompletion state id kind

def duplicateWitnessDetected : Bool :=
  let state := { State.initial with completed := [1] }
  decide ((step state (.complete 1 .ordinary)).errorCode? ≠
    (brokenAcceptDuplicate state 1 .ordinary).errorCode?)

def brokenLateStop (state : State) (cause : StopCause) : Verdict where
  state := { state with stopCause := some cause }
  emitted := []
  rejection := none

def lateStopWitnessDetected : Bool :=
  let state := {
    State.initial with
      phase := .finalPending
      stopCause := some .cancelled
      finalEmitted := true
  }
  decide ((step state (.stop .deadline)).state ≠
    (brokenLateStop state .deadline).state)

def brokenStopScheduling (state : State) : Verdict where
  state := state
  emitted := [.ordinary]
  rejection := none

def stopSchedulingWitnessDetected : Bool :=
  let state := { State.initial with stopCause := some .cancelled }
  decide ((step state (.stop .deadline)).emitted ≠
    (brokenStopScheduling state).emitted)

def brokenRepeatedStop (state : State) : Verdict where
  state := state
  emitted := [.cleanup]
  rejection := none

def repeatedCleanupWitnessDetected : Bool :=
  let state := (step pendingWitnessState (.stop .cancelled)).state
  decide ((step state (.stop .deadline)).emitted ≠ (brokenRepeatedStop state).emitted)
where
  pendingWitnessState := State.withPending 1 .ordinary

def brokenCleaningStop (state : State) (cause : StopCause) : Verdict where
  state := {
    state with
      pending := [(state.nextId, .cleanup)]
      retired := state.pending.map Prod.fst ++ state.retired
      stopCause := some cause
      cleanupEmitted := true
      nextId := state.nextId + 1
  }
  emitted := [.cleanup]
  rejection := none

def normalCleaningStopWitnessDetected : Bool :=
  let state := {
    State.initial with
      phase := .cleaning
      pending := [(1, .cleanup)]
      cleanupEmitted := true
      nextId := 2
  }
  decide ((step state (.stop .deadline)) ≠ (brokenCleaningStop state .deadline))

def brokenCleanupOrdering (state : State) : Verdict where
  state := { state with phase := .finalPending, finalEmitted := true }
  emitted := [.finalOutput]
  rejection := none

def cleanupOrderingWitnessDetected : Bool :=
  let state := State.withPending 1 .ordinary
  decide ((step state (.stop .cancelled)).emitted ≠ (brokenCleanupOrdering state).emitted)

def brokenStopDropsResults (state : State) : Verdict :=
  let verdict := step state (.stop .cancelled)
  { verdict with state := { verdict.state with acceptedResults := 0 } }

def acceptedResultWitnessDetected : Bool :=
  let state := State.withPending 1 .ordinary
  let accepted := (step state (.complete 1 .ordinary)).state
  decide ((step accepted (.stop .cancelled)).state.acceptedResults ≠
    (brokenStopDropsResults accepted).state.acceptedResults)

def brokenWitnessesDetected : Bool :=
  duplicateWitnessDetected && lateStopWitnessDetected && stopSchedulingWitnessDetected &&
    repeatedCleanupWitnessDetected && normalCleaningStopWitnessDetected &&
    cleanupOrderingWitnessDetected &&
    acceptedResultWitnessDetected

example : brokenWitnessesDetected = true := by decide

structure OracleStep where
  event : String
  verdict : String
  errorCode : Option String
  phase : String
  emitted : List String
  pending : Nat
  deriving Repr, DecidableEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  schedule : List String
  expected : List OracleStep
  deriving Repr, DecidableEq

private structure NamedEvent where
  name : String
  event : Event

private structure CaseSpec where
  id : String
  mode : String := "report"
  scenario : String
  initial : State
  schedule : List NamedEvent

def phaseName : Phase → String
  | .running => "running"
  | .cleaning => "cleaning"
  | .finalPending => "final_pending"
  | .finished => "finished"

def effectName : EffectKind → String
  | .ordinary => "ordinary"
  | .cleanup => "cleanup"
  | .finalOutput => "final_output"

private def observe (name : String) (verdict : Verdict) : OracleStep where
  event := name
  verdict := if verdict.rejection.isSome then "rejected" else "accepted"
  errorCode := verdict.errorCode?
  phase := phaseName verdict.state.phase
  emitted := verdict.emitted.map effectName
  pending := verdict.state.pending.length

private def runSchedule : State → List NamedEvent → List OracleStep
  | _, [] => []
  | state, named :: rest =>
      let verdict := step state named.event
      observe named.name verdict :: runSchedule verdict.state rest

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  mode := spec.mode
  scenario := spec.scenario
  schedule := spec.schedule.map NamedEvent.name
  expected := runSchedule spec.initial spec.schedule

private def pendingOrdinary : State := State.withPending 1 .ordinary

private def cleaning : State :=
  (step pendingOrdinary (.stop .cancelled)).state

private def normalCleaning : State := {
  State.initial with
    phase := .cleaning
    pending := [(1, .cleanup)]
    cleanupEmitted := true
    nextId := 2
}

private def finalPending : State :=
  (step cleaning (.complete 2 .cleanup)).state

private def finished : State :=
  (step finalPending (.complete 3 .finalOutput)).state

private def named (name : String) (event : Event) : NamedEvent := { name, event }

private def strict : String := "strict"

private def specs : List CaseSpec := [
  { id := "unknown_completion_is_rejected"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [named "complete_unknown" (.complete 99 .ordinary)] },
  { id := "wrong_kind_is_transactional"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [named "complete_wrong_cleanup" (.complete 1 .cleanup)] },
  { id := "duplicate_completion_is_rejected"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "complete_ordinary" (.complete 1 .ordinary),
      named "complete_ordinary_again" (.complete 1 .ordinary)
    ] },
  { id := "retired_completion_after_cancel_is_rejected"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "cancel" (.stop .cancelled),
      named "complete_retired_ordinary" (.complete 1 .ordinary)
    ] },
  { id := "deadline_stops_ordinary_scheduling"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [named "deadline" (.stop .deadline)] },
  { id := "cancel_stops_ordinary_scheduling"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [named "cancel" (.stop .cancelled)] },
  { id := "cleanup_precedes_final_output"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "cancel" (.stop .cancelled),
      named "complete_cleanup" (.complete 2 .cleanup)
    ] },
  { id := "cleanup_is_emitted_once"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "cancel" (.stop .cancelled),
      named "deadline" (.stop .deadline)
    ] },
  { id := "deadline_during_normal_cleanup_is_noop"
    mode := strict
    scenario := "normal_cleaning_with_copy"
    initial := normalCleaning
    schedule := [
      named "deadline" (.stop .deadline),
      named "complete_cleanup" (.complete 1 .cleanup)
    ] },
  { id := "cancel_during_normal_cleanup_is_noop"
    mode := strict
    scenario := "normal_cleaning_with_copy"
    initial := normalCleaning
    schedule := [
      named "cancel" (.stop .cancelled),
      named "complete_cleanup" (.complete 1 .cleanup)
    ] },
  { id := "final_output_is_emitted_once"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "cancel" (.stop .cancelled),
      named "complete_cleanup" (.complete 2 .cleanup),
      named "deadline" (.stop .deadline)
    ] },
  { id := "deadline_after_final_pending_is_noop"
    mode := strict
    scenario := "final_pending_without_copy"
    initial := finalPending
    schedule := [named "deadline" (.stop .deadline)] },
  { id := "cancel_after_final_pending_is_noop"
    mode := strict
    scenario := "final_pending_without_copy"
    initial := finalPending
    schedule := [named "cancel" (.stop .cancelled)] },
  { id := "deadline_after_finished_is_noop"
    mode := strict
    scenario := "finished_without_copy"
    initial := finished
    schedule := [named "deadline" (.stop .deadline)] },
  { id := "cancel_after_finished_is_noop"
    mode := strict
    scenario := "finished_without_copy"
    initial := finished
    schedule := [named "cancel" (.stop .cancelled)] },
  { id := "ordinary_completion_then_cancel"
    mode := strict
    scenario := "pending_resolve"
    initial := pendingOrdinary
    schedule := [
      named "complete_ordinary" (.complete 1 .ordinary),
      named "cancel" (.stop .cancelled)
    ] }
]

def cases : List OracleCase := specs.map toOracleCase

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def optionalStringJson : Option String → Lean.Json
  | none => .null
  | some value => .str value

def oracleStepJson (item : OracleStep) : Lean.Json := Lean.Json.mkObj [
  ("event", .str item.event),
  ("verdict", .str item.verdict),
  ("error_code", optionalStringJson item.errorCode),
  ("phase", .str item.phase),
  ("emitted", stringsJson item.emitted),
  ("pending", Lean.toJson item.pending)
]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("scenario", .str item.scenario),
  ("schedule", stringsJson item.schedule),
  ("expected", .arr (item.expected.toArray.map oracleStepJson))
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

end HoiminOracle
