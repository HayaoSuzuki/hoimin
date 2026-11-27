namespace HoiminOracle.LineCoverage

structure Span where
  start : Nat
  finish : Nat
  deriving DecidableEq, BEq

def Contains (span : Span) (line : Nat) : Prop := span.start ≤ line ∧ line ≤ span.finish

instance (span : Span) (line : Nat) : Decidable (Contains span line) := by
  unfold Contains
  infer_instance

def coveredAt (spans : List Span) (line : Nat) : Bool :=
  spans.any fun span => decide (Contains span line)

theorem covered_at_iff (spans : List Span) (line : Nat) :
    coveredAt spans line = true ↔ ∃ span ∈ spans, Contains span line := by
  simp [coveredAt]

def breakpoints (spans : List Span) (query : Span) : List Nat :=
  query.start :: spans.map (·.finish + 1)

def covers (spans : List Span) (query : Span) : Bool :=
  decide (query.start ≤ query.finish) &&
    (breakpoints spans query).all fun line => !(decide (Contains query line)) || coveredAt spans line

theorem covers_iff (spans : List Span) (query : Span) :
    covers spans query = true ↔
      query.start ≤ query.finish ∧ ∀ line, Contains query line → ∃ span ∈ spans, Contains span line := by
  constructor
  · intro result
    unfold covers at result
    obtain ⟨ordered, checkpoints⟩ := Bool.and_eq_true_iff.mp result
    have valid := of_decide_eq_true ordered
    have atCheckpoint (line : Nat) (breakpoint : line ∈ breakpoints spans query)
        (inside : Contains query line) : ∃ span ∈ spans, Contains span line := by
      have hit : coveredAt spans line = true := by
        simpa [inside] using List.all_eq_true.mp checkpoints line breakpoint
      exact (covered_at_iff spans line).mp hit
    have byOffset (offset : Nat) (within : query.start + offset ≤ query.finish) :
        ∃ span ∈ spans, Contains span (query.start + offset) := by
      induction offset with
      | zero =>
        exact atCheckpoint query.start (by simp [breakpoints]) ⟨Nat.le_refl _, valid⟩
      | succ offset ih =>
        obtain ⟨span, member, previous⟩ := ih (by omega)
        by_cases continues : query.start + (offset + 1) ≤ span.finish
        · exact ⟨span, member, ⟨by unfold Contains at previous; omega, continues⟩⟩
        · have next : query.start + (offset + 1) = span.finish + 1 := by
            unfold Contains at previous
            omega
          apply atCheckpoint
          · rw [next]
            exact List.mem_cons_of_mem _ (List.mem_map_of_mem member)
          · exact ⟨by omega, within⟩
    refine ⟨valid, fun line inside => ?_⟩
    have start : query.start + (line - query.start) = line := by
      unfold Contains at inside
      omega
    simpa only [start] using byOffset (line - query.start) (by rw [start]; exact inside.2)
  · rintro ⟨valid, allLines⟩
    unfold covers
    apply Bool.and_eq_true_iff.mpr
    refine ⟨decide_eq_true valid, List.all_eq_true.mpr ?_⟩
    intro line _
    by_cases inside : Contains query line
    · have hit := (covered_at_iff spans line).mpr (allLines line inside)
      simp [inside, hit]
    · simp [inside]

theorem adjacent_union (first second : Span) (line : Nat)
    (ordered : first.start ≤ second.start) (touches : second.start ≤ first.finish + 1) :
    Contains ⟨first.start, max first.finish second.finish⟩ line ↔
      Contains first line ∨ Contains second line := by
  change (first.start ≤ line ∧ line ≤ max first.finish second.finish) ↔
    (first.start ≤ line ∧ line ≤ first.finish) ∨
      (second.start ≤ line ∧ line ≤ second.finish)
  omega

-- Start lines without a selected predecessor are the starts of maximal
-- disjoint spans. Their ends must be ends of input spans.
def normalizedChanges (input : List Span) : List Span :=
  let spans := input.filter fun span => span.start > 0 && span.start ≤ span.finish
  let starts := ((spans.map Span.start).eraseDups).mergeSort (· ≤ ·)
  starts.filterMap fun start =>
    if coveredAt spans (start - 1) then none
    else
      let ends := (spans.map Span.finish).filter fun finish => covers spans ⟨start, finish⟩
      ends.max?.map fun finish => ⟨start, finish⟩

inductive Path | first | second
  deriving DecidableEq, BEq

structure Entry where
  path : Path
  span : Span

structure Target where
  path : Path
  spans : List Span

def rangesAt (input : List Entry) (path : Path) : List Span :=
  (input.filter (·.path == path)).map Entry.span

def targetPaths (input : List Entry) (targets : List Target) : List Path :=
  ([.first, .second] : List Path).filter fun path =>
    let matching := targets.filter (·.path == path)
    !matching.isEmpty && matching.all fun target =>
      !target.spans.isEmpty && target.spans.all (covers (rangesAt input path))

end HoiminOracle.LineCoverage
