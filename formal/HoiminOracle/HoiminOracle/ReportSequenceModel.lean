import Std

namespace HoiminOracle.ReportSequence

inductive RunId
  | first
  | second
  deriving Repr, DecidableEq, BEq

inductive MutantId
  | alpha
  | beta
  deriving Repr, DecidableEq, BEq

inductive Status
  | survived
  | killed
  | timeout
  | outOfMemory
  | processLimit
  | error
  | notRun
  deriving Repr, DecidableEq, BEq

inductive Termination
  | exitZero
  | exitNonzero
  | timeout
  | outOfMemory
  | processLimit
  | cancelled
  deriving Repr, DecidableEq, BEq

inductive EventKind
  | runStarted
  | mutantStarted (id : MutantId) (mutantSequence : Nat)
  | mutantFinished (id : MutantId) (mutantSequence : Nat)
      (status : Status) (termination : Option Termination)
  | diagnostic
  | runFinished
  deriving Repr, DecidableEq, BEq

structure Event where
  runId : RunId
  sequence : Nat
  kind : EventKind
  deriving Repr, DecidableEq, BEq

inductive Rejection
  | runNotStarted
  | runAlreadyStarted (runId : RunId)
  | runAlreadyFinished (runId : RunId)
  | runFinishedWithActiveMutants (count : Nat)
  | runIdMismatch (expected received : RunId)
  | notMonotonic (previous received : Nat)
  | mutantAlreadyStarted (id : MutantId) (mutantSequence : Nat)
  | duplicateMutantIdentity (id : MutantId) (mutantSequence : Nat)
  | mutantIdentitySequenceMismatch
      (id : MutantId) (expectedSequence receivedSequence : Nat)
  | mutantNotStarted (id : MutantId) (mutantSequence : Nat)
  | mutantStatusTerminationMismatch
      (id : MutantId) (mutantSequence : Nat)
      (status expectedStatus : Status) (termination : Termination)
  deriving Repr, DecidableEq, BEq

structure State where
  last : Option Nat
  runId : Option RunId
  active : List (MutantId × Nat)
  seen : List (MutantId × Nat)
  finished : Bool
  deriving Repr, DecidableEq, BEq

structure Verdict where
  state : State
  rejection : Option Rejection
  deriving Repr, DecidableEq, BEq

def initial : State where
  last := none
  runId := none
  active := []
  seen := []
  finished := false

def classify : Termination → Status
  | .exitZero => .survived
  | .exitNonzero => .killed
  | .timeout => .timeout
  | .outOfMemory => .outOfMemory
  | .processLimit => .processLimit
  | .cancelled => .notRun

def seenSequence? : List (MutantId × Nat) → MutantId → Option Nat
  | [], _ => none
  | (seenId, sequence) :: rest, id =>
      if seenId = id then some sequence else seenSequence? rest id

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  rejection := some reason

def lifecycleError? (state : State) (event : Event) : Option Rejection :=
  if state.finished then
    some (.runAlreadyFinished (state.runId.getD event.runId))
  else
    match event.kind, state.runId with
    | .runStarted, some _ => some (.runAlreadyStarted event.runId)
    | .runStarted, none => none
    | _, none => some .runNotStarted
    | _, some expected =>
        if event.runId = expected then none
        else some (.runIdMismatch expected event.runId)

def mutantError? (state : State) (event : Event) : Option Rejection :=
  match event.kind with
  | .mutantStarted id mutantSequence =>
      if state.active.contains (id, mutantSequence) then
        some (.mutantAlreadyStarted id mutantSequence)
      else
        match seenSequence? state.seen id with
        | some expectedSequence =>
            if expectedSequence = mutantSequence then
              some (.duplicateMutantIdentity id mutantSequence)
            else
              some (.mutantIdentitySequenceMismatch id expectedSequence mutantSequence)
        | none => none
  | .mutantFinished id mutantSequence status termination =>
      match seenSequence? state.seen id with
      | some expectedSequence =>
          if expectedSequence ≠ mutantSequence then
            some (.mutantIdentitySequenceMismatch id expectedSequence mutantSequence)
          else if !state.active.contains (id, mutantSequence) then
            some (.mutantNotStarted id mutantSequence)
          else
            match termination with
            | some value =>
                let expectedStatus := classify value
                if status = expectedStatus then none
                else some (.mutantStatusTerminationMismatch
                  id mutantSequence status expectedStatus value)
            | none => none
      | none => some (.mutantNotStarted id mutantSequence)
  | .runFinished =>
      if state.active.isEmpty then none
      else some (.runFinishedWithActiveMutants state.active.length)
  | .runStarted | .diagnostic => none

def accept (state : State) (event : Event) : State :=
  let advanced := { state with last := some event.sequence }
  match event.kind with
  | .runStarted => { advanced with runId := some event.runId }
  | .mutantStarted id mutantSequence =>
      { advanced with
          active := (id, mutantSequence) :: state.active
          seen := (id, mutantSequence) :: state.seen }
  | .mutantFinished id mutantSequence _ _ =>
      { advanced with active := state.active.erase (id, mutantSequence) }
  | .runFinished => { advanced with finished := true }
  | .diagnostic => advanced

def step (state : State) (event : Event) : Verdict :=
  match lifecycleError? state event with
  | some reason => reject state reason
  | none =>
      match mutantError? state event with
      | some reason => reject state reason
      | none =>
          match state.last with
          | some previous =>
              if event.sequence ≤ previous then
                reject state (.notMonotonic previous event.sequence)
              else { state := accept state event, rejection := none }
          | none => { state := accept state event, rejection := none }

def run : State → List Event → State
  | state, [] => state
  | state, event :: rest => run (step state event).state rest

end HoiminOracle.ReportSequence
