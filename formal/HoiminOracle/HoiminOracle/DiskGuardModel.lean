import Std

namespace HoiminOracle.DiskGuard

inductive StopReason where
  | sizeExceeded | reserveReached | measurementFailed | processFailed
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

abbrev RootId := Fin 2

def executionRoot : RootId := ⟨0, by decide⟩
def deliveryRoot : RootId := ⟨1, by decide⟩

inductive ComponentState where
  | pending | active | succeeded | failed
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

structure State where
  stop : Option StopReason := none
  secondaryStops : List StopReason := []
  active : Nat := 0
  dispatched : Nat := 0
  ownedRoots : List RootId := []
  deliveryRoots : List RootId := []
  cleanupRequested : List RootId := []
  cleanupClean : List RootId := []
  cleanupFailed : List RootId := []
  cleanupDeferred : List RootId := []
  cleanupRetained : List RootId := []
  processDrain : ComponentState := .pending
  outputDrain : ComponentState := .pending
  monitorJoin : ComponentState := .pending
  report : ComponentState := .pending
  finished : Bool := false
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

inductive Event where
  | dispatch
  | observe (owned maxOwned free minFree : Nat)
  | meterFailed
  | processDrainFailed
  | processDrainSucceeded
  | requestCleanup (root : RootId)
  | cleanupSucceeded (root : RootId)
  | cleanupFailed (root : RootId)
  | cleanupDeferred (root : RootId)
  | cleanupRetained (root : RootId)
  | outputDrained
  | outputDrainFailed
  | monitorJoined
  | monitorJoinFailed
  | reportSucceeded
  | reportFailed
  | finish
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

def State.initial (ownedRoots : List RootId := [])
    (deliveryRoots : List RootId := []) (active : Nat := 0) : State :=
  { ownedRoots, deliveryRoots, active }

def componentSettled : ComponentState → Bool
  | .succeeded | .failed => true
  | .pending | .active => false

def safetyComponentsSettled (state : State) : Bool :=
  componentSettled state.processDrain && componentSettled state.outputDrain &&
    componentSettled state.monitorJoin

def safetyComponentsSucceeded (state : State) : Bool :=
  state.processDrain == .succeeded && state.outputDrain == .succeeded &&
    state.monitorJoin == .succeeded

def reportSettled (state : State) : Bool := componentSettled state.report

def rootsSubset (left right : List RootId) : Bool :=
  left.all fun root => right.contains root

def rootsDisjoint (left right : List RootId) : Bool :=
  left.all fun root => !right.contains root

def outcomeRecorded (state : State) (root : RootId) : Bool :=
  state.cleanupClean.contains root || state.cleanupFailed.contains root ||
    state.cleanupDeferred.contains root || state.cleanupRetained.contains root

def terminalOutcomeRecorded (state : State) (root : RootId) : Bool :=
  state.cleanupClean.contains root || state.cleanupFailed.contains root ||
    state.cleanupRetained.contains root

def collectionInvariant (state : State) : Bool :=
  decide state.secondaryStops.Nodup && decide state.ownedRoots.Nodup &&
    decide state.deliveryRoots.Nodup && decide state.cleanupRequested.Nodup &&
    decide state.cleanupClean.Nodup && decide state.cleanupFailed.Nodup &&
    decide state.cleanupDeferred.Nodup && decide state.cleanupRetained.Nodup

def ownershipInvariant (state : State) : Bool :=
  rootsSubset state.deliveryRoots state.ownedRoots &&
    rootsSubset state.cleanupRequested state.ownedRoots &&
    rootsSubset state.cleanupClean state.cleanupRequested &&
    rootsSubset state.cleanupFailed state.cleanupRequested &&
    rootsSubset state.cleanupDeferred state.cleanupRequested &&
    rootsSubset state.cleanupRetained state.cleanupRequested

def cleanupInvariant (state : State) : Bool :=
  rootsDisjoint state.cleanupClean state.cleanupFailed &&
    rootsDisjoint state.cleanupClean state.cleanupDeferred &&
    rootsDisjoint state.cleanupClean state.cleanupRetained &&
    rootsDisjoint state.cleanupFailed state.cleanupDeferred &&
    rootsDisjoint state.cleanupFailed state.cleanupRetained &&
    rootsDisjoint state.cleanupDeferred state.cleanupRetained &&
    (state.cleanupRequested.isEmpty || safetyComponentsSettled state) &&
    (state.cleanupRequested.all fun root =>
      !state.deliveryRoots.contains root || reportSettled state) &&
    (state.cleanupClean.isEmpty || safetyComponentsSucceeded state) &&
    (state.cleanupFailed.isEmpty || safetyComponentsSucceeded state)

