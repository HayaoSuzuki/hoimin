import HoiminOracle.ExceptionHierarchyModel

namespace HoiminOracle.ExceptionHierarchy

set_option maxHeartbeats 50000 in
theorem ancestry_reaches (fuel : Nat) (graph : Graph) (id : ClassId)
    (h : ancestry fuel graph id = true) : ReachesException graph id := by
  induction fuel generalizing id with
  | zero => exact .root h
  | succ fuel ih =>
      by_cases hs : seed id = true
      · exact .root hs
      · cases hi : graph id with
        | none => simp [ancestry, hs, hi] at h
        | some info =>
            exact .child hi (ih info.parent (by simpa [ancestry, hs, hi] using h))

set_option maxHeartbeats 50000 in
theorem emitted_endpoints_reach_exception (graph : Graph) (source destination : ClassId)
    (loaded used : Nat) (raised trusted : Bool)
    (h : eligible graph source destination loaded used raised trusted = true) :
    ReachesException graph source ∧ ReachesException graph destination := by
  simp only [eligible, Bool.and_eq_true] at h
  exact ⟨ancestry_reaches _ _ _ h.1, ancestry_reaches _ _ _ h.2.1⟩

set_option maxHeartbeats 50000 in
theorem emitted_only_after_load (graph : Graph) (source destination : ClassId)
    (loaded used : Nat) (raised trusted : Bool)
    (h : eligible graph source destination loaded used raised trusted = true) : loaded ≤ used := by
  simp only [eligible, Bool.and_eq_true, visible, decide_eq_true_eq] at h
  exact h.2.2.2.1

set_option maxHeartbeats 50000 in
theorem opaque_source_blocks (graph : Graph) (globals : Nat → Option ClassId)
    (locals : Nat → Option (Option ClassId)) (source destination loaded used : Nat)
    (raised trusted : Bool) (h : locals source = some none) :
    namedEligible graph globals locals source destination loaded used raised trusted = false := by
  simp [namedEligible, resolve, h]

set_option maxHeartbeats 50000 in
theorem module_identity_distinguishes (a b member : Nat) (h : a ≠ b) :
    (⟨a, member⟩ : ClassId) ≠ ⟨b, member⟩ := by
  intro same
  exact h (congrArg ClassId.owner same)

set_option maxHeartbeats 50000 in
theorem reserve_within_bound (limit count next : Nat) (h : reserve limit count = some next) :
    next ≤ limit := by
  unfold reserve at h
  split at h
  · simp only [Option.some.injEq] at h
    omega
  · contradiction

set_option maxHeartbeats 50000 in
theorem retain_within_bound (limit count : Nat) (h : count ≤ limit) :
    retain limit count ≤ limit := by
  unfold retain reserve
  split <;> simp_all <;> omega

set_option maxHeartbeats 50000 in
theorem fill_within_bound (limit n count : Nat) (h : count ≤ limit) :
    fill limit n count ≤ limit := by
  induction n generalizing count with
  | zero => exact h
  | succ n ih => exact ih _ (retain_within_bound limit count h)

set_option maxHeartbeats 50000 in
theorem step_preserves_cache (base : Bool) (state : State) (event : Event)
    (h : CacheInvariant base state) : CacheInvariant base (step base state event) := by
  cases event with
  | change => exact h
  | delete => exact h
  | restore => exact h
  | build =>
      cases hc : state.cache with
      | some cache => simpa [step, hc, CacheInvariant] using h
      | none =>
          cases hi : state.current <;> simp [step, hc, hi, CacheInvariant]

set_option maxHeartbeats 50000 in
theorem arbitrary_trace_preserves_cache (base : Bool) (events : List Event) :
    CacheInvariant base (run base initial events) := by
  have preserve : ∀ (state : State), CacheInvariant base state →
      CacheInvariant base (run base state events) := by
    induction events with
    | nil => intro state h; exact h
    | cons event rest ih =>
        intro state h
        exact ih (step base state event) (step_preserves_cache base state event h)
  exact preserve initial (by simp [CacheInvariant, initial])

