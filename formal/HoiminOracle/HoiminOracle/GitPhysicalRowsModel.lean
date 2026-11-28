namespace HoiminOracle.GitPhysicalRows

inductive ByteClass | payload | cr | lf

inductive Token | payload | cr | lf | crlf

def classify (byte : UInt8) : ByteClass :=
  if byte == 13 then .cr else if byte == 10 then .lf else .payload

def tokenize : List ByteClass → List Token
  | [] => []
  | .cr :: .lf :: rest => .crlf :: tokenize rest
  | .cr :: rest => .cr :: tokenize rest
  | .lf :: rest => .lf :: tokenize rest
  | .payload :: rest => .payload :: tokenize rest

def gitEnd : Token → Bool
  | .lf | .crlf => true
  | _ => false

def pythonEnd : Token → Bool
  | .cr | .lf | .crlf => true
  | .payload => false

private def prepend (item : α) : List (List α) → List (List α)
  | [] => [[item]]
  | first :: rest => (item :: first) :: rest

/-- Split after delimiters, retaining them and omitting any empty trailing row. -/
def cut (endsRow : α → Bool) : List α → List (List α)
  | [] => []
  | item :: rest =>
    if endsRow item then [item] :: cut endsRow rest else prepend item (cut endsRow rest)

private theorem flatten_prepend (item : α) (rows : List (List α)) :
    (prepend item rows).flatten = item :: rows.flatten := by
  cases rows <;> rfl

theorem cut_preserves_input (endsRow : α → Bool) (input : List α) :
    (cut endsRow input).flatten = input := by
  induction input with
  | nil => rfl
  | cons item rest ih => by_cases mark : endsRow item <;> simp [cut, mark, flatten_prepend, ih]

private theorem no_empty_cut (endsRow : α → Bool) (input : List α) : [] ∉ cut endsRow input := by
  induction input with
  | nil => simp [cut]
  | cons item rest ih =>
    by_cases mark : endsRow item
    · simp [cut, mark, ih]
    · cases rows : cut endsRow rest with
      | nil => simp [cut, mark, rows, prepend]
      | cons first rest => simp_all [cut, prepend]

private theorem prepend_append (item : α) (left right : List (List α)) (nonempty : left ≠ []) :
    prepend item (left ++ right) = prepend item left ++ right := by
  cases left <;> simp_all [prepend]

private theorem refine_prepend (inner : α → Bool) (item : α) (rows : List (List α))
    (nonempty : [] ∉ rows) :
    (prepend item rows).flatMap (cut inner) =
      if inner item then [item] :: rows.flatMap (cut inner) else prepend item (rows.flatMap (cut inner)) := by
  cases rows with
  | nil => by_cases delimiter : inner item <;> simp [cut, prepend, delimiter]
  | cons first rest =>
    have firstRows : cut inner first ≠ [] := by
      intro empty
      have flattened := cut_preserves_input inner first
      rw [empty] at flattened
      have firstEmpty : first = [] := by simpa using flattened.symm
      exact nonempty (by simp [firstEmpty])
    by_cases delimiter : inner item
    · simp [cut, prepend, delimiter]
    · simpa [cut, prepend, delimiter] using
        (prepend_append item (cut inner first) (rest.flatMap (cut inner)) firstRows).symm

theorem cut_refines (outer inner : α → Bool) (input : List α)
    (shared : ∀ item, outer item = true → inner item = true) :
    (cut outer input).flatMap (cut inner) = cut inner input := by
  induction input with
  | nil => rfl
  | cons item rest ih =>
    by_cases stop : outer item
    · simp [cut, stop, shared item stop, ih]
    · simp only [cut, stop, Bool.false_eq_true, if_false]
      rw [refine_prepend inner item _ (no_empty_cut outer rest), ih]

def delimiters (source : List UInt8) : List Token := tokenize (source.map classify)

def layout (source : List UInt8) : List Nat :=
  (cut gitEnd (delimiters source)).map fun row => (cut pythonEnd row).length

def physicalCount (source : List UInt8) : Nat := (cut pythonEnd (delimiters source)).length

