import HoiminOracle.NestedMatchExitModel
import HoiminOracle.NestedTryFlowProofs

namespace HoiminOracle.NestedMatchExit

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

set_option maxHeartbeats 100000 in
theorem compose_match_preserves_breaks
    (branches : List Exits)
    (unmatched : Option Env)
    (branch : Exits)
    (member : branch ∈ branches)
    (environment : Env)
    (reachable : environment ∈ branch.breaks) :
    environment ∈ (composeMatch branches unmatched).breaks := by
  induction branches with
  | nil => simp at member
  | cons head tail inductionHypothesis =>
      simp only [List.mem_cons] at member
      simp only [composeMatch, Exits.merge]
      rcases member with rfl | member
      · exact List.mem_append_left _ reachable
      · exact List.mem_append_right _ (inductionHypothesis member)

set_option maxHeartbeats 100000 in
theorem compose_match_preserves_continues
    (branches : List Exits)
    (unmatched : Option Env)
    (branch : Exits)
    (member : branch ∈ branches)
    (environment : Env)
    (reachable : environment ∈ branch.continues) :
    environment ∈ (composeMatch branches unmatched).continues := by
  induction branches with
  | nil => simp at member
  | cons head tail inductionHypothesis =>
      simp only [List.mem_cons] at member
      simp only [composeMatch, Exits.merge]
      rcases member with rfl | member
      · exact List.mem_append_left _ reachable
      · exact List.mem_append_right _ (inductionHypothesis member)

set_option maxHeartbeats 100000 in
theorem compose_match_preserves_terminates
    (branches : List Exits)
    (unmatched : Option Env)
    (branch : Exits)
    (member : branch ∈ branches)
    (environment : Env)
    (reachable : environment ∈ branch.terminates) :
    environment ∈ (composeMatch branches unmatched).terminates := by
  induction branches with
  | nil => simp at member
  | cons head tail inductionHypothesis =>
      simp only [List.mem_cons] at member
      simp only [composeMatch, Exits.merge]
      rcases member with rfl | member
      · exact List.mem_append_left _ reachable
      · exact List.mem_append_right _ (inductionHypothesis member)

set_option maxHeartbeats 100000 in
theorem unreachable_unmatched_adds_no_exit :
    composeMatch [] none = Exits.empty := by
  rfl

private theorem empty_merge (exits : Exits) :
    Exits.empty.merge exits = exits := by
  cases exits
  simp [Exits.empty, Exits.merge, meetOption]

set_option maxHeartbeats 100000 in
theorem nested_handler_cleanup_precedes_finally
    (branches : List Exits)
    (unmatched : Option Env)
    (target : Name)
    (orelse finalizer : Env → Exits) :
    composeTry .empty (composeMatch branches unmatched) (some target)
        orelse finalizer =
      routeFinally (cleanupExits target (composeMatch branches unmatched))
        finalizer := by
  unfold composeTry
  simp only [Exits.andThen]
  change routeFinally
    (Exits.empty.merge (cleanupExits target (composeMatch branches unmatched)))
      finalizer = _
  rw [empty_merge]

set_option maxHeartbeats 100000 in
theorem nested_match_outgoing_is_reachable_meet
    (branches : List Exits)
    (unmatched : Option Env) :
    nestedMatchOutgoing branches unmatched =
      meetAll? (allReachableStates (composeMatch branches unmatched)) := by
  rfl

set_option maxHeartbeats 100000 in
theorem consume_loop_uses_continue_back_edges
    (zeroIteration : Env)
    (body : Exits) :
    loopNaturalEntry zeroIteration body =
      (meetAll?
        ([zeroIteration] ++ body.fallthrough.toList ++ body.continues)).getD
          zeroIteration := by
  rfl

set_option maxHeartbeats 100000 in
theorem consume_loop_propagates_only_terminates
    (zeroIteration : Env)
    (body : Exits)
    (orelse : Env → Exits) :
    let afterElse := orelse (loopNaturalEntry zeroIteration body)
    (consumeLoop zeroIteration body orelse).breaks = [] ∧
      (consumeLoop zeroIteration body orelse).continues = [] ∧
      (consumeLoop zeroIteration body orelse).terminates =
        body.terminates ++ afterElse.terminates := by
  simp [consumeLoop]

end HoiminOracle.NestedMatchExit
