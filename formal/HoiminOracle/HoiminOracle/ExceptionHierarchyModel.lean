import Std

namespace HoiminOracle.ExceptionHierarchy

structure ClassId where
  owner : Nat
  member : Nat
  deriving DecidableEq, BEq, Repr
structure ClassInfo where
  parent : ClassId
  transparent : Bool := true
  deriving Repr
abbrev Graph := ClassId → Option ClassInfo

def exceptionId : ClassId := ⟨0, 0⟩
def valueErrorId : ClassId := ⟨0, 1⟩
def seed (id : ClassId) : Bool := id == exceptionId || id == valueErrorId

-- Fuel counts user-defined classes; the terminal builtin consumes no step.
def ancestry : Nat → Graph → ClassId → Bool
  | 0, _, id => seed id
  | fuel + 1, graph, id =>
      if seed id then true
      else match graph id with
        | none => false
        | some info => ancestry fuel graph info.parent

def constructible : Nat → Graph → ClassId → Bool
  | 0, _, id => id == exceptionId
  | fuel + 1, graph, id =>
      if id == exceptionId then true
      else match graph id with
        | none => false
        | some info => info.transparent && constructible fuel graph info.parent

inductive ReachesException (graph : Graph) : ClassId → Prop
  | root {id} : seed id = true → ReachesException graph id
  | child {id info} : graph id = some info → ReachesException graph info.parent →
      ReachesException graph id

def related (graph : Graph) (source destination : ClassId) : Bool :=
  source != destination && match graph source, graph destination with
    | some a, some b => a.parent == destination || b.parent == source ||
        (a.parent == b.parent && !seed a.parent)
    | _, _ => false

def visible (loaded used : Nat) : Bool := decide (loaded ≤ used)

def eligible (graph : Graph) (source destination : ClassId)
    (loaded used : Nat) (raised trustedOrigin : Bool) : Bool :=
  ancestry 256 graph source && (ancestry 256 graph destination &&
    (related graph source destination && (visible loaded used && (trustedOrigin &&
      (!raised || (constructible 256 graph source && constructible 256 graph destination))))))

-- Some none denotes an opaque local binding; none denotes no local binding.
def resolve (globals : Nat → Option ClassId) (locals : Nat → Option (Option ClassId))
    (key : Nat) : Option ClassId :=
  match locals key with
  | some binding => binding
  | none => globals key

def namedEligible (graph : Graph) (globals : Nat → Option ClassId)
    (locals : Nat → Option (Option ClassId)) (source destination loaded used : Nat)
    (raised trustedOrigin : Bool) : Bool :=
  match resolve globals locals source, resolve globals locals destination with
  | some a, some b => eligible graph a b loaded used raised trustedOrigin
  | _, _ => false

def reserve (limit count : Nat) : Option Nat :=
  if count < limit then some (count + 1) else none

def retain (limit count : Nat) : Nat := (reserve limit count).getD count

def fill (limit : Nat) : Nat → Nat → Nat
  | 0, count => count
  | n + 1, count => fill limit n (retain limit count)

inductive Input where
  | original | changed | missing
  deriving DecidableEq, BEq, Repr
inductive Event where
  | change | delete | restore | build
  deriving DecidableEq, BEq, Repr
inductive Load where
  | never | ok | error
  deriving DecidableEq, BEq, Repr
structure Cache where
  origin : Nat
  digest : Nat
  eligible : Bool
  deriving DecidableEq, BEq, Repr
structure State where
  current : Input := .original
  cache : Option Cache := none
  lastLoad : Load := .never
  deriving DecidableEq, BEq, Repr

def initial : State := {}

def step (initialRelated : Bool) (state : State) : Event → State
  | .change => { state with current := .changed }
  | .delete => { state with current := .missing }
  | .restore => { state with current := .original }
  | .build =>
      if state.cache.isSome then { state with lastLoad := .ok }
      else match state.current with
        | .changed => { state with lastLoad := .error }
        | .original => { state with cache := some ⟨0, 0, initialRelated⟩, lastLoad := .ok }
        | .missing => { state with cache := some ⟨0, 0, false⟩, lastLoad := .ok }

def run (initialRelated : Bool) : State → List Event → State
  | state, [] => state
  | state, event :: rest => run initialRelated (step initialRelated state event) rest

def candidate (state : State) : Bool := state.cache.any (·.eligible)
def fingerprintMatches (state : State) : Bool := state.current == .original

def CacheInvariant (initialRelated : Bool) (state : State) : Prop :=
  ∀ cache, state.cache = some cache →
    cache.origin = 0 ∧ cache.digest = 0 ∧ (cache.eligible = true → initialRelated = true)

end HoiminOracle.ExceptionHierarchy
