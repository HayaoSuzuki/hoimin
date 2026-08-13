import HoiminOracle.NestedTryFlowModel
import HoiminOracle.BindingFlowProofs

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

set_option maxHeartbeats 100000 in
theorem sequential_abrupt_excludes_next
    (first : Exits)
    (next : Env → Exits)
    (unreachable : first.fallthrough = none) :
    first.andThen next = first.withoutFallthrough := by
  cases first
  simp_all [Exits.andThen, Exits.withoutFallthrough]

private theorem empty_merge (exits : Exits) :
    Exits.empty.merge exits = exits := by
  cases exits
  simp [Exits.empty, Exits.merge, meetOption]

private theorem merge_empty (exits : Exits) :
    exits.merge Exits.empty = exits := by
  cases exits with
  | mk fallthrough breaks continues terminates =>
      cases fallthrough <;>
        simp [Exits.empty, Exits.merge, meetOption]

set_option maxHeartbeats 100000 in
theorem singleton_finally_routes_exactly
    (category : ExitCategory)
    (environment : Env)
    (finalizer : Env → Exits) :
    routeFinally (Exits.categoryOnly category environment) finalizer =
      routeCategory category (finalizer environment) := by
  cases category <;>
    simp [routeFinally, routeMany,
      Exits.categoryOnly, empty_merge, merge_empty]

set_option maxHeartbeats 100000 in
theorem falling_finally_preserves_category
    (category : ExitCategory)
    (environment after : Env)
    (finalizer : Env → Exits)
    (falls : finalizer environment = Exits.fallthroughOnly after) :
    routeFinally (Exits.categoryOnly category environment) finalizer =
      Exits.categoryOnly category after := by
  rw [singleton_finally_routes_exactly, falls]
  exact HoiminOracle.BindingFlow.fallthrough_finally_preserves_category category after

set_option maxHeartbeats 100000 in
theorem abrupt_finally_replaces_category
    (incoming outgoing : ExitCategory)
    (environment after : Env)
    (finalizer : Env → Exits)
    (abrupt : finalizer environment = Exits.categoryOnly outgoing after)
    (notFalls : outgoing ≠ .fallthrough) :
    routeFinally (Exits.categoryOnly incoming environment) finalizer =
      Exits.categoryOnly outgoing after := by
  rw [singleton_finally_routes_exactly, abrupt]
  cases outgoing <;>
    simp_all [routeCategory, Exits.categoryOnly]

private theorem cleanupName_idempotent (name : Name) (environment : Env) :
    cleanupName name (cleanupName name environment) =
      cleanupName name environment := by
  cases name <;> cases environment <;> rfl

set_option maxHeartbeats 100000 in
theorem cleanup_exits_idempotent (name : Name) (exits : Exits) :
    cleanupExits name (cleanupExits name exits) = cleanupExits name exits := by
  cases exits
  simp [cleanupExits, mapExits, Function.comp_def, cleanupName_idempotent]

set_option maxHeartbeats 100000 in
theorem reachable_meet_retains_only_common_knowledge
    (exits : Exits)
    (joined : Env)
    (name : Name)
    (target : Target)
    (joinedPaths : meetAll? (allReachableStates exits) = some joined)
    (retained : joined.get name = .known target) :
    ∀ environment ∈ allReachableStates exits,
      environment.get name = .known target := by
  exact HoiminOracle.BindingFlow.meetAll_retained_on_every_path
    (allReachableStates exits) joined name target joinedPaths retained

end HoiminOracle.NestedTryFlow
