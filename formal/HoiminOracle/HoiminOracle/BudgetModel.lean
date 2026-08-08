import Std

namespace HoiminOracle.BudgetAudit

inductive Kind
  | memory
  | copy
  | processes
  deriving Repr, DecidableEq

structure Limits where
  memory : Nat
  copy : Nat
  processes : Nat
  deriving Repr, DecidableEq

structure Entry where
  id : Nat
  kind : Kind
  amount : Nat
  deriving Repr, DecidableEq

structure State where
  limits : Limits
  active : List Entry
  released : List Nat
  nextId : Option Nat
  maxId : Nat
  deriving Repr, DecidableEq

inductive Event
  | reserve (kind : Kind) (amount : Nat)
  | release (ids : List Nat)
  deriving Repr, DecidableEq

inductive Rejection
  | limitReached
  | idsExhausted
  | duplicate
  | alreadyReleased
  | unknown
  deriving Repr, DecidableEq

structure Verdict where
  state : State
  rejection : Option Rejection
  allocated : Option Nat
  deriving Repr, DecidableEq

namespace State

def initial (limits : Limits) (maxId : Nat := 2) : State where
  limits := limits
  active := []
  released := []
  nextId := some 0
  maxId := maxId

end State

namespace Rejection

def code : Rejection → String
  | .limitReached => "budget.limit"
  | .idsExhausted => "budget.reservation_id.exhausted"
  | .duplicate => "budget.reservation.already_released"
  | .alreadyReleased => "budget.reservation.already_released"
  | .unknown => "budget.reservation.unknown"

end Rejection

namespace Verdict

def errorCode? (verdict : Verdict) : Option String :=
  verdict.rejection.map Rejection.code

end Verdict

def limit (limits : Limits) : Kind → Nat
  | .memory => limits.memory
  | .copy => limits.copy
  | .processes => limits.processes

def totalEntries : List Entry → Kind → Nat
  | [], _ => 0
  | entry :: rest, kind =>
      (if entry.kind == kind then entry.amount else 0) + totalEntries rest kind

def total (state : State) (kind : Kind) : Nat :=
  totalEntries state.active kind

def available (state : State) (kind : Kind) : Nat :=
  (limit state.limits kind).sub (total state kind)

def activeIds (state : State) : List Nat :=
  state.active.map Entry.id

def knownIds (state : State) : List Nat :=
  activeIds state ++ state.released

def hasDuplicates (ids : List Nat) : Bool :=
  !decide ids.Nodup

def disjointIds (left right : List Nat) : Bool :=
  left.all fun id => !(id ∈ right)

def frontierValid (state : State) : Bool :=
  match state.nextId with
  | some next =>
      next ≤ state.maxId && (knownIds state).all fun id => id < next
  | none => (knownIds state).all fun id => id ≤ state.maxId

def safe (state : State) : Bool :=
  total state .memory ≤ limit state.limits .memory &&
    total state .copy ≤ limit state.limits .copy &&
    total state .processes ≤ limit state.limits .processes &&
    !hasDuplicates (activeIds state) &&
    !hasDuplicates state.released &&
    disjointIds (activeIds state) state.released &&
    frontierValid state

def Invariant (state : State) : Prop :=
  safe state = true

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  rejection := some reason
  allocated := none

def validateRelease (state : State) (ids : List Nat) : Option Rejection :=
  if hasDuplicates ids then
    some .duplicate
  else if ids.any fun id => id ∈ state.released then
    some .alreadyReleased
  else if ids.any fun id => !(id ∈ activeIds state) then
    some .unknown
  else
    none

def reserve (state : State) (kind : Kind) (amount : Nat) : Verdict :=
  if amount > available state kind then
    reject state .limitReached
  else
    match state.nextId with
    | none => reject state .idsExhausted
    | some id =>
        if id > state.maxId then
          reject state .idsExhausted
        else
          let nextId := if id = state.maxId then none else some (id + 1)
          { state := {
              state with
                active := { id, kind, amount } :: state.active
                nextId := nextId
            }
            rejection := none
            allocated := some id }

def release (state : State) (ids : List Nat) : Verdict :=
  match validateRelease state ids with
  | some reason => reject state reason
  | none =>
      { state := {
          state with
            active := state.active.filter fun entry => !(entry.id ∈ ids)
            released := ids ++ state.released
        }
        rejection := none
        allocated := none }

def step (state : State) : Event → Verdict
  | .reserve kind amount => reserve state kind amount
  | .release ids => release state ids

def run : State → List Event → State
  | state, [] => state
  | state, event :: rest => run (step state event).state rest

def semanticValues : List Nat := [0, 1, 2]

def reserveEvents : List Event :=
  [.memory, .copy, .processes].flatMap fun kind =>
    semanticValues.map fun amount => .reserve kind amount

