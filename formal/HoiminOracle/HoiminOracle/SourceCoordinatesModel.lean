namespace HoiminOracle.SourceCoordinates

abbrev Width := Nat × Nat

def reverseWidths (widths : List Width) : List Width := widths.map Prod.swap

def Valid (widths : List Width) : Prop := ∀ width ∈ widths, 0 < width.1 ∧ 0 < width.2

def translate : List Width → Nat → Option Nat
  | [], offset => if offset = 0 then some 0 else none
  | (input, output) :: more, offset =>
    if offset = 0 then some 0
    else if offset < input then none
    else (translate more (offset - input)).map (output + ·)

def inputLength (widths : List Width) : Nat := (widths.map Prod.fst).sum
def outputLength (widths : List Width) : Nat := (widths.map Prod.snd).sum

theorem translated_boundary_recovers_original (widths : List Width) (valid : Valid widths)
    (input output : Nat) (mapped : translate widths input = some output) :
    translate (reverseWidths widths) output = some input := by
  induction widths generalizing input output with
  | nil =>
    by_cases zero : input = 0
    · subst input
      have : output = 0 := by simpa [translate] using mapped.symm
      subst output
      rfl
    · simp [translate, zero] at mapped
  | cons width rest ih =>
    obtain ⟨rawWidth, utfWidth⟩ := width
    have positive := valid (rawWidth, utfWidth) (by simp)
    have tailValid : Valid rest := fun width member => valid width (by simp [member])
    by_cases zero : input = 0
    · subst input
      have : output = 0 := by simpa [translate] using mapped.symm
      subst output
      rfl
    · by_cases splitScalar : input < rawWidth
      · simp [translate, zero, splitScalar] at mapped
      · cases tailResult : translate rest (input - rawWidth) with
        | none => simp [translate, zero, splitScalar, tailResult] at mapped
        | some converted =>
          have ends : output = utfWidth + converted := by
            simpa [translate, zero, splitScalar, tailResult] using mapped.symm
          have reversed := ih tailValid (input - rawWidth) converted tailResult
          have nonzero : utfWidth + converted ≠ 0 := by
            have widthPositive := positive.2
            omega
          have fits : ¬ utfWidth + converted < utfWidth := by omega
          have restored : rawWidth + (input - rawWidth) = input := by omega
          rw [ends]
          simp only [reverseWidths, List.map_cons, Prod.swap, translate, if_neg nonzero, if_neg fits,
            Nat.add_sub_cancel_left]
          change (translate (reverseWidths rest) converted).map (rawWidth + ·) = some input
          simp [reversed, restored]

theorem boundary_of_appended_prefix (before after : List Width) (valid : Valid before) :
    translate (before ++ after) (inputLength before) = some (outputLength before) := by
  induction before with
  | nil => cases after with
    | nil => rfl
    | cons width rest => cases width <;> rfl
  | cons width rest ih =>
    obtain ⟨rawWidth, utfWidth⟩ := width
    have positive := (valid (rawWidth, utfWidth) (by simp)).1
    have tailValid : Valid rest := fun width member => valid width (by simp [member])
    have nonzero : rawWidth + (rest.map Prod.fst).sum ≠ 0 := by omega
    have fits : ¬ rawWidth + (rest.map Prod.fst).sum < rawWidth := by omega
    simpa only [List.cons_append, inputLength, outputLength, List.map_cons, Prod.fst, Prod.snd,
      List.sum_cons, translate, if_neg nonzero, if_neg fits, Nat.add_sub_cancel_left, Option.map_some]
      using congrArg (Option.map (utfWidth + ·)) (ih tailValid)

def latin1Width (byte : UInt8) : Width := (1, if byte < 128 then 1 else 2)

def latin1Widths (bytes : List UInt8) : List Width := bytes.map latin1Width

theorem latin1_widths_are_positive (bytes : List UInt8) : Valid (latin1Widths bytes) := by
  intro width member
  obtain ⟨byte, _, rfl⟩ := List.mem_map.mp member
  simp only [latin1Width]
  split <;> decide

theorem latin1_input_length (bytes : List UInt8) : inputLength (latin1Widths bytes) = bytes.length := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih => simp [inputLength, latin1Widths, latin1Width] at ih ⊢; omega

theorem latin1_output_length (bytes : List UInt8) :
    outputLength (latin1Widths bytes) = bytes.length + (bytes.filter (fun byte => 128 ≤ byte)).length := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
    have order : (128 : UInt8) ≤ byte ↔ ¬byte < 128 := by
      rw [UInt8.le_iff_toNat_le, UInt8.lt_iff_toNat_lt]
      omega
    by_cases short : byte < 128
    · simp [outputLength, latin1Widths, latin1Width, order, short] at ih ⊢
      omega
    · simp [outputLength, latin1Widths, latin1Width, order, short] at ih ⊢
      omega

theorem latin1_raw_boundary_by_prefix (bytes : List UInt8) (offset : Nat) (within : offset ≤ bytes.length) :
    translate (latin1Widths bytes) offset =
      some (offset + ((bytes.take offset).filter (fun byte => 128 ≤ byte)).length) := by
  have partition : latin1Widths (bytes.take offset) ++ latin1Widths (bytes.drop offset) = latin1Widths bytes := by
    unfold latin1Widths
    rw [← List.map_append, List.take_append_drop]
  have boundary := boundary_of_appended_prefix (latin1Widths (bytes.take offset))
    (latin1Widths (bytes.drop offset)) (latin1_widths_are_positive (bytes.take offset))
  rw [partition, latin1_input_length, latin1_output_length] at boundary
  simpa [Nat.min_eq_left within] using boundary

end HoiminOracle.SourceCoordinates
