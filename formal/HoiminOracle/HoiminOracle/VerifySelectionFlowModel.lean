import HoiminOracle.LineDiverseModel
import HoiminOracle.SamplingModel

namespace HoiminOracle.VerifySelectionFlow

open HoiminOracle.LineDiverse (Candidate Policy)

inductive RequestId
  | candidate (id : String)
  | malformed (text : String)
  deriving DecidableEq

inductive Request
  | explicit (ids : List RequestId)
  | top (policy : Policy) (offset count : Nat)
  | sample (count : Nat) (seed : UInt64)

structure Plan where
  ranked : List Candidate
  truncated : Bool
  budget : Nat

inductive Issue
  | malformed (text : String)
  | unknown (id : String)
  | offset (offset population : Nat)
  | explicitBudget (selected budget : Nat)
  | orderedBudget (selected budget : Nat)
  | emptyExplicit | emptyTop | emptySample | truncatedSample | sampleUnavailable
  deriving DecidableEq

inductive Mode | explicit | top (policy : Policy) | sample (population : Nat) (seed : UInt64)

structure Selection where
  mode : Mode
  requested : Nat
  ids : List String
  truncated : Bool

def requestText : RequestId → String
  | .candidate id => id
  | .malformed text => text

def firstMalformed (ids : List RequestId) : Option String :=
  ids.findSome? fun
    | .candidate _ => none
    | .malformed text => some text

def select (plan : Plan) : Request → Except Issue Selection
  | .explicit requested =>
    if let some text := firstMalformed requested then .error (.malformed text)
    else
      let ids := (requested.map requestText).eraseDups
      if ids.isEmpty then .error .emptyExplicit
      else if ids.length > plan.budget then .error (.explicitBudget ids.length plan.budget)
      else if let some id := (ids.mergeSort (· ≤ ·)).find? (fun id => !(plan.ranked.any (·.id == id))) then
        .error (.unknown id)
      else .ok ⟨.explicit, ids.length, ids, plan.truncated⟩
  | .top policy offset count =>
    let population := plan.ranked.length
    if offset > 0 && offset ≥ population then .error (.offset offset population)
    else
      let ids := LineDiverse.page (LineDiverse.ids (LineDiverse.order policy plan.ranked)) offset count
      if ids.isEmpty then .error .emptyTop
      else if ids.length > plan.budget then .error (.orderedBudget ids.length plan.budget)
      else .ok ⟨.top policy, count, ids, plan.truncated⟩
  | .sample count seed =>
    let population := plan.ranked.length
    let selected := min count population
    if plan.truncated then .error .truncatedSample
    else if population = 0 then .error .emptySample
    else if selected > plan.budget then .error (.orderedBudget selected plan.budget)
    else match Sampling.sample population count seed with
      | none => .error .sampleUnavailable
      | some indices =>
        let ids := indices.filterMap (fun index => plan.ranked[index]?.map (·.id))
        .ok ⟨.sample population seed, count, ids, false⟩

def orderOverride (selection : Selection) : Option (List String) :=
  match selection.mode with
  | .explicit => none
  | .top _ | .sample _ _ => some selection.ids

def discoveredProjection (selection : Selection) (discovered : List String) : List String :=
  discovered.filter selection.ids.contains

def preview (selection : Selection) (discovered : List String) : List String :=
  (orderOverride selection).getD (discoveredProjection selection discovered)

-- Streamed IDs are unique in this model. Ordered runs release their claims at
-- EOF after the discovery stream has accounted for every selected ID.
def dispatch (selection : Selection) (discovered : List String) : Option (List String) :=
  match orderOverride selection with
  | none => some (discoveredProjection selection discovered)
  | some ordered => if ordered.all discovered.contains then some ordered else none

theorem truncated_plan_refuses_sampling (plan : Plan) (count : Nat) (seed : UInt64)
    (truncated : plan.truncated = true) :
    select plan (.sample count seed) = .error .truncatedSample := by
  simp [select, truncated]

theorem malformed_id_is_rejected_before_budget (plan : Plan) (ids : List RequestId)
    (text : String) (first : firstMalformed ids = some text) :
    select plan (.explicit ids) = .error (.malformed text) := by
  simp [select, first]

theorem top_offset_preempts_page_selection (plan : Plan) (policy : Policy) (offset count : Nat)
    (positive : offset > 0) (outside : offset ≥ plan.ranked.length) :
    select plan (.top policy offset count) = .error (.offset offset plan.ranked.length) := by
  simp [select, positive, outside]

theorem explicit_preview_agrees_with_dispatch (selection : Selection)
    (previewDiscovery executionDiscovery : List String)
    (explicit : selection.mode = .explicit)
    (sameProjection : discoveredProjection selection previewDiscovery =
      discoveredProjection selection executionDiscovery) :
    dispatch selection executionDiscovery = some (preview selection previewDiscovery) := by
  simp [dispatch, preview, orderOverride, explicit, sameProjection]

theorem ordered_preview_agrees_with_dispatch (selection : Selection)
    (previewDiscovery executionDiscovery order : List String)
    (ordered : orderOverride selection = some order)
    (seen : order.all executionDiscovery.contains = true) :
    dispatch selection executionDiscovery = some (preview selection previewDiscovery) := by
  simp [dispatch, preview, ordered, seen]

theorem missing_candidate_blocks_ordered_dispatch (selection : Selection)
    (order discovered : List String) (ordered : orderOverride selection = some order)
    (missing : order.all discovered.contains = false) :
    dispatch selection discovered = none := by
  simp [dispatch, ordered, missing]

end HoiminOracle.VerifySelectionFlow