set_option maxHeartbeats 50000 in
theorem unrelated_snapshot_never_invents_candidate (events : List Event) :
    candidate (run false initial events) = false := by
  have invariant := arbitrary_trace_preserves_cache false events
  cases hc : (run false initial events).cache with
  | none => simp [candidate, hc]
  | some cache =>
      have rejected := (invariant cache hc).2.2
      cases he : cache.eligible <;> simp_all [candidate]

set_option maxHeartbeats 50000 in
theorem step_keeps_successful_cache (base : Bool) (state : State) (cache : Cache)
    (event : Event) (h : state.cache = some cache) :
    (step base state event).cache = some cache := by
  cases event <;> simp [step, h]

set_option maxHeartbeats 50000 in
theorem arbitrary_trace_keeps_successful_cache (base : Bool) (events : List Event)
    (state : State) (cache : Cache) (h : state.cache = some cache) :
    (run base state events).cache = some cache := by
  induction events generalizing state with
  | nil => exact h
  | cons event rest ih => exact ih _ (step_keeps_successful_cache base state cache event h)

set_option maxHeartbeats 50000 in
theorem written_alias_invalidates_provider (aliases : Nat → Option Nat)
    (written : List Nat) (name owner : Nat) (hw : name ∈ written)
    (ho : aliases name = some owner) : providerTrusted aliases written owner = false := by
  have affected : written.any (fun key => aliases key == some owner) = true :=
    List.any_eq_true.mpr ⟨name, hw, by simp [ho]⟩
  simp [providerTrusted, affected]

set_option maxHeartbeats 50000 in
theorem closed_writes_cover_alias_paths (edges : AliasEdges) (written marked : List ScopedName)
    (complete : closureComplete edges written marked = true) (name : ScopedName)
    (reachable : WriteReach edges written name) : name ∈ marked := by
  simp only [closureComplete, Bool.and_eq_true] at complete
  have direct := List.all_eq_true.mp complete.1
  have closed := List.all_eq_true.mp complete.2
  induction reachable with
  | direct member => simpa using direct _ member
  | @alias target source edge _ ih =>
      have rule := closed (target, source) edge
      simpa [ih] using rule

set_option maxHeartbeats 50000 in
theorem alias_path_invalidates_provider (edges : AliasEdges) (written : List ScopedName)
    (imports : List (ScopedName × String)) (name : ScopedName) (owner : String)
    (reachable : WriteReach edges written name) (imported : (name, owner) ∈ imports) :
    aliasTrusted edges written imports owner = false := by
  unfold aliasTrusted
  dsimp only
  by_cases complete : closureComplete edges written (writeClosure edges.length edges written) = true
  · have marked := closed_writes_cover_alias_paths edges written _ complete name reachable
    have affected : imports.any (fun (name, origin) =>
        (writeClosure edges.length edges written).contains name && origin == owner) = true :=
      List.any_eq_true.mpr ⟨(name, owner), imported, by simp [marked]⟩
    rw [affected]
    simp
  · simp [complete]

-- Equal spellings in distinct lexical scopes are different graph vertices.
set_option maxHeartbeats 50000 in
theorem distinct_scopes_have_distinct_names (a b : Nat) (name : String) (h : a ≠ b) :
    (a, name) ≠ (b, name) := by
  intro equal
  exact h (congrArg Prod.fst equal)

-- Even an alias write cannot reach the same spelling in another scope without
-- an actual edge connecting those identities.
set_option maxHeartbeats 50000 in
theorem isolated_scope_write_preserves_provider (a b : Nat) (name owner : String)
    (h : a ≠ b) : aliasTrusted [] [(a, name)] [((b, name), owner)] owner = true := by
  simp [aliasTrusted, writeClosure, closureComplete, Ne.symm h]

end HoiminOracle.ExceptionHierarchy
