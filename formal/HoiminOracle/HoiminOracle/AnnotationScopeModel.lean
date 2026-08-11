import HoiminOracle.BindingFlowModel

namespace HoiminOracle.AnnotationScope

open BindingFlow

inductive Resolution
  | definitelyBuiltin
  | shadowed
  | unknown
  deriving Repr, DecidableEq, BEq

structure DirectedState where
  moduleEnv : Env
  nearestFunctionEnv : Env
  currentEnv : Env
  deriving Repr, DecidableEq, BEq

def writeDirected
    (directive : Directive)
    (name : Name)
    (fact : Fact)
    (state : DirectedState) : DirectedState :=
  match directive with
  | .global => { state with moduleEnv := state.moduleEnv.set name fact }
  | .nonlocal =>
      { state with nearestFunctionEnv := state.nearestFunctionEnv.set name fact }
  | .normal => { state with currentEnv := state.currentEnv.set name fact }

def resolutionOfFact : Fact → Resolution
  | .known .builtin | .absent => .definitelyBuiltin
  | .shadowed | .known .typing => .shadowed
  | .unknown => .unknown

structure ComprehensionObservation where
  firstIterable : Resolution
  inside : Resolution
  after : Env
  deriving Repr, DecidableEq, BEq

def observeComprehension
    (outer : Env)
    (target : Name) : ComprehensionObservation where
  firstIterable := resolutionOfFact (outer.get target)
  inside := .shadowed
  after := outer

end HoiminOracle.AnnotationScope
