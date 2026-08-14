import HoiminOracle.NestedTryFlowModel

namespace HoiminOracle.MultipleHandlerJoin

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

structure HandlerStep where
  selected : Option Exits
  remainder : Option Env
  target : Option Name := none
  deriving Repr, DecidableEq, BEq

structure HandlerRoute where
  exits : Exits := .empty
  remainder : Option Env := none
  deriving Repr, DecidableEq, BEq

def cleanSelected (step : HandlerStep) : Exits :=
  match step.selected, step.target with
  | none, _ => .empty
  | some exits, none => exits
  | some exits, some name => cleanupExits name exits

def routeHandler (current : HandlerRoute) (step : HandlerStep) : HandlerRoute :=
  match current.remainder with
  | none => current
  | some _ =>
      { exits := current.exits.merge (cleanSelected step)
        remainder := step.remainder }

def routeHandlers (incoming : Option Env) (steps : List HandlerStep) : HandlerRoute :=
  steps.foldl routeHandler { remainder := incoming }

def finishHandlers (route : HandlerRoute) : Exits :=
  match route.remainder with
  | none => route.exits
  | some environment =>
      route.exits.merge (.categoryOnly .terminate environment)

end HoiminOracle.MultipleHandlerJoin
