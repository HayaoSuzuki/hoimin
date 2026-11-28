namespace HoiminOracle.RegexLiteralSpan

abbrev Span := Nat × Nat

structure Fragment where
  source : String
  value : Option Char

structure Unit where
  value : Char
  source : Span

def place : Nat → List Fragment → List Unit
  | _, [] => []
  | start, fragment :: rest =>
    let stop := start + fragment.source.utf8ByteSize
    let later := place stop rest
    match fragment.value with
    | some value => ⟨value, (start, stop)⟩ :: later
    | none => later

def project (units : List Unit) (lastBodyEnd start stop : Nat) : Option Span := do
  if start = stop then
    let point := (units[start]?.map fun unit => unit.source.1).getD lastBodyEnd
    return (point, point)
  else
    let first ← units[start]?
    let last ← units[stop - 1]?
    return (first.source.1, last.source.2)

inductive Edit
  | replace (value : String)
  | surround (before after : String)

def revised (units : List Char) (start stop : Nat) (edit : Edit) : List Char :=
  let selected := (units.take stop).drop start
  let middle := match edit with
    | .replace value => value.toList
    | .surround before after => before.toList ++ selected ++ after.toList
  units.take start ++ middle ++ units.drop stop

theorem fragment_without_value_skips_bytes (start : Nat) (spelling : String) (rest : List Fragment) :
    place start (⟨spelling, none⟩ :: rest) = place (start + spelling.utf8ByteSize) rest := rfl

theorem emitted_unit_covers_complete_spelling (start : Nat) (spelling : String) (value : Char)
    (rest : List Fragment) :
    (place start (⟨spelling, some value⟩ :: rest)).head? =
      some ⟨value, (start, start + spelling.utf8ByteSize)⟩ := rfl

theorem placement_preserves_decoded_order (fragments : List Fragment) (start : Nat) :
    (place start fragments).map Unit.value = fragments.filterMap Fragment.value := by
  induction fragments generalizing start with
  | nil => rfl
  | cons fragment rest inductiveHypothesis =>
    rcases fragment with ⟨source, value⟩
    cases value <;> simp [place, inductiveHypothesis]

theorem one_unit_projects_exact_spelling (units : List Unit) (bodyEnd index : Nat) (unit : Unit)
    (atIndex : units[index]? = some unit) :
    project units bodyEnd index (index + 1) = some unit.source := by
  rcases unit with ⟨value, opening, closing⟩
  simp [project, atIndex, bind, Option.bind]

theorem insertion_at_end_uses_last_token (units : List Unit) (lastBodyEnd : Nat) :
    project units lastBodyEnd units.length units.length = some (lastBodyEnd, lastBodyEnd) := by
  simp [project]

theorem surround_keeps_selected_units (before selected after : List Char) (left right : String) :
    revised (before ++ selected ++ after) before.length (before.length + selected.length)
      (.surround left right) = before ++ (left.toList ++ selected ++ right.toList) ++ after := by
  simp [revised, List.take_append, List.drop_append]

end HoiminOracle.RegexLiteralSpan
