namespace HoiminOracle.DelimiterDepth

abbrev Pair := Nat × Nat

def limit : Nat := 200

def depthAt (pairs : List Pair) (offset : Nat) : Nat :=
  (pairs.filter fun (opening, closing) => opening ≤ offset && offset < closing).length

def depthBefore (pairs : List Pair) (offset : Nat) : Nat :=
  (pairs.filter fun (opening, closing) => opening < offset && offset < closing).length

def fits (added : Nat) (depths : List Nat) : Bool :=
  depths.all fun depth => depth + added ≤ limit

def roomBefore (pairs : List Pair) (offset added : Nat) : Bool :=
  depthBefore pairs offset + added ≤ limit

def canEnclose (pairs : List Pair) (start stop added : Nat) : Bool :=
  if start = stop then roomBefore pairs start added
  else fits added ((List.range (stop - start)).map fun n => depthAt pairs (start + n))

theorem fits_iff (added : Nat) (depths : List Nat) :
    fits added depths = true ↔ ∀ depth ∈ depths, depth + added ≤ limit := by
  simp [fits, List.all_eq_true]

theorem smaller_addition_fits {smaller larger : Nat} {depths : List Nat} (order : smaller ≤ larger)
    (allowed : fits larger depths = true) : fits smaller depths = true := by
  rw [fits_iff] at allowed ⊢
  intro depth member
  have bound := allowed depth member
  omega

theorem exact_limit_fits {depth added : Nat} (exactLimit : depth + added = limit) :
    fits added [depth] = true := by
  simp [fits, exactLimit]

theorem empty_range_is_insertion (pairs : List Pair) (offset added : Nat) :
    canEnclose pairs offset offset added = roomBefore pairs offset added := by
  simp [canEnclose]

theorem insertion_touches_pair (opening closing added : Nat) :
    roomBefore [(opening, closing)] opening added = decide (added ≤ limit) ∧
    roomBefore [(opening, closing)] closing added = decide (added ≤ limit) := by
  simp [roomBefore, depthBefore]

theorem enclose_respects_budget (pairs : List Pair) {start stop added : Nat} (nonempty : start < stop) :
    canEnclose pairs start stop added = true ↔
      ∀ offset, start ≤ offset → offset < stop → depthAt pairs offset + added ≤ limit := by
  simp only [canEnclose, if_neg (Nat.ne_of_lt nonempty), fits_iff, List.mem_map, List.mem_range]
  constructor
  · intro allowed offset fromStart beforeStop
    apply allowed
    exact ⟨offset - start, by omega, by rw [Nat.add_sub_of_le fromStart]⟩
  · intro allowed depth ⟨index, within, equation⟩
    rw [← equation]
    exact allowed (start + index) (by omega) (by omega)

theorem disjoint_iff_no_common_byte {opening closing start stop : Nat}
    (pairNonempty : opening < closing) (rangeNonempty : start < stop) :
    (closing ≤ start ∨ stop ≤ opening) ↔
      ¬∃ offset, opening ≤ offset ∧ offset < closing ∧ start ≤ offset ∧ offset < stop := by
  constructor
  · intro disjoint ⟨offset, afterOpening, beforeClosing, afterStart, beforeStop⟩
    rcases disjoint with finished | notStarted <;> omega
  · intro noCommon
    by_cases finished : closing ≤ start
    · exact Or.inl finished
    by_cases notStarted : stop ≤ opening
    · exact Or.inr notStarted
    exfalso
    apply noCommon
    exact ⟨max opening start, by omega, by omega, by omega, by omega⟩

theorem enclose_matches_covering_intervals (pairs forbidden : List Pair)
    {start stop added : Nat} (nonempty : start < stop)
    (proper : ∀ pair ∈ forbidden, pair.1 < pair.2)
    (coverage : ∀ offset, limit < depthAt pairs offset + added ↔
      ∃ pair ∈ forbidden, pair.1 ≤ offset ∧ offset < pair.2) :
    canEnclose pairs start stop added = true ↔
      ∀ pair ∈ forbidden, pair.2 ≤ start ∨ stop ≤ pair.1 := by
  rw [enclose_respects_budget pairs nonempty]
  constructor
  · intro allowed pair member
    apply (disjoint_iff_no_common_byte (proper pair member) nonempty).mpr
    intro ⟨offset, insideStart, insideEnd, fromStart, beforeStop⟩
    have over := (coverage offset).mpr ⟨pair, member, insideStart, insideEnd⟩
    have within := allowed offset fromStart beforeStop
    omega
  · intro disjoint offset fromStart beforeStop
    by_cases within : depthAt pairs offset + added ≤ limit
    · exact within
    obtain ⟨pair, member, insideStart, insideEnd⟩ := (coverage offset).mp (by omega)
    rcases disjoint pair member with finished | notStarted <;> omega

end HoiminOracle.DelimiterDepth
