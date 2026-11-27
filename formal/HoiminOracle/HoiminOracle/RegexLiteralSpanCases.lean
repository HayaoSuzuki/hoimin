import HoiminOracle.RegexLiteralSpanModel
import Lean.Data.Json

namespace HoiminOracle.RegexLiteralSpan

private structure Token where
  opening : String
  body : List Fragment
  closing : String
  after : String

private def single (body : List Fragment) (after : String := "") : Token :=
  ⟨"'", body, "'", after⟩

private def literal (text : String) : List Fragment :=
  text.toList.map fun value => ⟨String.singleton value, some value⟩

private def emits (source : String) (value : Char) : Fragment := ⟨source, some value⟩
private def skipBytes (source : String) : Fragment := ⟨source, none⟩

private def Token.spelling (token : Token) : String :=
  token.opening ++ String.join (token.body.map Fragment.source) ++ token.closing ++ token.after

private def placeTokens : Nat → List Token → List Unit × Nat
  | start, [] => ([], start)
  | start, token :: rest =>
    let firstByte := start + token.opening.utf8ByteSize
    let bodyEnd := firstByte + (token.body.map fun fragment => fragment.source.utf8ByteSize).sum
    let rendered := place firstByte token.body
    match rest with
    | [] => (rendered, bodyEnd)
    | _ :: _ =>
      let (more, lastBodyEnd) := placeTokens (start + token.spelling.utf8ByteSize) rest
      (rendered ++ more, lastBodyEnd)

private structure EditQuery where
  start : Nat
  stop : Nat
  edit : Edit

private structure Fixture where
  id : String
  tokens : List Token
  edits : List EditQuery

private def samples : List Fixture := [
  ⟨"escaped-and-utf8", [single (literal "é" ++
    [emits "\\u96EA" '雪'] ++ literal "\\q" ++ [emits "\\\\" '\\', emits "\\n" '\n'] ++ literal "A")],
    [⟨1, 2, .replace "\\Z"⟩, ⟨2, 4, .replace ""⟩, ⟨4, 6, .surround "(" ")"⟩]⟩,
  ⟨"bytes", [⟨"b'", [emits "\\xff" (Char.ofNat 255), emits "\\400" (Char.ofNat 0)] ++
    literal "\\u0041Q", "'", ""⟩], [⟨0, 2, .replace "XY"⟩, ⟨2, 4, .replace ""⟩]⟩,
  ⟨"raw", [⟨"R'", literal "\\q雪", "'", ""⟩], [⟨0, 2, .surround "(?a:" ")"⟩]⟩,
  ⟨"mixed-tokens", [⟨"r'", literal "[ab", "'", " # left ご\n  "⟩,
    single [emits "\\x5d" ']'] " # right\n  ", single []],
    [⟨1, 4, .replace "X"⟩, ⟨0, 4, .surround "(?a:" ")"⟩]⟩,
  ⟨"continuations", [single (literal "A" ++ [skipBytes "\\\n"] ++ literal "B" ++ [skipBytes "\\\r\n"] ++ literal "C")
        " # bridge 雪\n  ",
    single ([skipBytes "\\\r\n"] ++ literal "D")],
    [⟨1, 4, .replace "X"⟩, ⟨0, 4, .replace ""⟩, ⟨0, 4, .surround "(" ")"⟩]⟩,
  ⟨"physical-newlines", [⟨"'''", literal "A" ++ [emits "\r\n" '\n'] ++ literal "é", "'''", " "⟩,
    ⟨"r\"\"\"", literal "雪" ++ [emits "\r\n" '\n', emits "\r" '\n'] ++ literal "B", "\"\"\"", ""⟩],
    [⟨2, 4, .replace "X"⟩]⟩,
  ⟨"named-escape", [single (literal "é" ++ [emits "\\N{LATIN SMALL LETTER A}" 'a'] ++ literal "雪")],
    [⟨1, 2, .replace "X"⟩]⟩,
  ⟨"adjacent-empty-first", [single (literal "one"), single (literal "|two")], [⟨0, 4, .replace ""⟩]⟩,
  ⟨"adjacent-empty-last", [single (literal "A"), single (literal "B"), single (literal "C")],
    [⟨0, 2, .replace ""⟩]⟩,
  ⟨"odd-raw-tail", [⟨"r'", literal "\\\\", "'", " "⟩, single (literal "d")],
    [⟨1, 3, .surround "(?a:" ")"⟩]⟩,
  ⟨"empty-final", [single (literal "A") " ", single [] " ", single [skipBytes "\\\n"]], []⟩,
  ⟨"all-empty", [single [] " ", single [skipBytes "\\\r\n"]], []⟩]

