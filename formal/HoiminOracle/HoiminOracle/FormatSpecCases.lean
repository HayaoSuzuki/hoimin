import HoiminOracle.FormatSpecModel
import Lean.Data.Json

namespace HoiminOracle.FormatSpec

private inductive Part
  | glyph (code : Char) (spelling : String)
  | field (expression : String)

private def glyph (code : Char) : Part := .glyph code (String.singleton code)

private def Part.source : Part → String
  | .glyph _ spelling => spelling
  | .field expression => "{" ++ expression ++ "}"

private def Part.code : Part → Option Char
  | .glyph code _ => some code
  | .field _ => none

private def Part.plain : Part → Bool
  | .glyph code spelling => String.singleton code == spelling
  | .field _ => true

private def digits (value : String) := value.toList.map glyph

private structure Layout where
  fill : Option Part := none
  alignment : Option Part := none
  sign : Option Part := none
  negativeZero : Bool := false
  alternate : Bool := false
  zeroPadding : Bool := false
  width : List Part := []
  grouping : Option Part := none
  precision : Option (List Part) := none
  fractional : Option Part := none
  presentation : Option Part := none

private def Layout.slots (layout : Layout) : List (Option Slot × List Part) :=
  [(none, layout.fill.toList),
   (some .alignment, layout.alignment.toList), (some .sign, layout.sign.toList),
   (none, (if layout.negativeZero then [glyph 'z'] else []) ++
     (if layout.alternate then [glyph '#'] else []) ++ (if layout.zeroPadding then [glyph '0'] else [])),
   (some .width, layout.width), (some .grouping, layout.grouping.toList),
   (none, if layout.precision.isSome then [glyph '.'] else []),
   (some .precision, layout.precision.getD []), (some .fractional, layout.fractional.toList),
   (some .presentation, layout.presentation.toList)]

private def Layout.options (layout : Layout) : Options := {
  alignment := layout.alignment.bind Part.code
  sign := layout.sign.bind Part.code
  grouping := layout.grouping.bind Part.code
  fractional := layout.fractional.bind Part.code
  presentation := layout.presentation.bind Part.code
  precision := !(layout.precision.getD []).isEmpty
  negativeZero := layout.negativeZero
  alternate := layout.alternate
  zeroPadding := layout.zeroPadding || layout.fill.bind Part.code == some '0'
}

private structure Fixture where
  id : String
  expression : String := "value"
  subject : Subject := .unknown
  layout : Layout := {}
  unsupported : Option String := none
  raw : Bool := false

private def subjects : List (String × String × Subject) := [
  ("unknown", "value", .unknown), ("string", "\"label\"", .text),
  ("converted", "value!r", .text), ("integer", "-42", .integer), ("boolean", "True", .integer),
  ("float", "+0.5", .float), ("complex", "+-1j", .complex)]

private def layouts : List (String × Layout) := [
  ("plain", { alignment := some (glyph '>'), width := digits "4" }),
  ("grouped", {
    alignment := some (glyph '^'), sign := some (glyph '+'), width := digits "8",
    grouping := some (glyph ','), precision := some (digits "2") }),
  ("padded", {
    fill := some (glyph '0'), alignment := some (glyph '='), width := digits "8",
    grouping := some (glyph '_'), precision := some (digits "3"), fractional := some (glyph '_') }),
  ("fractional-default", {
    alignment := some (glyph '^'), width := digits "8", precision := some [],
    fractional := some (glyph ',') })]

private def matrix : List Fixture := subjects.flatMap fun (id, expression, subject) =>
  layouts.flatMap fun (name, layout) =>
    (none :: "bcdeEfFgGnosxX%".toList.map some).map fun presentation =>
      {
        id := s!"{id}-{name}-{presentation.map String.singleton |>.getD "default"}"
        expression, subject, layout := { layout with presentation := presentation.map glyph } }

private def focused : List Fixture := [
  { id := "two-holes", layout := {
      fill := some (.field "fill()"), alignment := some (glyph '>'),
      width := [.field "width()"], precision := some (digits "2"), presentation := some (glyph 'f') } },
  { id := "dynamic-width", layout := {
      zeroPadding := true, width := [.field "width"],
      precision := some (digits "2"), presentation := some (glyph 'f') } },
  { id := "dynamic-precision", layout := {
      alignment := some (glyph '>'), width := digits "8",
      precision := some [.field "precision"], presentation := some (glyph 'f') } },
  { id := "width-with-hole", layout := {
      width := digits "8" ++ [.field "width"],
      presentation := some (glyph 'f') } },
  {
    id := "escaped-align", expression := "value!s", subject := .text,
    layout := { alignment := some (.glyph '>' "\\x3e"), width := digits "4" } },
  { id := "escaped-sign", layout := {
      alignment := some (glyph '>'), sign := some (.glyph '+' "\\u002b"),
      width := digits "4", presentation := some (glyph 'd') } },
  { id := "raw-fill", raw := true, layout := {
      fill := some (glyph '\\'), alignment := some (glyph '>'),
      width := digits "4" } },
  { id := "unicode-fill", layout := {
      fill := some (glyph '🙂'), alignment := some (glyph '<'),
      width := digits "3" } },
  { id := "precision-zero", layout := { precision := some (digits "0"), presentation := some (glyph 'f') } },
  { id := "zero-flag-alone", layout := { zeroPadding := true } },
  { id := "negative-zero-float", expression := "-0.0", subject := .float, layout := {
      negativeZero := true,
      sign := some (glyph '+'), precision := some (digits "1"), presentation := some (glyph 'f') } },
  { id := "negative-zero-unknown", layout := { negativeZero := true, presentation := some (glyph 'f') } },
  { id := "alternate-c", layout := { alternate := true, presentation := some (glyph 'c') } },
  { id := "numeric-overflow64", layout := { width := digits "9223372036854775808" } },
  { id := "numeric-limit64", layout := { width := digits "9223372036854775807" } },
  { id := "numeric-below-limit64", layout := { width := digits "9223372036854775806" } },
  { id := "numeric-limit32", layout := { width := digits "2147483647" } },
  { id := "overlong-precision", layout := { precision := some (digits "00000000000000000001") } },
  { id := "unsupported-type", unsupported := some ">4custom" },
  { id := "missing-precision", unsupported := some ">4.f" },
  { id := "duplicate-grouping", unsupported := some "8,_f" },
  { id := "misplaced-type", unsupported := some "4f.2" },
  { id := "extra-semicolon", unsupported := some "2;d" }]

