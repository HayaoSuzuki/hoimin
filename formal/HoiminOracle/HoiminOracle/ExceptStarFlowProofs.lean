import HoiminOracle.ExceptStarFlowModel
import HoiminOracle.BindingFlowProofs

namespace HoiminOracle.ExceptStarFlow

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

set_option maxHeartbeats 100000 in
theorem later_sibling_runs_after_raised_handler
    (environment : Env)
    (firstAction secondAction : EnvAction) :
    let initial : StarRoute :=
      { env := environment, activeRemainder := true }
    let first : StarHandler :=
      { split := { matched := true, remainder := true }
        action := firstAction
        raises := true }
    let second : StarHandler :=
      { split := { matched := true, remainder := false }
        action := secondAction }
    let result := routeStarHandlers initial [first, second]
    result.visits = 2 ∧ result.pendingRaised = true := by
  simp [routeStarHandlers, routeStarHandler]

set_option maxHeartbeats 100000 in
theorem handler_runs_at_most_once
    (route : StarRoute)
    (handler : StarHandler)
    (active : route.activeRemainder = true)
    (matched : handler.split.matched = true) :
    (routeStarHandler route handler).visits = route.visits + 1 := by
  simp [routeStarHandler, active, matched]

set_option maxHeartbeats 100000 in
theorem only_remainder_advances
    (route : StarRoute)
    (handler : StarHandler)
    (active : route.activeRemainder = true) :
    (routeStarHandler route handler).activeRemainder =
      handler.split.remainder := by
  simp [routeStarHandler, active]
  split <;> rfl

set_option maxHeartbeats 100000 in
theorem target_cleanup_reaches_fallthrough
    (route : StarRoute)
    (handler : StarHandler)
    (name : Name)
    (active : route.activeRemainder = true)
    (matched : handler.split.matched = true)
    (target : handler.target = some name)
    (falls : handler.raises = false)
    (handled : handler.split.remainder = false)
    (noPending : route.pendingRaised = false) :
    let result := routeStarHandler route handler
    result.env.get name = .absent ∧
      (finishStarRoute result).fallthrough = some result.env := by
  simp [routeStarHandler, finishStarRoute, active, matched, target, falls, handled,
    noPending, cleanupTarget, cleanupName, Exits.fallthroughOnly]
  cases name <;> simp [Env.get, Env.set]

set_option maxHeartbeats 100000 in
theorem target_cleanup_reaches_delayed_terminate
    (route : StarRoute)
    (handler : StarHandler)
    (name : Name)
    (active : route.activeRemainder = true)
    (matched : handler.split.matched = true)
    (target : handler.target = some name)
    (raises : handler.raises = true) :
    let result := routeStarHandler route handler
    result.env.get name = .absent ∧
      (finishStarRoute result).terminates = [result.env] := by
  simp [routeStarHandler, finishStarRoute, active, matched, target, raises,
    cleanupTarget, cleanupName, Exits.categoryOnly]
  cases name <;> simp [Env.get, Env.set]

set_option maxHeartbeats 100000 in
theorem final_remainder_terminates
    (route : StarRoute)
    (active : route.activeRemainder = true) :
    (finishStarRoute route).terminates = [route.env] := by
  simp [finishStarRoute, active, Exits.categoryOnly]

set_option maxHeartbeats 100000 in
theorem retained_fact_occurs_in_every_route
    (routes : List StarRoute)
    (joined : Env)
    (name : Name)
    (target : Target)
    (joinedRoutes : exactSummary routes = some joined)
    (retained : joined.get name = .known target) :
    ∀ environment ∈ finalStates routes,
      environment.get name = .known target := by
  exact meetAll_retained_on_every_path
    (finalStates routes) joined name target joinedRoutes retained

private theorem fact_action_subset_collapse
    (incoming : Fact)
    (first second : FactAction) :
    ((incoming.meet (first.apply incoming)).meet (second.apply incoming)).meet
        (second.apply (first.apply incoming)) =
      (incoming.meet (first.apply incoming)).meet (second.apply incoming) := by
  cases incoming with
  | absent =>
      cases first <;> cases second <;>
        simp [FactAction.apply, Fact.meet]
  | known target =>
      cases target <;> cases first <;> cases second <;>
        simp [FactAction.apply, Fact.meet]
  | shadowed =>
      cases first <;> cases second <;>
        simp [FactAction.apply, Fact.meet]
  | unknown =>
      cases first <;> cases second <;>
        simp [FactAction.apply, Fact.meet]

set_option maxHeartbeats 100000 in
theorem conservative_summary_matches_exact_subset_meet
    (incoming : Env)
    (first second : EnvAction) :
    exactSubsetSummary incoming first second =
      conservativeSummary incoming first second := by
  cases incoming with
  | mk source destination =>
      cases first with
      | mk firstSource firstDestination =>
          cases second with
          | mk secondSource secondDestination =>
              simp only [exactSubsetSummary, conservativeSummary, applyTwo,
                EnvAction.apply, Env.meet]
              rw [fact_action_subset_collapse source firstSource secondSource]
              rw [fact_action_subset_collapse
                destination firstDestination secondDestination]

end HoiminOracle.ExceptStarFlow
