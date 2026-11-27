import HoiminOracle.FormatRemovalModel
import Lean.Data.Json

namespace HoiminOracle.FormatRemoval

private def chars (count : Nat) (char : Char) := String.ofList (List.replicate count char)

private def fieldStem (field : Field) : String :=
  let expr := chars field.grouping '(' ++ field.expression ++ chars field.grouping ')'
  let conversion := (field.conversion.map fun flag => "!" ++ String.singleton flag).getD ""
  "{" ++ expr ++ (if field.debug then " = " else "") ++ conversion

private def specText : Elements → String
  | .absent | .empty => ""
  | .text text rest => text ++ specText rest
  | .interpolation field nested rest =>
    let spec := match nested with | .absent => "" | _ => ":" ++ specText nested
    fieldStem field ++ spec ++ "}" ++ specText rest

private structure Edit where
  start : Nat
  original : String
  replacement : String
  deriving BEq

private abbrev Selector := Field → Nat → Elements → Option Removal

private def edits (choose : Selector) (quote : Char) (depth : Nat) :
    Nat → Elements → List Edit
  | _, .absent | _, .empty => []
  | start, .text name rest => edits choose quote depth (start + name.utf8ByteSize) rest
  | start, .interpolation field nested rest =>
    let colon := start + (fieldStem field).utf8ByteSize
    let ownSpec := match nested with | .absent => "" | _ => ":" ++ specText nested
    let changes := (choose field (depth + 1) nested).toList.map fun plan =>
      ⟨colon, ownSpec, renderReplacement quote plan⟩
    changes ++ edits choose quote (depth + 1) (colon + 1) nested ++
      edits choose quote depth (colon + ownSpec.utf8ByteSize + 1) rest

private def text (value : String) : Elements := .text value .empty

private def field (expression : String) (nested rest : Elements := .absent) : Elements :=
  .interpolation { expression } nested rest

private def call (function : String) (nested rest : Elements := .absent) : Elements :=
  .interpolation { expression := function ++ "()", innerDepth := 1 } nested rest

private structure Fixture where
  id : String
  layout : Elements
  quote : Char := '"'
  quoteCount : Nat := 1
  parens : Nat := 0
  before : String := "result = "
  raw : Bool := false

private def shapes : List (String × Elements) := [
  ("absent", .absent), ("empty", .empty), ("static", text "雪>4"),
  ("dynamic", field "width"),
  ("siblings", .text "0" (field "width" .absent (.text "." (field "precision")))),
  ("inner-static", field "width" (text "2")),
  ("inner-dynamic", field "width" (field "pad") (field "precision"))]

private def matrix : List Fixture :=
  ['\'', '"'].flatMap fun quote => [false, true].flatMap fun debug =>
    ([none, some 's', some 'r', some 'a'] : List (Option Char)).flatMap fun conversion =>
      shapes.map fun (id, spec) =>
        {
          id := s!"{quote.toNat}-{debug}-{conversion.map String.singleton |>.getD "implicit"}-{id}"
          layout := .interpolation { debug, conversion } spec .empty
          quote
        }

private def aroundDepth : List Fixture :=
  [194, 195, 196, 197].flatMap fun parens =>
    [("name", field "width"), ("call", call "width"),
     ("nested", field "width" (field "pad")), ("static", text "4")].map fun (id, spec) =>
      { id := s!"depth-{id}-{parens}", layout := field "value" spec, parens }

private def namedAssignment : Field := { expression := "width := 4", grouping := 1, kind := .assignment }
private def yieldRead : Field := { expression := "yield 4", grouping := 1, kind := .yield }