private structure Chunk where
  slot : Option Slot
  parts : List Part
  start : Nat
  stop : Nat

private def locate : Nat → List (Option Slot × List Part) → List Chunk
  | _, [] => []
  | start, (slot, parts) :: more =>
    let stop := start + (parts.map fun part => part.source.utf8ByteSize).sum
    ⟨slot, parts, start, stop⟩ :: locate stop more

private def numeric (parts : List Part) : Option Nat := do
  if parts.isEmpty || 19 < parts.length then none else
    let characters ← parts.mapM Part.code
    (String.ofList characters).toNat?

private structure Edit where
  kind : String
  start : Nat
  stop : Nat
  replacement : String
  deriving BEq

private def Layout.neighborSites (layout : Layout) : Bool :=
  let parts := layout.slots.flatMap Prod.snd
  parts.all Part.plain && (parts.filter (fun part => part.code.isNone)).length ≤ 1 &&
    (layout.fractional.isNone || !(layout.precision.getD []).isEmpty)

private structure Rules where
  options : Subject → Options → Slot → List String := replacements
  eligibility : Layout → Bool := Layout.neighborSites
  near : Nat → Nat → List Nat := neighbors
  optionWidth : Chunk → Nat := fun chunk => chunk.stop

private def renderStart (item : Fixture) : String :=
  (if item.raw then "rf" else "f") ++ "'雪{" ++ item.expression ++ ":"

private def edits (rules : Rules) (maximum : Nat) (item : Fixture) : List Edit :=
  if item.unsupported.isSome then [] else
    let chunks := locate (renderStart item).utf8ByteSize item.layout.slots
    let options := item.layout.options
    let static := fun slot => chunks.find? (fun chunk => chunk.slot == some slot)
    let values := if !rules.eligibility item.layout then [] else
      [.width, .precision].flatMap fun slot =>
        ((static slot).bind fun chunk => (numeric chunk.parts).map fun value =>
          (rules.near maximum value).map fun neighbor => ⟨"value", chunk.start, chunk.stop, toString neighbor⟩).getD []
    let swaps := [.alignment, .sign, .grouping, .fractional, .presentation].flatMap fun slot =>
      ((static slot).map fun chunk => (rules.options item.subject options slot).map fun replacement =>
        ⟨"option", chunk.start, rules.optionWidth chunk, replacement⟩).getD []
    values ++ swaps

private def editJson (edit : Edit) : Lean.Json :=
  Lean.Json.mkObj [("kind", .str edit.kind), ("start", Lean.toJson edit.start),
    ("end", Lean.toJson edit.stop), ("replacement", .str edit.replacement)]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let content := item.unsupported.getD (String.join (item.layout.slots.flatMap Prod.snd |>.map Part.source))
  let regular := edits {} (2^63 - 1) item
  let narrow := edits {} (2^31 - 1) item
  let render := fun values : List Edit => Lean.Json.arr (values.map editJson).toArray
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("source", .str (renderStart item ++ content ++ "}'")),
    ("edits32", if narrow == regular then .null else render narrow), ("edits", render regular)]

private def cases := matrix ++ focused

def corpus : String := String.join (cases.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let maximum := 2^63 - 1
  for (family, broken) in [
      ("format_forget_subject", { options := fun _ => replacements .unknown : Rules }),
      ("format_drop_locale_default", { options := fun subject options slot =>
        (replacements subject options slot).filter (!·.isEmpty) }),
      ("format_edit_ineligible_numeric_fields", { eligibility := fun _ => true }),
      ("format_increment_past_limit", {
        near := fun maxValue value =>
          neighbors maxValue value ++ (if value == maxValue then [value + 1] else []) }),
      ("format_use_single_byte_option_span", { optionWidth := fun chunk => chunk.start + 1 })] do
    match cases.find? (fun item => edits broken maximum item != edits {} maximum item) with
    | some item => IO.println s!"family={family} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"format spec corpus missed broken family {family}"
      return 2
  return 0

end HoiminOracle.FormatSpec
