import Std

namespace HoiminOracle.BindingFlow

inductive Name
  | source
  | destination
  deriving Repr, DecidableEq, BEq

inductive Target
  | builtin
  | typing
  deriving Repr, DecidableEq, BEq

inductive Fact
  | absent
  | known (target : Target)
  | shadowed
  | unknown
  deriving Repr, DecidableEq, BEq

def Fact.meet (left right : Fact) : Fact :=
  if left = right then left else .unknown

def Fact.rank : Fact → Nat
  | .unknown => 0
  | .absent | .known _ | .shadowed => 1

structure Env where
  source : Fact
  destination : Fact
  deriving Repr, DecidableEq, BEq

def emptyEnv : Env where
  source := .absent
  destination := .absent

def shadowedEnv : Env where
  source := .shadowed
  destination := .shadowed

def Env.get (environment : Env) : Name → Fact
  | .source => environment.source
  | .destination => environment.destination

def Env.set (environment : Env) (name : Name) (fact : Fact) : Env :=
  match name with
  | .source => { environment with source := fact }
  | .destination => { environment with destination := fact }

def Env.meet (left right : Env) : Env where
  source := left.source.meet right.source
  destination := left.destination.meet right.destination

def Env.rank (environment : Env) : Nat :=
  environment.source.rank + environment.destination.rank

def meetAll? : List Env → Option Env
  | [] => none
  | first :: rest => some (rest.foldl Env.meet first)

inductive ScopeKind
  | module
  | function
  | class
  | comprehension
  deriving Repr, DecidableEq, BEq

inductive Directive
  | normal
  | global
  | nonlocal
  deriving Repr, DecidableEq, BEq

structure Frame where
  id : Nat
  kind : ScopeKind
  directive : Directive := .normal
  before : Env := emptyEnv
  whole : Env := emptyEnv
  deriving Repr, DecidableEq, BEq

def factResolution (fact fallback : Fact) : Fact :=
  match fact with
  | .absent => fallback
  | other => other

mutual
  def resolveModuleWhole : Name → List Frame → Fact
    | _, [] => .known .builtin
    | name, frame :: rest =>
        if frame.kind == .module then
          factResolution (frame.whole.get name) (.known .builtin)
        else
          resolveModuleWhole name rest

  def resolveNonlocal : Name → List Frame → Fact
    | _, [] => .unknown
    | name, frame :: rest =>
        match frame.kind with
        | .function | .comprehension =>
            match frame.whole.get name with
            | .absent => resolveNonlocal name rest
            | other => other
        | .module | .class => resolveNonlocal name rest

  def resolveFrom : Name → List Frame → Bool → Fact
    | _, [], _ => .known .builtin
    | name, frame :: rest, direct =>
        match frame.kind with
        | .module =>
            factResolution
              ((if direct then frame.before else frame.whole).get name)
              (.known .builtin)
        | .function | .comprehension =>
            match frame.directive with
            | .global => resolveModuleWhole name rest
            | .nonlocal => resolveNonlocal name rest
            | .normal =>
                factResolution (frame.whole.get name) (resolveFrom name rest false)
        | .class =>
            if direct then
              match frame.directive with
              | .global => resolveModuleWhole name rest
              | .nonlocal => resolveNonlocal name rest
              | .normal =>
                  factResolution (frame.before.get name) (resolveFrom name rest false)
            else
              resolveFrom name rest false
end

structure Candidate where
  path : List Frame
  unrelatedSiblings : List Frame := []
  deriving Repr, DecidableEq, BEq

def resolve (name : Name) (candidate : Candidate) : Fact :=
  resolveFrom name candidate.path true

def resolveCandidate (candidate : Candidate) : Env where
  source := resolve .source candidate
  destination := resolve .destination candidate

def moduleFrame (before whole : Env) : Frame where
  id := 0
  kind := .module
  before
  whole

def functionFrame
    (id : Nat)
    (whole : Env)
    (directive : Directive := .normal) : Frame where
  id
  kind := .function
  directive
  whole

def classFrame
    (id : Nat)
    (before whole : Env)
    (directive : Directive := .normal) : Frame where
  id
  kind := .class
  directive
  before
  whole

def comprehensionFrame (id : Nat) (whole : Env) : Frame where
  id
  kind := .comprehension
  whole

def allowsCandidate
    (environment : Env)
    (sourceTarget destinationTarget : Target) : Bool :=
  decide (environment.source = .known sourceTarget) &&
    decide (environment.destination = .known destinationTarget)

inductive ExitCategory
  | fallthrough
  | break
  | continue
  | terminate
  deriving Repr, DecidableEq, BEq

structure Exits where
  fallthrough : Option Env := none
  breaks : List Env := []
  continues : List Env := []
  terminates : List Env := []
  deriving Repr, DecidableEq, BEq

def Exits.empty : Exits := {}

def Exits.fallthroughOnly (environment : Env) : Exits where
  fallthrough := some environment

def Exits.categoryOnly (category : ExitCategory) (environment : Env) : Exits :=
  match category with
  | .fallthrough => { fallthrough := some environment }
  | .break => { breaks := [environment] }
  | .continue => { continues := [environment] }
  | .terminate => { terminates := [environment] }

def meetOption (left right : Option Env) : Option Env :=
  match left, right with
  | none, other | other, none => other
  | some leftEnv, some rightEnv => some (leftEnv.meet rightEnv)

