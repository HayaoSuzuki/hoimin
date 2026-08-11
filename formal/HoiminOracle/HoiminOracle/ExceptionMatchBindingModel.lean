import HoiminOracle.BindingFlowModel

namespace HoiminOracle.ExceptionMatchBinding

open BindingFlow

def deleteName (environment : Env) (name : Name) : Env :=
  environment.set name .shadowed

def bindTarget (environment : Env) (name : Name) : Env :=
  environment.set name .shadowed

def mapExitEnvs (transform : Env → Env) (exits : Exits) : Exits where
  fallthrough := exits.fallthrough.map transform
  breaks := exits.breaks.map transform
  continues := exits.continues.map transform
  terminates := exits.terminates.map transform

def cleanupHandlerExits (name : Name) (body : Exits) : Exits :=
  mapExitEnvs (fun environment => deleteName environment name) body

structure HandlerObservation where
  typeEntry : Env
  bodyEntry : Env
  exits : Exits
  deriving Repr, DecidableEq, BEq

def observeHandler (incoming : Env) (target : Name)
    (body : Env → Exits) : HandlerObservation where
  typeEntry := incoming
  bodyEntry := bindTarget incoming target
  exits := cleanupHandlerExits target (body (bindTarget incoming target))

structure PatternResult where
  matched : Env
  failed : Option Env
  deriving Repr, DecidableEq, BEq

structure CaseStep where
  body : Option Env
  nextCase : Option Env
  deriving Repr, DecidableEq, BEq

def advanceCase (pattern : PatternResult) (guardPassed : Option Bool)
    (guardEnvironment : Env := pattern.matched) : CaseStep :=
  match guardPassed with
  | none => { body := some pattern.matched, nextCase := pattern.failed }
  | some true => { body := some guardEnvironment, nextCase := pattern.failed }
  | some false =>
      { body := none
        nextCase := meetOption pattern.failed (some guardEnvironment) }

def finishMatch (unmatched : Option Env)
    (completed : List Env) : Option Env :=
  meetAll? (completed ++ unmatched.toList)

end HoiminOracle.ExceptionMatchBinding
