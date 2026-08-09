import Std

namespace HoiminOracle.WorkspaceAudit

inductive Worker
  | w0
  | w1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Generation
  | g0
  | g1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive TaskId
  | t0
  | t1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Operation
  | create
  | apply
  | reset
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive TaskPhase
  | prepared
  | completedActive
  | completedPending
  | completedAbsent
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Slot where
  worker : Worker
  generation : Generation
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Task where
  id : TaskId
  operation : Operation
  slot : Slot
  target : Generation
  epoch : Nat
  phase : TaskPhase
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure State where
  ready : Bool
  epoch : Nat
  active : List Slot
  pending : List Slot
  tasks : List Task
  released : Bool
  deriving Repr, DecidableEq, BEq

namespace State

def initial : State where
  ready := false
  epoch := 0
  active := []
  pending := []
  tasks := []
  released := false

end State

def taskSlot? (task : Task) : Option Slot :=
  match task.phase with
  | .prepared | .completedActive | .completedPending => some task.slot
  | .completedAbsent => none

def ownedSlots (state : State) : List Slot :=
  state.active ++ state.pending ++ state.tasks.filterMap taskSlot?

def safe (state : State) : Bool :=
  decide (state.active.map Slot.worker).Nodup &&
    decide (state.pending.map Slot.worker).Nodup &&
    decide (state.tasks.map Task.id).Nodup &&
    decide (ownedSlots state).Nodup &&
    decide ((ownedSlots state).map Slot.worker).Nodup &&
    (!state.released || ownedSlots state == [])

def Invariant (state : State) : Prop := safe state = true

inductive Execution
  | active
  | pending
  | absent
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Event
  | preflight
  | seed (worker : Worker) (generation : Generation)
  | prepare (task : TaskId) (operation : Operation) (worker : Worker)
      (target : Generation)
  | execute (task : TaskId) (result : Execution)
  | accept (task : TaskId)
  | cleanupSuccess
  | cleanupFailure
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Rejection
  | notReady
  | workerMissing
  | workerBusy
  | workerAlreadyExists
  | taskExists
  | taskMissing
  | invalidTask
  | staleEpoch
  | cleanupFailed
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

namespace Rejection

def code : Rejection → String
  | .notReady => "workspace.plan.missing"
  | .workerMissing => "workspace.worker.missing"
  | .workerBusy => "workspace.worker.busy"
  | .workerAlreadyExists => "workspace.worker.missing"
  | .taskExists => "workspace.task.duplicate"
  | .taskMissing => "workspace.task.missing"
  | .invalidTask => "workspace.task.invalid"
  | .staleEpoch => "workspace.task.stale"
  | .cleanupFailed => "workspace.cleanup.failed"

end Rejection

structure Verdict where
  state : State
  rejection : Option Rejection
  deriving Repr, DecidableEq, BEq

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  rejection := some reason

def acceptState (state : State) : Verdict where
  state := state
  rejection := none

def findSlot (slots : List Slot) (worker : Worker) : Option Slot :=
  slots.find? fun slot => slot.worker == worker

def eraseWorker (slots : List Slot) (worker : Worker) : List Slot :=
  slots.filter fun slot => slot.worker != worker

def findTask (tasks : List Task) (id : TaskId) : Option Task :=
  tasks.find? fun task => task.id == id

def eraseTask (tasks : List Task) (id : TaskId) : List Task :=
  tasks.filter fun task => task.id != id

def replaceTask (tasks : List Task) (replacement : Task) : List Task :=
  tasks.map fun task => if task.id == replacement.id then replacement else task

def workerTaskBusy (state : State) (worker : Worker) : Bool :=
  state.tasks.any fun task => task.slot.worker == worker

def workerLocated (state : State) (worker : Worker) : Bool :=
  (findSlot state.active worker).isSome ||
    (findSlot state.pending worker).isSome ||
    workerTaskBusy state worker

def validExecution (task : Task) (result : Execution) : Bool :=
  match task.operation, result with
  | .apply, .active => true
  | .reset, _ => true
  | .create, _ => true
  | _, _ => false

