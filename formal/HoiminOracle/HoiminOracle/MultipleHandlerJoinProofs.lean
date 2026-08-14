import HoiminOracle.MultipleHandlerJoinModel
import HoiminOracle.BindingFlowProofs

namespace HoiminOracle.MultipleHandlerJoin

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

private theorem empty_merge (exits : Exits) :
    Exits.empty.merge exits = exits := by
  cases exits
  simp [Exits.empty, Exits.merge, meetOption]

set_option maxHeartbeats 100000 in
theorem route_handlers_after_remainder_exhausted
    (exits : Exits) (suffix : List HandlerStep) :
    suffix.foldl routeHandler { exits, remainder := none } =
      { exits, remainder := none } := by
  induction suffix generalizing exits with
  | nil => rfl
  | cons step rest inductionHypothesis =>
      simp only [List.foldl_cons, routeHandler]
      exact inductionHypothesis exits

theorem reachable_handler_merges_cleaned_selected
    (exits : Exits) (incoming : Env) (step : HandlerStep) :
    routeHandler { exits, remainder := some incoming } step =
      { exits := exits.merge (cleanSelected step)
        remainder := step.remainder } := by
  rfl

set_option maxHeartbeats 100000 in
theorem selected_target_cleaned_in_every_category
    (target : Name) (category : ExitCategory) (environment : Env) :
    HoiminOracle.NestedTryFlow.Exits.categoryStates
      (cleanSelected
        { selected := some (.categoryOnly category environment)
          remainder := none
          target := some target }) category =
      [cleanupName target environment] := by
  cases category <;>
    simp [cleanSelected, cleanupExits, mapExits, Exits.categoryStates,
      Exits.categoryOnly]

set_option maxHeartbeats 100000 in
theorem selected_unrelated_fact_preserved
    (environment : Env) (target observed : Name)
    (different : target ≠ observed) :
    (cleanSelected
      { selected := some (.fallthroughOnly environment)
        remainder := none
        target := some target }).fallthrough.map (fun cleaned =>
          cleaned.get observed) = some (environment.get observed) := by
  cases target <;> cases observed <;>
    simp_all [cleanSelected, cleanupExits, mapExits, cleanupName,
      Exits.fallthroughOnly, Env.set, Env.get]

set_option maxHeartbeats 100000 in
theorem reachable_fallthrough_participates_in_meet
    (route : HandlerRoute) (joined : Env) (name : Name) (target : Target)
    (joinedPaths : outgoingEnv (finishHandlers route) = some joined)
    (retained : joined.get name = .known target) :
    ∀ environment ∈ (finishHandlers route).fallthrough.toList,
      environment.get name = .known target := by
  have allReachable := HoiminOracle.BindingFlow.meetAll_retained_on_every_path
    (allReachableStates (finishHandlers route)) joined name target
    joinedPaths retained
  intro environment membership
  apply allReachable environment
  simp only [allReachableStates, Exits.states, List.mem_append]
  exact Or.inl (Or.inl (Or.inl membership))

set_option maxHeartbeats 100000 in
theorem selected_abrupt_categories_preserved
    (category : ExitCategory) (environment : Env)
    (abrupt : category ≠ .fallthrough) :
    HoiminOracle.NestedTryFlow.Exits.categoryStates
      (cleanSelected
        { selected := some (.categoryOnly category environment)
          remainder := none }) category = [environment] := by
  cases category <;>
    simp_all [cleanSelected, Exits.categoryStates, Exits.categoryOnly]

theorem unhandled_remainder_terminates_once (environment : Env) :
    (finishHandlers
      { exits := .empty, remainder := some environment }).terminates =
      [environment] := by
  simp [finishHandlers, Exits.empty, Exits.merge, Exits.categoryOnly,
    meetOption]

theorem first_reachable_handler_is_its_cleaned_selection
    (incoming : Env) (step : HandlerStep) :
    (routeHandlers (some incoming) [step]).exits = cleanSelected step := by
  simp [routeHandlers, routeHandler, empty_merge]

end HoiminOracle.MultipleHandlerJoin
