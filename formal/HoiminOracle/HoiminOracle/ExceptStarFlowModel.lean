import HoiminOracle.NestedTryFlowModel

namespace HoiminOracle.ExceptStarFlow

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

inductive FactAction
  | keep
  | invalidate
  | restoreTyping
  deriving Repr, DecidableEq, BEq

def FactAction.apply (action : FactAction) (fact : Fact) : Fact :=
  match action with
  | .keep => fact
  | .invalidate => .absent
  | .restoreTyping => .known .typing

structure EnvAction where
  source : FactAction := .keep
  destination : FactAction := .keep
  deriving Repr, DecidableEq, BEq

def EnvAction.apply (action : EnvAction) (environment : Env) : Env where
  source := action.source.apply environment.source
  destination := action.destination.apply environment.destination

structure Split where
  matched : Bool
  remainder : Bool
  deriving Repr, DecidableEq, BEq

structure StarHandler where
  split : Split
  action : EnvAction := {}
  raises : Bool := false
  target : Option Name := none
  deriving Repr, DecidableEq, BEq

structure StarRoute where
  env : Env
  activeRemainder : Bool
  pendingRaised : Bool := false
  visits : Nat := 0
  deriving Repr, DecidableEq, BEq

def cleanupTarget (target : Option Name) (environment : Env) : Env :=
  match target with
  | none => environment
  | some name => cleanupName name environment

def routeStarHandler (route : StarRoute) (handler : StarHandler) : StarRoute :=
  if !route.activeRemainder then route
  else if !handler.split.matched then
    { route with activeRemainder := handler.split.remainder }
  else
    { env := cleanupTarget handler.target (handler.action.apply route.env)
      activeRemainder := handler.split.remainder
      pendingRaised := route.pendingRaised || handler.raises
      visits := route.visits + 1 }

def routeStarHandlers (route : StarRoute) (handlers : List StarHandler) : StarRoute :=
  handlers.foldl routeStarHandler route

def finishStarRoute (route : StarRoute) : Exits :=
  if route.activeRemainder || route.pendingRaised then
    .categoryOnly .terminate route.env
  else
    .fallthroughOnly route.env

def finalStates (routes : List StarRoute) : List Env :=
  routes.flatMap fun route => (finishStarRoute route).states

def exactSummary (routes : List StarRoute) : Option Env :=
  meetAll? (finalStates routes)

def applyTwo
    (incoming : Env)
    (first second : EnvAction) : Env :=
  second.apply (first.apply incoming)

def conservativeSummary
    (incoming : Env)
    (first second : EnvAction) : Env :=
  (incoming.meet (first.apply incoming)).meet (second.apply incoming)

def exactSubsetSummary
    (incoming : Env)
    (first second : EnvAction) : Env :=
  (conservativeSummary incoming first second).meet
    (applyTwo incoming first second)

end HoiminOracle.ExceptStarFlow