def releaseEvents : List Event := [
  .release [],
  .release [0],
  .release [1],
  .release [2],
  .release [0, 1],
  .release [1, 0],
  .release [0, 0],
  .release [0, 2]
]

def eventAlphabet : List Event :=
  reserveEvents ++ releaseEvents

def auditLimits : List Limits :=
  semanticValues.flatMap fun memory =>
    semanticValues.flatMap fun copy =>
      semanticValues.map fun processes => { memory, copy, processes }

def auditInitials : List State :=
  auditLimits.map fun limits => State.initial limits

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq

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
  let initial := auditInitials.map fun state => { trace := [], state := state }
  explorationLayersWith step depth initial initial

def reachableUpTo (depth : Nat) : List Reachable :=
  (explorationLayers depth).flatten

def firstCounterexample?
    (next : State → Event → Verdict)
    (initial : State)
    (depth : Nat) : Option (List Event) :=
  let start : Reachable := { trace := [], state := initial }
  let layers := explorationLayersWith next depth [start] [start]
  (layers.flatten.find? fun item => !(safe item.state)).map Reachable.trace

def auditDepth : Nat := 6

def alphabetSize : Nat := eventAlphabet.length

def reachableStateCount : Nat :=
  (reachableUpTo auditDepth).length

def checkedTransitionCount : Nat :=
  ((explorationLayers auditDepth).take auditDepth).foldl
    (fun count layer => count + layer.length * alphabetSize) 0

def boundedAuditPasses : Bool :=
  (reachableUpTo auditDepth).all fun item => safe item.state

def partialReleaseStep (state : State) (ids : List Nat) : Verdict :=
  ids.foldl (fun verdict id =>
    if verdict.rejection.isSome then
      verdict
    else if id ∈ activeIds verdict.state then
      { state := {
          verdict.state with
            active := verdict.state.active.filter fun entry => entry.id != id
            released := id :: verdict.state.released
        }
        rejection := none
        allocated := none }
    else
      reject verdict.state .unknown) {
        state := state
        rejection := none
        allocated := none
      }

def brokenPartialRelease (state : State) : Event → Verdict
  | .release ids => partialReleaseStep state ids
  | event => step state event

def brokenReuseReleasedId (state : State) : Event → Verdict
  | .reserve kind amount =>
      match state.nextId, state.released with
      | none, id :: _ =>
          { state := {
              state with
                active := { id, kind, amount } :: state.active
            }
            rejection := none
            allocated := some id }
      | _, _ => step state (.reserve kind amount)
  | event => step state event

def brokenCrossKindLimit (state : State) : Event → Verdict
  | .reserve kind amount =>
      if amount > state.limits.memory + state.limits.copy + state.limits.processes -
          state.active.foldl (fun current entry => current + entry.amount) 0 then
        reject state .limitReached
      else
        match state.nextId with
        | none => reject state .idsExhausted
        | some id =>
            let nextId := if id = state.maxId then none else some (id + 1)
            { state := {
                state with
                  active := { id, kind, amount } :: state.active
                  nextId := nextId
              }
              rejection := none
              allocated := some id }
  | event => step state event

def partialReleaseWitness : List Event := [
  .reserve .copy 1,
  .reserve .copy 1,
  .release [0, 2]
]

def reuseWitness : List Event := [
  .reserve .copy 1,
  .release [0],
  .reserve .copy 1
]

def crossKindWitness : List Event := [
  .reserve .memory 1
]

def partialReleaseWitnessDetected : Bool :=
  let initial := State.initial { memory := 2, copy := 2, processes := 2 }
  decide (run initial partialReleaseWitness ≠
    runWith brokenPartialRelease initial partialReleaseWitness)
where
  runWith (next : State → Event → Verdict) : State → List Event → State
    | state, [] => state
    | state, event :: rest => runWith next (next state event).state rest

def reuseWitnessDetected : Bool :=
  let initial := State.initial { memory := 1, copy := 1, processes := 1 } 0
  decide (run initial reuseWitness ≠
    runWith brokenReuseReleasedId initial reuseWitness)
where
  runWith (next : State → Event → Verdict) : State → List Event → State
    | state, [] => state
    | state, event :: rest => runWith next (next state event).state rest

def crossKindWitnessDetected : Bool :=
  let initial := State.initial { memory := 0, copy := 2, processes := 0 }
  decide (run initial crossKindWitness ≠
    runWith brokenCrossKindLimit initial crossKindWitness)
where
  runWith (next : State → Event → Verdict) : State → List Event → State
    | state, [] => state
    | state, event :: rest => runWith next (next state event).state rest

def brokenWitnessesDetected : Bool :=
  partialReleaseWitnessDetected && reuseWitnessDetected &&
    crossKindWitnessDetected

example : brokenWitnessesDetected = true := by decide

example :
    firstCounterexample? step
      (State.initial { memory := 2, copy := 2, processes := 2 })
      auditDepth = none := by native_decide

end HoiminOracle.BudgetAudit