def finishInvariant (state : State) : Bool :=
  (!state.finished ||
      (state.cleanupDeferred.isEmpty &&
        state.ownedRoots.all (terminalOutcomeRecorded state))) &&
    (!state.finished || (state.active == 0 && safetyComponentsSettled state)) &&
    (!state.finished || state.report == .succeeded) &&
    (!state.finished ||
      state.deliveryRoots.all fun root => state.cleanupClean.contains root)

def Invariant (state : State) : Bool :=
  collectionInvariant state && ownershipInvariant state &&
    cleanupInvariant state && finishInvariant state

def TransitionInvariant (start candidate : State) : Bool :=
  !start.stop.isSome ||
    (candidate.stop == start.stop && candidate.dispatched == start.dispatched)

def addUnique [BEq α] (value : α) (values : List α) : List α :=
  if values.contains value then values else values ++ [value]

def removeRoot (root : RootId) (roots : List RootId) : List RootId :=
  roots.filter (· != root)

def addSecondary (state : State) (reason : StopReason) : State :=
  if state.stop == some reason || state.secondaryStops.contains reason then state
  else { state with secondaryStops := state.secondaryStops ++ [reason] }

def recordReasons : State → List StopReason → State
  | state, [] => state
  | state, reason :: rest =>
      let next := match state.stop with
        | none => { state with stop := some reason }
        | some _ => addSecondary state reason
      recordReasons next rest

def observationReasons (owned maxOwned free minFree : Nat) : List StopReason :=
  if free ≤ minFree then
    if owned ≥ maxOwned then [.reserveReached, .sizeExceeded] else [.reserveReached]
  else if owned ≥ maxOwned then [.sizeExceeded]
  else []

def allSafetySucceeded (state : State) : Bool :=
  state.processDrain == .succeeded && state.outputDrain == .succeeded &&
    state.monitorJoin == .succeeded

def allSafetySettled (state : State) : Bool :=
  (state.processDrain == .succeeded || state.processDrain == .failed)
    && (state.outputDrain == .succeeded || state.outputDrain == .failed)
    && (state.monitorJoin == .succeeded || state.monitorJoin == .failed)

def anyOutcomeRecorded (state : State) (root : RootId) : Bool :=
  state.cleanupClean.contains root || state.cleanupFailed.contains root ||
    state.cleanupDeferred.contains root || state.cleanupRetained.contains root

def accept (candidate : State) : Option State :=
  if Invariant candidate then some candidate else none

def acceptStep (start candidate : State) : Option State :=
  if Invariant candidate && TransitionInvariant start candidate then some candidate else none

