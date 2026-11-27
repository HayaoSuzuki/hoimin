import HoiminOracle.LineCoverageModel
import HoiminOracle.SymbolCatalogModel

namespace HoiminOracle.CandidateSelection

open LineCoverage (Span Contains)
open SymbolCatalog (Name Selector selected)

def overlaps (range query : Span) : Bool :=
  range.start ≤ range.finish && range.start ≤ query.finish && query.start ≤ range.finish

def lineSelected (ranges : List Span) (query : Span) : Bool :=
  ranges.isEmpty || ranges.any (overlaps · query)

def startsBefore (last : Nat) (range : Span) : Bool := range.start ≤ last

def byStart (left right : Span) : Bool := left.start ≤ right.start

def prepared (ranges : List Span) : List Span :=
  (ranges.filter fun range => range.start ≤ range.finish).mergeSort byStart

def indexedLineSelected (ranges : List Span) (query : Span) : Bool :=
  ranges.isEmpty ||
    (((prepared ranges).takeWhile (startsBefore query.finish)).map Span.finish).max?.any
      (fun finish => query.start ≤ finish)

def admitted (ranges : List Span) (selectors : List Selector) (modulePath scope : Name)
    (query : Span) : Bool :=
  lineSelected ranges query && selected modulePath selectors scope

theorem hit_iff_common_line (range query : Span) (ordered : query.start ≤ query.finish) :
    overlaps range query = true ↔ ∃ line, Contains range line ∧ Contains query line := by
  simp only [overlaps, Bool.and_eq_true_iff, decide_eq_true_eq]
  constructor
  · rintro ⟨⟨rangeValid, startsEarly⟩, finishesLate⟩
    exact ⟨max range.start query.start, ⟨by omega, by omega⟩, ⟨by omega, by omega⟩⟩
  · rintro ⟨line, ⟨afterRange, beforeRange⟩, ⟨afterQuery, beforeQuery⟩⟩
    exact ⟨⟨by omega, by omega⟩, by omega⟩

theorem eligible_starts_form_prefix (ranges : List Span) (last : Nat)
    (sorted : ranges.Pairwise (fun a b => a.start ≤ b.start)) :
    ranges.takeWhile (startsBefore last) = ranges.filter (startsBefore last) := by
  induction ranges with
  | nil => rfl
  | cons range rest ih =>
    obtain ⟨leRest, orderedRest⟩ := List.pairwise_cons.mp sorted
    by_cases eligible : range.start ≤ last
    · simp [startsBefore, eligible, ih orderedRest]
    · have noLater : rest.filter (startsBefore last) = [] := by
        apply List.filter_eq_nil_iff.mpr
        intro other inRest
        have order := leRest other inRest
        simp only [startsBefore, decide_eq_true_eq]
        omega
      simpa [startsBefore, eligible, List.takeWhile_cons] using noLater.symm

theorem max_end_answers_any_eligible_end (ends : List Nat) (first : Nat) :
    ends.max?.any (fun finish => first ≤ finish) = ends.any (fun finish => first ≤ finish) := by
  cases result : ends.max? with
  | none =>
    have empty := List.max?_eq_none_iff.mp result
    simp [empty]
  | some maximum =>
    obtain ⟨member, isMax⟩ := List.max?_eq_some_iff.mp result
    apply Bool.eq_iff_iff.mpr
    simp only [Option.any_some, decide_eq_true_eq, List.any_eq_true]
    exact ⟨fun enough => ⟨maximum, member, enough⟩,
      fun ⟨finish, inEnds, enough⟩ => Nat.le_trans enough (isMax finish inEnds)⟩

theorem prepared_is_sorted (ranges : List Span) :
    (prepared ranges).Pairwise (fun a b => a.start ≤ b.start) := by
  have sorted := List.pairwise_mergeSort (le := byStart)
    (fun a b c ab bc => by simp [byStart] at ab bc ⊢; exact Nat.le_trans ab bc)
    (fun a b => by simp [byStart]; exact Nat.le_total _ _)
    (ranges.filter fun range => range.start ≤ range.finish)
  simpa [prepared, byStart] using sorted

theorem prefix_max_preserves_line_selection (ranges : List Span) (query : Span) :
    indexedLineSelected ranges query = lineSelected ranges query := by
  unfold indexedLineSelected lineSelected
  rw [eligible_starts_form_prefix _ _ (prepared_is_sorted ranges), max_end_answers_any_eligible_end]
  congr 1
  apply Bool.eq_iff_iff.mpr
  simp only [List.any_eq_true, List.mem_map, List.mem_filter]
  constructor
  · rintro ⟨finish, ⟨range, ⟨member, beforeEnd⟩, rfl⟩, afterStart⟩
    have inOriginal : range ∈ ranges ∧ range.start ≤ range.finish := by
      simpa [prepared] using member
    refine ⟨range, inOriginal.1, ?_⟩
    have startsEarly : range.start ≤ query.finish := by simpa [startsBefore] using beforeEnd
    have endsLate : query.start ≤ range.finish := of_decide_eq_true afterStart
    simp [overlaps, inOriginal.2, startsEarly, endsLate]
  · rintro ⟨range, member, hit⟩
    have valid := Bool.and_eq_true_iff.mp hit
    have left := Bool.and_eq_true_iff.mp valid.1
    refine ⟨range.finish, ⟨range, ⟨?_, left.2⟩, rfl⟩, valid.2⟩
    simpa [prepared] using And.intro member (of_decide_eq_true left.1)

end HoiminOracle.CandidateSelection