def Exits.merge (left right : Exits) : Exits where
  fallthrough := meetOption left.fallthrough right.fallthrough
  breaks := left.breaks ++ right.breaks
  continues := left.continues ++ right.continues
  terminates := left.terminates ++ right.terminates

def Exits.withoutFallthrough (exits : Exits) : Exits :=
  { exits with fallthrough := none }

def Exits.addCategory
    (exits : Exits)
    (category : ExitCategory)
    (environment : Env) : Exits :=
  exits.merge (categoryOnly category environment)

def Exits.states (exits : Exits) : List Env :=
  exits.fallthrough.toList ++ exits.breaks ++ exits.continues ++ exits.terminates

def routeCategory (category : ExitCategory) (finallyResult : Exits) : Exits :=
  match finallyResult.fallthrough with
  | none => finallyResult
  | some environment =>
      finallyResult.withoutFallthrough.addCategory category environment

def routeMany
    (category : ExitCategory)
    (states : List Env)
    (finalizer : Env → Exits) : Exits :=
  states.foldl
    (fun accumulated environment =>
      accumulated.merge (routeCategory category (finalizer environment)))
    .empty

def routeFinally (incoming : Exits) (finalizer : Env → Exits) : Exits :=
  let fallthrough := routeMany .fallthrough incoming.fallthrough.toList finalizer
  let breaks := routeMany .break incoming.breaks finalizer
  let continues := routeMany .continue incoming.continues finalizer
  let terminates := routeMany .terminate incoming.terminates finalizer
  ((fallthrough.merge breaks).merge continues).merge terminates

def Exits.andThen (first : Exits) (next : Env → Exits) : Exits :=
  let abrupt := { first with fallthrough := none }
  match first.fallthrough with
  | none => abrupt
  | some environment => abrupt.merge (next environment)

def loopIteration (head backEdge : Env) : Env :=
  head.meet backEdge

inductive Stmt
  | skip
  | bind (name : Name) (fact : Fact)
  | seq (first second : Stmt)
  | branch (left right : Stmt)
  | breakNow
  | continueNow
  | terminateNow
  | loop (body : Stmt)
  | tryFinally (body finalizer : Stmt)
  | tryFlow
      (body : Stmt)
      (handlerEntry : Env)
      (handler : Stmt)
      (orelse finalizer : Stmt)
  | matchFlow (matched failed : Stmt)
  | matchIrrefutable (body : Stmt)
  | scoped (candidate : Candidate) (body : Stmt)
  deriving Repr, DecidableEq, BEq

def meetStates (initial : Env) (states : List Env) : Env :=
  states.foldl Env.meet initial

def iterateToFixedPoint : Nat → (Env → Env) → Env → Option Env
  | 0, _, _ => none
  | fuel + 1, transfer, current =>
      let next := current.meet (transfer current)
      if next = current then some current
      else iterateToFixedPoint fuel transfer next

def eval : Nat → Stmt → Env → Exits
  | 0, _, _ => .empty
  | fuel + 1, statement, environment =>
      match statement with
      | .skip => .fallthroughOnly environment
      | .bind name fact => .fallthroughOnly (environment.set name fact)
      | .seq first second =>
          (eval fuel first environment).andThen (eval fuel second)
      | .branch left right =>
          (eval fuel left environment).merge (eval fuel right environment)
      | .breakNow => .categoryOnly .break environment
      | .continueNow => .categoryOnly .continue environment
      | .terminateNow => .categoryOnly .terminate environment
      | .loop body =>
          let transfer := fun head =>
            let bodyResult := eval fuel body head
            meetStates environment
              (bodyResult.fallthrough.toList ++ bodyResult.continues)
          match iterateToFixedPoint (environment.rank + 1) transfer environment with
          | none => .empty
          | some loopHead =>
              let bodyResult := eval fuel body loopHead
              {
                fallthrough := some (meetStates loopHead bodyResult.breaks)
                terminates := bodyResult.terminates
              }
      | .tryFinally body finalizer =>
          routeFinally (eval fuel body environment) (eval fuel finalizer)
      | .tryFlow body handlerEntry handler orelse finalizer =>
          let normal := (eval fuel body environment).andThen (eval fuel orelse)
          let handled := eval fuel handler handlerEntry
          routeFinally (normal.merge handled) (eval fuel finalizer)
      | .matchFlow matched failed =>
          (eval fuel matched environment).merge (eval fuel failed environment)
      | .matchIrrefutable body => eval fuel body environment
      | .scoped candidate body => eval fuel body (resolveCandidate candidate)

def Stmt.requiredFuel : Stmt → Nat
  | .skip | .bind _ _ | .breakNow | .continueNow | .terminateNow => 1
  | .seq first second | .branch first second |
      .tryFinally first second | .matchFlow first second =>
      Nat.max first.requiredFuel second.requiredFuel + 1
  | .loop body | .matchIrrefutable body | .scoped _ body =>
      body.requiredFuel + 1
  | .tryFlow body _ handler orelse finalizer =>
      Nat.max body.requiredFuel
        (Nat.max handler.requiredFuel
          (Nat.max orelse.requiredFuel finalizer.requiredFuel)) + 1

end HoiminOracle.BindingFlow