def prepare
    (state : State)
    (id : TaskId)
    (operation : Operation)
    (worker : Worker)
    (target : Generation) : Verdict :=
  if !state.ready then
    reject state .notReady
  else if (findTask state.tasks id).isSome then
    reject state .taskExists
  else if workerTaskBusy state worker then
    reject state .workerBusy
  else
    match operation with
    | .create =>
        if (findSlot state.active worker).isSome then
          reject state .workerAlreadyExists
        else
          let source := (findSlot state.pending worker).getD { worker, generation := target }
          acceptState {
            state with
              pending := eraseWorker state.pending worker
              tasks := state.tasks ++ [{
                id
                operation
                slot := source
                target
                epoch := state.epoch
                phase := .prepared
              }]
          }
    | .apply | .reset =>
        match findSlot state.active worker with
        | none => reject state .workerMissing
        | some slot =>
            acceptState {
              state with
                active := eraseWorker state.active worker
                tasks := state.tasks ++ [{
                  id
                  operation
                  slot
                  target := slot.generation
                  epoch := state.epoch
                  phase := .prepared
                }]
            }

def execute (state : State) (id : TaskId) (result : Execution) : Verdict :=
  match findTask state.tasks id with
  | none => reject state .taskMissing
  | some task =>
      if task.phase != .prepared || !validExecution task result then
        reject state .invalidTask
      else
        let phase := match result with
          | .active => TaskPhase.completedActive
          | .pending => TaskPhase.completedPending
          | .absent => TaskPhase.completedAbsent
        let slot := if task.operation == .create && result == .active then
          { task.slot with generation := task.target }
        else
          task.slot
        acceptState {
          state with
            tasks := replaceTask state.tasks { task with slot, phase }
        }

def acceptCompletion (state : State) (id : TaskId) : Verdict :=
  match findTask state.tasks id with
  | none => reject state .taskMissing
  | some task =>
      if task.phase == .prepared then
        reject state .invalidTask
      else
        let without := { state with tasks := eraseTask state.tasks id }
        if task.epoch != state.epoch then
          reject without .staleEpoch
        else
          match task.phase with
          | .completedActive =>
              if (findSlot state.active task.slot.worker).isSome ||
                  (findSlot state.pending task.slot.worker).isSome then
                reject without .workerAlreadyExists
              else
                acceptState { without with active := without.active ++ [task.slot] }
          | .completedPending =>
              if (findSlot state.active task.slot.worker).isSome ||
                  (findSlot state.pending task.slot.worker).isSome then
                reject without .workerAlreadyExists
              else
                acceptState { without with pending := without.pending ++ [task.slot] }
          | .completedAbsent => acceptState without
          | .prepared => reject state .invalidTask

def cleanup (state : State) (succeeds : Bool) : Verdict :=
  if !succeeds then
    reject state .cleanupFailed
  else if !state.tasks.isEmpty then
    reject state .workerBusy
  else
    acceptState {
      state with
        ready := false
        epoch := state.epoch + 1
        active := []
        pending := []
        released := true
    }

def step (state : State) : Event → Verdict
  | .preflight =>
      if state.tasks.isEmpty then
        acceptState { state with ready := true, released := false }
      else
        reject state .workerBusy
  | .seed worker generation =>
      if !state.ready then
        reject state .notReady
      else if workerLocated state worker then
        reject state .workerAlreadyExists
      else
        acceptState {
          state with active := state.active ++ [{ worker, generation }]
        }
  | .prepare id operation worker target => prepare state id operation worker target
  | .execute id result => execute state id result
  | .accept id => acceptCompletion state id
  | .cleanupSuccess => cleanup state true
  | .cleanupFailure => cleanup state false

def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

def run : State → List Event → State := runWith step

def allWorkers : List Worker := [.w0, .w1]
def allGenerations : List Generation := [.g0, .g1]
def allTasks : List TaskId := [.t0, .t1]

def eventAlphabet : List Event := [
  .preflight,
  .seed .w0 .g0,
  .seed .w1 .g0,
  .prepare .t0 .apply .w0 .g0,
  .prepare .t0 .reset .w0 .g0,
  .prepare .t0 .create .w0 .g1,
  .execute .t0 .active,
  .execute .t0 .pending,
  .execute .t0 .absent,
  .accept .t0,
  .cleanupSuccess,
  .cleanupFailure
]

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq, BEq

def containsState (items : List Reachable) (state : State) : Bool :=
  items.any fun item => item.state == state

def successors (next : State → Event → Verdict) (item : Reachable) : List Reachable :=
  eventAlphabet.map fun event => {
    trace := item.trace ++ [event]
    state := (next item.state event).state
  }

