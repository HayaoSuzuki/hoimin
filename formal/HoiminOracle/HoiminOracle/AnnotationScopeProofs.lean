import HoiminOracle.AnnotationScopeModel

namespace HoiminOracle.AnnotationScope

open BindingFlow

theorem comprehension_preserves_outer (outer : Env) (target : Name) :
    (observeComprehension outer target).after = outer := by
  rfl

theorem comprehension_target_is_shadowed (outer : Env) (target : Name) :
    (observeComprehension outer target).inside = .shadowed := by
  rfl

theorem global_write_preserves_nearest_function
    (state : DirectedState)
    (name : Name)
    (fact : Fact) :
    (writeDirected .global name fact state).nearestFunctionEnv =
      state.nearestFunctionEnv := by
  rfl

theorem global_write_updates_module
    (state : DirectedState)
    (name : Name)
    (fact : Fact) :
    (writeDirected .global name fact state).moduleEnv.get name = fact := by
  cases name <;> rfl

theorem nonlocal_write_preserves_module
    (state : DirectedState)
    (name : Name)
    (fact : Fact) :
    (writeDirected .nonlocal name fact state).moduleEnv = state.moduleEnv := by
  rfl

theorem nonlocal_write_updates_nearest_function
    (state : DirectedState)
    (name : Name)
    (fact : Fact) :
    (writeDirected .nonlocal name fact state).nearestFunctionEnv.get name = fact := by
  cases name <;> rfl

theorem normal_write_updates_only_current
    (state : DirectedState)
    (name : Name)
    (fact : Fact) :
    (writeDirected .normal name fact state).currentEnv.get name = fact := by
  cases name <;> rfl

theorem global_reimport_restores_typing
    (state : DirectedState)
    (name : Name) :
    (writeDirected .global name (.known .typing)
      (writeDirected .global name .shadowed state)).moduleEnv.get name =
        .known .typing := by
  cases name <;> rfl

theorem nonlocal_reimport_restores_typing
    (state : DirectedState)
    (name : Name) :
    (writeDirected .nonlocal name (.known .typing)
      (writeDirected .nonlocal name .shadowed state)).nearestFunctionEnv.get name =
        .known .typing := by
  cases name <;> rfl

theorem comprehension_preserves_name
    (outer : Env)
    (target observed : Name) :
    (observeComprehension outer target).after.get observed = outer.get observed := by
  rfl

private def witnessEnv : Env where
  source := .known .builtin
  destination := .known .typing

private def witnessState : DirectedState where
  moduleEnv := witnessEnv
  nearestFunctionEnv := witnessEnv
  currentEnv := witnessEnv

private def brokenComprehensionLeak
    (outer : Env)
    (target : Name) : ComprehensionObservation :=
  { observeComprehension outer target with after := outer.set target .shadowed }

private def brokenFirstIterableBinding
    (outer : Env)
    (target : Name) : ComprehensionObservation :=
  { observeComprehension outer target with firstIterable := .shadowed }

private def brokenGlobalCurrent
    (name : Name)
    (fact : Fact)
    (state : DirectedState) : DirectedState :=
  writeDirected .normal name fact state

private def brokenNonlocalModule
    (name : Name)
    (fact : Fact)
    (state : DirectedState) : DirectedState :=
  writeDirected .global name fact state

example :
    brokenComprehensionLeak witnessEnv .source ≠
      observeComprehension witnessEnv .source := by
  decide

example :
    brokenFirstIterableBinding witnessEnv .source ≠
      observeComprehension witnessEnv .source := by
  decide

example :
    brokenGlobalCurrent .source .shadowed witnessState ≠
      writeDirected .global .source .shadowed witnessState := by
  decide

example :
    brokenNonlocalModule .source .shadowed witnessState ≠
      writeDirected .nonlocal .source .shadowed witnessState := by
  decide

example : witnessEnv ≠ witnessEnv.set .destination .shadowed := by
  decide

end HoiminOracle.AnnotationScope