theorem physical_count_eq (source : List UInt8) : (layout source).sum = physicalCount source := by
  have shared (item : Token) : gitEnd item = true → pythonEnd item = true := by
    cases item <;> simp [gitEnd, pythonEnd]
  unfold physicalCount layout
  rw [← cut_refines gitEnd pythonEnd (delimiters source) shared, List.length_flatMap]

theorem layout_positive (source : List UInt8) (width : Nat) (member : width ∈ layout source) :
    0 < width := by
  obtain ⟨row, appears, rfl⟩ := List.mem_map.mp member
  apply Nat.pos_of_ne_zero
  intro zero
  have noRows : cut pythonEnd row = [] := List.length_eq_zero_iff.mp zero
  have unchanged := cut_preserves_input pythonEnd row
  rw [noRows] at unchanged
  have empty : row = [] := by simpa using unchanged.symm
  exact no_empty_cut gitEnd (delimiters source) (empty ▸ appears)

structure Span where
  start : Nat
  finish : Nat
  deriving BEq

inductive Diagnostic | malformed | outside
  deriving BEq

def prefixCount (widths : List Nat) (count : Nat) : Nat := (widths.take count).sum

def spanFor (widths : List Nat) (rows : Span) : Span :=
  ⟨1 + prefixCount widths (rows.start - 1), prefixCount widths rows.finish⟩

theorem prefix_monotone (widths : List Nat) (first last : Nat) (h : first ≤ last) :
    prefixCount widths first ≤ prefixCount widths last := by
  have split := congrArg List.sum (List.take_append_drop first (widths.take last))
  rw [List.take_take, Nat.min_eq_left h, List.sum_append_nat] at split
  unfold prefixCount
  omega

theorem span_order (widths : List Nat) (first second : Span) (ordered : first.finish < second.start) :
    (spanFor widths first).finish < (spanFor widths second).start := by
  have count := prefix_monotone widths first.finish (second.start - 1) (by omega)
  simp only [spanFor]
  omega

theorem span_within_source (source : List UInt8) (span : Span)
    (positive : 0 < span.start) (ordered : span.start ≤ span.finish)
    (within : span.finish ≤ (layout source).length) :
    (spanFor (layout source) span).start ≤ (spanFor (layout source) span).finish ∧
      (spanFor (layout source) span).finish ≤ physicalCount source := by
  let widths := layout source
  let earlier := span.start - 1
  let throughEnd := widths.take span.finish
  have laterRows : (throughEnd.drop earlier) ≠ [] := by
    intro empty
    have size := congrArg List.length empty
    simp [throughEnd, widths, earlier, Nat.min_eq_left within] at size
    omega
  obtain ⟨width, rest, firstRow⟩ := List.exists_cons_of_ne_nil laterRows
  have seen : width ∈ throughEnd.drop earlier := by simp [firstRow]
  have inLayout : width ∈ widths := List.mem_of_mem_take (List.mem_of_mem_drop seen)
  have includedRows : 0 < (throughEnd.drop earlier).sum :=
    List.sum_pos_iff_exists_pos_nat.mpr ⟨width, seen, layout_positive source width inLayout⟩
  have partition := congrArg List.sum (List.take_append_drop earlier throughEnd)
  have earlierRows : throughEnd.take earlier = widths.take earlier := by
    have atMost : earlier ≤ span.finish := by dsimp [earlier]; omega
    simp [throughEnd, List.take_take, Nat.min_eq_left atMost]
  rw [List.sum_append_nat, earlierRows] at partition
  have withinTotal := prefix_monotone widths span.finish widths.length within
  simp only [prefixCount, List.take_length] at withinTotal
  change 1 + (widths.take earlier).sum ≤ throughEnd.sum ∧ throughEnd.sum ≤ physicalCount source
  exact ⟨by omega, (physical_count_eq source) ▸ withinTotal⟩

def translateFrom (widths : List Nat) : Nat → List Span → Except Diagnostic (List Span)
  | _, [] => .ok []
  | next, span :: more =>
    if span.start = 0 ∨ span.finish < span.start then .error .malformed
    else if span.start < next ∨ widths.length < span.finish then .error .outside
    else (translateFrom widths (span.finish + 1) more).map (spanFor widths span :: ·)

def translate (source : List UInt8) (ranges : List Span) : Except Diagnostic (List Span) :=
  translateFrom (layout source) 1 ranges

end HoiminOracle.GitPhysicalRows