def uniqueNewStates (seen candidates : List Reachable) : List Reachable :=
  candidates.foldl (fun retained candidate =>
    if containsState seen candidate.state || containsState retained candidate.state then
      retained
    else
      retained ++ [candidate]) []

def explorationLayersWith
    (next : State → Event → Verdict) :
    Nat → List Reachable → List Reachable → List (List Reachable)
  | 0, _, frontier => [frontier]
  | depth + 1, seen, frontier =>
      let following := uniqueNewStates seen (frontier.flatMap (successors next))
      frontier :: explorationLayersWith next depth (seen ++ following) following

def explorationLayers (depth : Nat) : List (List Reachable) :=
  let initial : Reachable := { trace := [], state := State.initial }
  explorationLayersWith step depth [initial] [initial]

def reachableUpTo (depth : Nat) : List Reachable :=
  (explorationLayers depth).flatten

def firstCounterexample?
    (next : State → Event → Verdict)
    (depth : Nat) : Option (List Event) :=
  let initial : Reachable := { trace := [], state := State.initial }
  let layers := explorationLayersWith next depth [initial] [initial]
  (layers.flatten.find? fun item => !(safe item.state)).map Reachable.trace

def auditDepth : Nat := 8
def alphabetSize : Nat := eventAlphabet.length
def reachableStateCount : Nat := (reachableUpTo auditDepth).length
def checkedTransitionCount : Nat :=
  ((explorationLayers auditDepth).take auditDepth).foldl
    (fun count layer => count + layer.length * alphabetSize) 0
def boundedAuditPasses : Bool :=
  (reachableUpTo auditDepth).all fun item => safe item.state

def brokenAtomicityStep (state : State) (event : Event) : Verdict :=
  let verdict := step state event
  match event with
  | .prepare _ _ worker _ =>
      if verdict.rejection.isSome && (findSlot state.active worker).isSome then
        { verdict with state := { verdict.state with active := eraseWorker state.active worker } }
      else
        verdict
  | _ => verdict

def brokenUniquenessStep (state : State) (event : Event) : Verdict :=
  match event with
  | .seed worker generation =>
      if state.ready then
        acceptState { state with active := state.active ++ [{ worker, generation }] }
      else
        step state event
  | _ => step state event

def brokenBoundaryStep (state : State) (event : Event) : Verdict :=
  match event with
  | .cleanupSuccess =>
      acceptState {
        state with
          ready := false
          epoch := state.epoch + 1
          active := []
          pending := []
          released := true
      }
  | .accept id =>
      match findTask state.tasks id with
      | some task =>
          if task.phase == .completedActive then
            acceptState {
              state with
                tasks := eraseTask state.tasks id
                active := eraseWorker state.active task.slot.worker ++ [task.slot]
            }
          else
            step state event
      | none => step state event
  | _ => step state event

def atomicityWitness : List Event := [
  .preflight,
  .seed .w0 .g0,
  .seed .w1 .g1,
  .prepare .t0 .apply .w1 .g1,
  .prepare .t0 .apply .w0 .g0
]

def uniquenessWitness : List Event := [
  .preflight,
  .seed .w0 .g0,
  .seed .w0 .g1
]

def boundaryWitness : List Event := [
  .preflight,
  .seed .w0 .g0,
  .prepare .t0 .apply .w0 .g0,
  .cleanupSuccess,
  .execute .t0 .active,
  .accept .t0
]

def rejectionPreservedWith
    (next : State → Event → Verdict)
    (trace : List Event)
    (event : Event) : Bool :=
  let before := runWith next State.initial trace
  let verdict := next before event
  !verdict.rejection.isSome || verdict.state == before

def atomicityWitnessDetected : Bool :=
  let beforeTrace := atomicityWitness.dropLast
  let event := atomicityWitness.getLast?.getD .preflight
  !rejectionPreservedWith brokenAtomicityStep beforeTrace event

def uniquenessWitnessDetected : Bool :=
  !(safe (runWith brokenUniquenessStep State.initial uniquenessWitness))

def boundaryWitnessDetected : Bool :=
  !(safe (runWith brokenBoundaryStep State.initial boundaryWitness))

def brokenWitnessesDetected : Bool :=
  atomicityWitnessDetected && uniquenessWitnessDetected && boundaryWitnessDetected

example : safe State.initial = true := by decide
example : brokenWitnessesDetected = true := by decide

end HoiminOracle.WorkspaceAudit
