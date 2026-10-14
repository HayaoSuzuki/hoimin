import Std

-- The module name is retained for audit continuity. The live contract is direct
-- initialization of a fresh database and rejection of unsupported versions.
namespace HoiminOracle.SchemaMigration

inductive Version
  | fresh | v1 | v2 | v3 | current | future
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Actor
  | left | right
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Phase
  | ready
  | waiting (observed : Version)
  | locked
  | succeeded
  | unsupportedRejected
  | initializationFailed
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Event
  | observe (actor : Actor)
  | begin (actor : Actor)
  | initializeSchema (actor : Actor)
  | commit (actor : Actor)
  | fail (actor : Actor)
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure State where
  version : Version
  legacyRow : Bool
  lock : Option Actor
  txVersion : Option Version
  leftPhase : Phase
  rightPhase : Phase
  deriving Repr, DecidableEq, BEq

def initial (version : Version) (legacyRow : Bool) : State := {
  version
  legacyRow
  lock := none
  txVersion := none
  leftPhase := .ready
  rightPhase := .ready
}

def phase (state : State) : Actor → Phase
  | .left => state.leftPhase
  | .right => state.rightPhase

def setPhase (state : State) (actor : Actor) (next : Phase) : State :=
  match actor with
  | .left => { state with leftPhase := next }
  | .right => { state with rightPhase := next }

def observe (state : State) (actor : Actor) : State :=
  match phase state actor with
  | .ready =>
      if state.version == .current then setPhase state actor .succeeded
      else setPhase state actor (.waiting state.version)
  | _ => state

-- Re-read the committed version after obtaining the writer transaction.
def begin (state : State) (actor : Actor) : State :=
  match phase state actor, state.lock with
  | .waiting _, none =>
      match state.version with
      | .fresh | .current =>
          setPhase { state with lock := some actor, txVersion := some state.version } actor .locked
      | .v1 | .v2 | .v3 | .future => setPhase state actor .unsupportedRejected
  | _, _ => state

def releaseFailed (state : State) (actor : Actor) : State :=
  setPhase { state with lock := none, txVersion := none } actor .initializationFailed

def initializeSchema (state : State) (actor : Actor) : State :=
  if state.lock == some actor && phase state actor == .locked then
    match state.txVersion with
    | some .fresh =>
        if state.version == .fresh then { state with txVersion := some .current }
        else releaseFailed state actor
    | _ => state
  else state

def commit (state : State) (actor : Actor) : State :=
  if state.lock == some actor && phase state actor == .locked then
    match state.txVersion with
    | some .current =>
        match state.version with
        | .fresh | .current =>
            setPhase {
              state with version := .current, lock := none, txVersion := none
            } actor .succeeded
        | .v1 | .v2 | .v3 | .future => releaseFailed state actor
    | _ => state
  else state

def fail (state : State) (actor : Actor) : State :=
  if state.lock == some actor && phase state actor == .locked then
    releaseFailed state actor
  else state

def step (state : State) : Event → State
  | .observe actor => observe state actor
  | .begin actor => begin state actor
  | .initializeSchema actor => initializeSchema state actor
  | .commit actor => commit state actor
  | .fail actor => fail state actor

def run : State → List Event → State
  | state, [] => state
  | state, event :: rest => run (step state event) rest

def Invariant (state : State) : Prop :=
  (state.leftPhase = .succeeded → state.version = .current) ∧
  (state.rightPhase = .succeeded → state.version = .current)

end HoiminOracle.SchemaMigration
