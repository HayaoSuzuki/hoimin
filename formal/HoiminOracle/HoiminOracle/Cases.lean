import HoiminOracle.Model

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

def brokenWitnessesDetected : Bool :=
  duplicateWitnessDetected && lateStopWitnessDetected && stopSchedulingWitnessDetected

example : brokenWitnessesDetected = true := by decide

end HoiminOracle