def transition (state : State) (event : Event) : Option State :=
  if state.finished then none else
  match event with
  | .dispatch =>
      if state.stop.isSome || state.processDrain != .pending then none
      else some { state with active := state.active + 1, dispatched := state.dispatched + 1 }
  | .observe owned maxOwned free minFree =>
      some (recordReasons state (observationReasons owned maxOwned free minFree))
  | .meterFailed => some (recordReasons state [.measurementFailed])
  | .processDrainFailed =>
      if state.processDrain == .pending || state.processDrain == .active then
        some { recordReasons state [.processFailed] with
          active := 0, processDrain := .failed }
      else none
  | .processDrainSucceeded =>
      if state.processDrain == .pending || state.processDrain == .active then
        some { state with active := 0, processDrain := .succeeded }
      else none
  | .outputDrained =>
      if state.outputDrain == .pending || state.outputDrain == .active then
        some { state with outputDrain := .succeeded }
      else none
  | .outputDrainFailed =>
      if state.outputDrain == .pending || state.outputDrain == .active then
        some { state with outputDrain := .failed }
      else none
  | .monitorJoined =>
      if state.monitorJoin == .pending || state.monitorJoin == .active then
        some { state with monitorJoin := .succeeded }
      else none
  | .monitorJoinFailed =>
      if state.monitorJoin == .pending || state.monitorJoin == .active then
        some { state with monitorJoin := .failed }
      else none
  | .reportSucceeded =>
      if state.report == .pending || state.report == .active then
        some { state with report := .succeeded }
      else none
  | .reportFailed =>
      if state.report == .pending || state.report == .active then
        some { state with report := .failed }
      else none
  | .requestCleanup root =>
      if !state.ownedRoots.contains root || state.cleanupRequested.contains root ||
          state.active != 0 || !allSafetySettled state then none
      else if state.deliveryRoots.contains root &&
          !(state.report == .succeeded || state.report == .failed) then none
      else some { state with cleanupRequested := state.cleanupRequested ++ [root] }
  | .cleanupSucceeded root =>
      if !state.cleanupRequested.contains root || anyOutcomeRecorded state root ||
          !allSafetySucceeded state then none
      else some { state with cleanupClean := state.cleanupClean ++ [root] }
  | .cleanupFailed root =>
      if !state.cleanupRequested.contains root || anyOutcomeRecorded state root ||
          !allSafetySucceeded state then none
      else some { state with cleanupFailed := state.cleanupFailed ++ [root] }
  | .cleanupDeferred root =>
      if !state.cleanupRequested.contains root || anyOutcomeRecorded state root then none
      else some { state with cleanupDeferred := state.cleanupDeferred ++ [root] }
  | .cleanupRetained root =>
      if !state.cleanupRequested.contains root || anyOutcomeRecorded state root then none
      else some { state with cleanupRetained := state.cleanupRetained ++ [root] }
  | .finish =>
      if state.active != 0 || !allSafetySettled state ||
          state.cleanupDeferred != [] || state.report != .succeeded then none
      else if !(state.ownedRoots.all fun root =>
          state.cleanupClean.contains root || state.cleanupFailed.contains root ||
            state.cleanupRetained.contains root) then none
      else if !(state.deliveryRoots.all fun root => state.cleanupClean.contains root) then none
      else some { state with finished := true }

def step (state : State) (event : Event) : Option State :=
  (transition state event).bind (acceptStep state)

def run : State → List Event → Option State
  | state, [] => some state
  | state, event :: rest => (step state event).bind fun next => run next rest

structure Execution where
  state : State
  accepted : Bool
  rejectedAt : Option Nat
  deriving BEq, DecidableEq, Repr

def execute (start : State) (events : List Event) : Execution :=
  let rec loop (state : State) (remaining : List Event) (index : Nat) : Execution :=
    match remaining with
    | [] => { state, accepted := true, rejectedAt := none }
    | event :: rest =>
        match step state event with
        | none => { state, accepted := false, rejectedAt := some index }
        | some next => loop next rest (index + 1)
  if Invariant start then loop start events 0
  else { state := start, accepted := false, rejectedAt := some 0 }

def renameRoot (root : RootId) : RootId :=
  if root == executionRoot then deliveryRoot else executionRoot

def renameRoots (roots : List RootId) : List RootId := roots.map renameRoot

def renameState (state : State) : State :=
  { state with
    ownedRoots := renameRoots state.ownedRoots
    deliveryRoots := renameRoots state.deliveryRoots
    cleanupRequested := renameRoots state.cleanupRequested
    cleanupClean := renameRoots state.cleanupClean
    cleanupFailed := renameRoots state.cleanupFailed
    cleanupDeferred := renameRoots state.cleanupDeferred
    cleanupRetained := renameRoots state.cleanupRetained }

def renameEvent : Event → Event
  | .requestCleanup root => .requestCleanup (renameRoot root)
  | .cleanupSucceeded root => .cleanupSucceeded (renameRoot root)
  | .cleanupFailed root => .cleanupFailed (renameRoot root)
  | .cleanupDeferred root => .cleanupDeferred (renameRoot root)
  | .cleanupRetained root => .cleanupRetained (renameRoot root)
  | event => event

def samePayloadClass (left right : Event) : Bool :=
  match left, right with
  | .observe owned maxOwned free minFree,
      .observe owned' maxOwned' free' minFree' =>
      (owned ≥ maxOwned) == (owned' ≥ maxOwned') &&
        (free ≤ minFree) == (free' ≤ minFree')
  | _, _ => false

end HoiminOracle.DiskGuard
