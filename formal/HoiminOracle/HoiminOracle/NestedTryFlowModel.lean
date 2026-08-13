import HoiminOracle.BindingFlowModel

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

def Exits.categoryStates (exits : Exits) : ExitCategory → List Env
  | .fallthrough => exits.fallthrough.toList
  | .break => exits.breaks
  | .continue => exits.continues
  | .terminate => exits.terminates

def mapExits (transfer : Env → Env) (exits : Exits) : Exits where
  fallthrough := exits.fallthrough.map transfer
  breaks := exits.breaks.map transfer
  continues := exits.continues.map transfer
  terminates := exits.terminates.map transfer

def cleanupName (name : Name) (environment : Env) : Env :=
  environment.set name .absent

def cleanupExits (name : Name) (exits : Exits) : Exits :=
  mapExits (cleanupName name) exits

def composeTry
    (body handler : Exits)
    (handlerTarget : Option Name)
    (orelse finalizer : Env → Exits) : Exits :=
  let normal := body.andThen orelse
  let cleanedHandler := match handlerTarget with
    | none => handler
    | some name => cleanupExits name handler
  routeFinally (normal.merge cleanedHandler) finalizer

def allReachableStates (exits : Exits) : List Env :=
  exits.states

end HoiminOracle.NestedTryFlow