private def header := "(\n  # 雪\n  "

private def layout (fixture : Fixture) : List Unit × Nat := placeTokens header.utf8ByteSize fixture.tokens

private def queries (count : Nat) : List Span :=
  (List.range (count + 1)).flatMap (fun start =>
    (List.range (count + 1 - start)).map fun size => (start, start + size)) ++
  [(0, count + 1), (count + 1, count + 2)]

private def spanJson (range : Span) : Lean.Json := .arr #[Lean.toJson range.1, Lean.toJson range.2]

private def editJson (values : List Char) (query : EditQuery) : Lean.Json :=
  let (kind, before, after) := match query.edit with
    | .replace value => ("replace", value, "")
    | .surround before after => ("surround", before, after)
  Lean.Json.mkObj [("span", spanJson (query.start, query.stop)),
    ("kind", .str kind), ("before", .str before), ("after", .str after),
    ("expected", .str (String.ofList (revised values query.start query.stop query.edit)))]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let (units, lastBodyEnd) := layout item
  let values := units.map Unit.value
  let ranges := (queries units.length).map fun (start, stop) =>
    Lean.Json.mkObj [("span", spanJson (start, stop)),
      ("expected", ((project units lastBodyEnd start stop).map spanJson).getD .null)]
  let preservedSource := (item.tokens.flatMap fun token =>
    let gaps := if token.after.contains '#' then [token.after] else []
    (token.body.filterMap fun fragment => if fragment.value.isNone then some fragment.source else none) ++ gaps).eraseDups
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("source", .str (header ++ String.join (item.tokens.map Token.spelling) ++ "\n)")),
    ("decoded", .str (String.ofList values)), ("ranges", .arr ranges.toArray),
    ("edits", .arr ((item.edits.map (editJson values)).toArray)), ("protected", Lean.toJson preservedSource)]

def corpus : String := String.join (samples.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("regex_span_return_decoded_offsets", fun _ _ start stop => some (start, stop)),
      ("regex_span_empty_left_affinity", fun units bodyEnd start stop =>
        if start = stop && 0 < start then
          let point := (units[start - 1]?.map fun unit => unit.source.2).getD bodyEnd
          some (point, point)
        else project units bodyEnd start stop),
      ("regex_span_stop_at_last_unit_start", fun units bodyEnd start stop =>
        if start < stop then do
          let projected ← project units bodyEnd start stop
          let final ← units[stop - 1]?
          pure (projected.1, final.source.1)
        else project units bodyEnd start stop),
      ("regex_span_clip_to_first_unit", fun units bodyEnd start stop =>
        project units bodyEnd start (min (start + 1) stop))] do
    let witness := samples.findSome? fun item =>
      let (units, bodyEnd) := layout item
      (queries units.length).findSome? fun (start, stop) =>
        if broken units bodyEnd start stop != project units bodyEnd start stop
        then some s!"{item.id} span={start}..{stop}" else none
    match witness with
    | some detail => IO.println s!"family={name} detected=true case={detail}"
    | none =>
      IO.eprintln s!"regex literal span corpus missed broken family {name}"
      return 2
  for (name, broken) in [
      ("regex_edit_consumes_exclusive_end", fun values (query : EditQuery) =>
        revised values query.start (query.stop + 1) query.edit),
      ("regex_edit_surround_discards_original", fun values query =>
        match query.edit with
        | .surround left right => revised values query.start query.stop (.replace (left ++ right))
        | _ => revised values query.start query.stop query.edit)] do
    let witness := samples.findSome? fun item =>
      let values := (layout item).1.map Unit.value
      item.edits.findSome? fun query =>
        if broken values query != revised values query.start query.stop query.edit
        then some s!"{item.id} span={query.start}..{query.stop}" else none
    match witness with
    | some detail => IO.println s!"family={name} detected=true case={detail}"
    | none =>
      IO.eprintln s!"regex literal edit corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.RegexLiteralSpan