private def focused : List Fixture := [
  { id := "named-assignment", layout := field "value" (.interpolation namedAssignment .absent .empty) },
  { id := "yield", before := "def holder():\n    return ",
    layout := field "value" (.interpolation yieldRead .absent .empty) },
  {
    id := "assignment-with-editable-child"
    layout := field "value" (.interpolation namedAssignment (text "4") (field "height" (text "2")))
  },
  { id := "yield-with-editable-child", before := "def holder():\n    return ",
    layout := field "value" (.interpolation yieldRead (text "4") .empty) },
  { id := "nested-assignment", layout := field "value" (field "width"
    (.interpolation namedAssignment .absent .empty) (field "height" (text "4"))) },
  { id := "keywords-in-format-text", layout := field "value" (text "yield := 4") },
  {
    id := "grouped-read"
    layout := field "value" (.interpolation { expression := "幅", grouping := 2 } .absent .empty)
  },
  {
    id := "adjacent-outer-fields"
    layout := field "first" (field "width") (.text "雪" (field "second" (text "4")))
  },
  { id := "raw-dynamic", raw := true, layout := field "value" (.text "\\>" (field "width")) },
  { id := "triple-single", quote := '\'', quoteCount := 3,
    layout := field "value" (.text "雪\n" (field "width")) },
  { id := "triple-double", quoteCount := 3,
    layout := field "value" (.text "雪\n" (field "width")) },
  { id := "static-at-limit", parens := 199, layout := field "value" (text "4") },
  {
    id := "runtime-siblings"
    layout := call "probe" (call "width" .absent (.text "." (call "precision")))
  },
  {
    id := "runtime-debug"
    layout := .interpolation { expression := "probe()", innerDepth := 1, debug := true }
      (call "width") .empty
  },
  {
    id := "runtime-nested"
    layout := call "probe" (call "width" (call "pad"))
  },
  {
    id := "compiler-free-variables"
    before := "def outer(width, precision):\n    def inner(value):\n        return "
    layout := field "value" (field "width" .absent (.text "." (field "precision")))
  },
  {
    id := "compiler-class-cell"
    before := "class Owner:\n    def render(self, value):\n        return "
    layout := field "value" (call "super")
  }]

private def fixtures := matrix ++ aroundDepth ++ focused

private def firstBytes (fixture : Fixture) : String :=
  fixture.before ++ chars fixture.parens '(' ++ (if fixture.raw then "rf" else "f") ++
    chars fixture.quoteCount fixture.quote ++ "雪"

private def fixtureEdits (choose : Selector) (fixture : Fixture) : List Edit :=
  edits choose fixture.quote fixture.parens (firstBytes fixture).utf8ByteSize fixture.layout

private def editJson (edit : Edit) : Lean.Json :=
  Lean.Json.mkObj [("start", Lean.toJson edit.start), ("original", .str edit.original),
    ("replacement", .str edit.replacement)]

private def fixtureJson (fixture : Fixture) : Lean.Json :=
  let source := firstBytes fixture ++ specText fixture.layout ++ chars fixture.quoteCount fixture.quote ++
    chars fixture.parens ')' ++ "\n"
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str fixture.id),
    ("source", .str source), ("edits", .arr ((fixtureEdits select fixture).map editJson).toArray)]

def corpus : String := String.join (fixtures.map fun fixture => (fixtureJson fixture).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (family, broken) in [
      ("format_remove_empty_ordinary", fun parent depth spec =>
        match spec with | .empty => some .omit | _ => select parent depth spec),
      ("format_remove_ignore_unsafe_read", fun parent depth spec =>
        let reads := footprint spec
        if reads.blocksRemoval then some (.guard (needsRepr parent) reads.expressions)
        else select parent depth spec),
      ("format_remove_reverse_reads", fun parent depth spec =>
        (select parent depth spec).map fun
          | .guard repr reads => .guard repr reads.reverse
          | .omit => .omit),
      ("format_remove_forget_debug_repr", fun parent depth spec =>
        (select parent depth spec).map fun
          | .guard _ expressions => .guard false expressions
          | .omit => .omit),
      ("format_remove_allow_one_more_delimiter", fun parent depth spec => select parent (depth - 1) spec)] do
    match fixtures.find? (fun item => fixtureEdits broken item != fixtureEdits select item) with
    | some item => IO.println s!"family={family} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"format removal corpus missed broken family {family}"
      return 2
  return 0

end HoiminOracle.FormatRemoval
