namespace HoiminOracle

inductive Phase
  | running
  | cleaning
  | finalPending
  | finished
  deriving Repr, DecidableEq

inductive EffectKind
  | ordinary
  | cleanup
  | finalOutput
  deriving Repr, DecidableEq

inductive StopCause
  | deadline
  | cancelled
  deriving Repr, DecidableEq

inductive Event
  | complete (id : Nat) (kind : EffectKind)
  | stop (cause : StopCause)
  deriving Repr, DecidableEq

inductive Rejection
  | unknown
  | duplicate
  | retired
  | wrongCompletion
  deriving Repr, DecidableEq

structure State where
  phase : Phase
  pending : List (Nat × EffectKind)
  completed : List Nat
  retired : List Nat
  stopCause : Option StopCause
  cleanupEmitted : Bool
  finalEmitted : Bool
  acceptedResults : Nat
  nextId : Nat
  deriving Repr, DecidableEq

namespace State

def initial : State where
  phase := .running
  pending := []
  completed := []
  retired := []
  stopCause := none
  cleanupEmitted := false
  finalEmitted := false
  acceptedResults := 0
  nextId := 1

def withPending (id : Nat) (kind : EffectKind) : State :=
  { initial with pending := [(id, kind)], nextId := id + 1 }

end State

structure Verdict where
  state : State
  emitted : List EffectKind
  rejection : Option Rejection
  deriving Repr, DecidableEq

namespace Verdict

def errorCode? (verdict : Verdict) : Option String :=
  match verdict.rejection with
  | none => none
  | some .unknown => some "machine.effect.unknown"
  | some .duplicate => some "machine.effect.duplicate"
  | some .retired => some "machine.effect.retired"
  | some .wrongCompletion => some "machine.effect.wrong_completion"

end Verdict

def pendingKind? (pending : List (Nat × EffectKind)) (id : Nat) : Option EffectKind :=
  match pending with
  | [] => none
  | (pendingId, kind) :: rest =>
      if pendingId = id then some kind else pendingKind? rest id

def removePending (pending : List (Nat × EffectKind)) (id : Nat) :
    List (Nat × EffectKind) :=
  pending.filter fun entry => entry.1 != id

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  emitted := []
  rejection := some reason

def acceptCompletion (state : State) (id : Nat) (kind : EffectKind) : Verdict :=
  let accepted := {
    state with
      pending := removePending state.pending id
      completed := id :: state.completed
  }
  match kind with
  | .ordinary =>
      match accepted.stopCause with
      | some _ =>
          { state := { accepted with acceptedResults := accepted.acceptedResults + 1 }
            emitted := []
            rejection := none }
      | none =>
          { state := {
              accepted with
                pending := [(accepted.nextId, .ordinary)]
                acceptedResults := accepted.acceptedResults + 1
                nextId := accepted.nextId + 1
            }
            emitted := [.ordinary]
            rejection := none }
  | .cleanup =>
      { state := {
          accepted with
            phase := .finalPending
            pending := [(accepted.nextId, .finalOutput)]
            finalEmitted := true
            nextId := accepted.nextId + 1
        }
        emitted := [.finalOutput]
        rejection := none }
  | .finalOutput =>
      { state := { accepted with phase := .finished }
        emitted := []
        rejection := none }

def complete (state : State) (id : Nat) (kind : EffectKind) : Verdict :=
  if id ∈ state.retired then
    reject state .retired
  else if id ∈ state.completed then
    reject state .duplicate
  else
    match pendingKind? state.pending id with
    | none => reject state .unknown
    | some expected =>
        if expected = kind then acceptCompletion state id kind
        else reject state .wrongCompletion

def stop (state : State) (cause : StopCause) : Verdict :=
  match state.phase with
  | .cleaning | .finalPending | .finished =>
      { state := state, emitted := [], rejection := none }
  | .running =>
      match state.stopCause with
      | some _ => { state := state, emitted := [], rejection := none }
      | none =>
          { state := {
              state with
                phase := .cleaning
                pending := [(state.nextId, .cleanup)]
                retired := state.pending.map Prod.fst ++ state.retired
                stopCause := some cause
                cleanupEmitted := true
                nextId := state.nextId + 1
            }
            emitted := [.cleanup]
            rejection := none }

def step (state : State) (event : Event) : Verdict :=
  match event with
  | .complete id kind => complete state id kind
  | .stop cause => stop state cause

end HoiminOracle
