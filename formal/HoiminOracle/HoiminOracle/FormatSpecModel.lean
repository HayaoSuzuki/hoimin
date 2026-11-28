namespace HoiminOracle.FormatSpec

inductive Subject | unknown | text | integer | float | complex
  deriving BEq

inductive Slot | alignment | sign | grouping | fractional | presentation | width | precision
  deriving BEq

structure Options where
  alignment : Option Char := none
  sign : Option Char := none
  grouping : Option Char := none
  fractional : Option Char := none
  presentation : Option Char := none
  precision : Bool := false
  negativeZero : Bool := false
  alternate : Bool := false
  zeroPadding : Bool := false

def integerCode (code : Char) : Bool := "bcdoxX".contains code

def subjectAllows (subject : Subject) (code : Option Char) : Bool :=
  match subject, code with
  | _, none => true
  | .text, some code => code == 's'
  | .unknown, some _ => true
  | .integer, some code => code != 's'
  | _, some code => !integerCode code && code != 's'

def valid (subject : Subject) (o : Options) : Bool :=
  let code := o.presentation.getD '\x00'
  let grouped := o.grouping.isSome || o.fractional.isSome
  let integerPresentation := integerCode code ||
    (subject == .integer && (o.presentation.isNone || code == 'n'))
  subjectAllows subject o.presentation &&
    (if subject == .text || code == 's' then
      o.sign.isNone && !o.negativeZero && !o.alternate && !grouped && o.alignment != some '='
    else
      (!integerPresentation || (!o.precision && !o.negativeZero && o.fractional.isNone)) &&
      (subject != .complex || (code != '%' && !o.zeroPadding && o.alignment != some '=')) &&
      (code != 'c' || (o.sign.isNone && !o.alternate && o.grouping.isNone)) &&
      (code != 'n' || !grouped) &&
      (o.grouping != some ',' || !"boxX".contains code))

def optionAt (o : Options) : Slot → Option Char
  | .alignment => o.alignment
  | .sign => o.sign
  | .grouping => o.grouping
  | .fractional => o.fractional
  | .presentation => o.presentation
  | .width | .precision => none

def withOption (o : Options) (value : Char) : Slot → Options
  | .alignment => { o with alignment := some value }
  | .sign => { o with sign := some value }
  | .grouping => { o with grouping := some value }
  | .fractional => { o with fractional := some value }
  | .presentation => { o with presentation := some value }
  | .width | .precision => o

def alphabet (subject : Subject) (o : Options) : Slot → String
  | .alignment =>
    if subject == .integer || subject == .float || o.presentation.any integerCode
    then "<>^=" else "<>^"
  | .sign => "+- "
  | .grouping | .fractional => ",_"
  | .presentation =>
    let knownRealOrComplex := subject == .float || subject == .complex
    let realOnly := o.precision || o.negativeZero
    match o.presentation with
    | none => ""
    | some code =>
      if integerCode code || (code == 'n' && subject == .integer) then "bdoxXn"
      else if code == 'n' then
        if knownRealOrComplex || realOnly then "eEfFgGn%" else ""
      else if "eEfFgG%".contains code then
        if knownRealOrComplex || !realOnly then "eEfFgGn%" else "eEfFgG%"
      else ""
  | .width | .precision => ""

def replacements (subject : Subject) (o : Options) (slot : Slot) : List String :=
  if !valid subject o then [] else
    match optionAt o slot with
    | none => []
    | some existing =>
      let eligible := (alphabet subject o slot).toList.filter fun replacement =>
        replacement != existing && valid subject (withOption o replacement slot)
      (eligible.map String.singleton) ++
        if slot == .presentation && existing == 'n' then [""] else []

def neighbors (maximum : Nat) (value : Nat) : List Nat :=
  if value > maximum then [] else
    (if value = 0 then [] else [value - 1]) ++
    (if value < maximum then [value + 1] else [])

theorem invalid_original_has_no_options (subject : Subject) (o : Options) (slot : Slot)
    (invalid : valid subject o = false) : replacements subject o slot = [] := by
  simp [replacements, invalid]

theorem emitted_character_replacement_is_valid (subject : Subject) (o : Options) (slot : Slot)
    (replacement : Char) (emitted : String.singleton replacement ∈ replacements subject o slot) :
    valid subject (withOption o replacement slot) = true := by
  unfold replacements at emitted
  split at emitted
  · simp at emitted
  · split at emitted
    · simp at emitted
    · rcases List.mem_append.mp emitted with candidate | localeDeletion
      · obtain ⟨ch, member, same⟩ := List.mem_map.mp candidate
        obtain rfl := String.singleton_inj.mp same
        exact (Bool.and_eq_true_iff.mp (List.mem_filter.mp member).2).2
      · split at localeDeletion
        · simp [String.singleton_ne_empty] at localeDeletion
        · simp at localeDeletion

theorem neighbor_zero_has_only_increment (maximum : Nat) (positive : 0 < maximum) :
    neighbors maximum 0 = [1] := by
  simp [neighbors, positive]

theorem neighbor_at_limit_has_only_decrement (maximum : Nat) (positive : 0 < maximum) :
    neighbors maximum maximum = [maximum - 1] := by
  simp [neighbors, Nat.ne_of_gt positive]

theorem neighbor_above_limit_is_ignored (maximum value : Nat) (above : maximum < value) :
    neighbors maximum value = [] := by
  simp [neighbors, above]

end HoiminOracle.FormatSpec
