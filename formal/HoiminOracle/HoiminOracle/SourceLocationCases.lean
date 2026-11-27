import HoiminOracle.SourceLocationModel
import Lean.Data.Json

namespace HoiminOracle.SourceLocation

private def bomText := String.singleton bom
private def acute := String.singleton (Char.ofNat 0x301)

private def alphabet : List String :=
  ["a", "\t", "é", "雪", "🦀", acute, bomText, "\r", "\n", "\r\n"]

private def fragments : List (List String) :=
  [[]] ++ alphabet.map (fun text => [text]) ++
    alphabet.flatMap (fun a => alphabet.map fun b => [a, b])

private def texts : List (String × String) :=
  ([false, true].flatMap fun initialBom =>
    fragments.zipIdx.map fun (pieces, index) =>
      (s!"matrix-{initialBom}-{index}", (if initialBom then bomText else "") ++ String.join pieces)) ++
  [("mixed", "雪🦀é\r\nβ\t\rx\n"),
   ("separators", String.ofList ['a', Char.ofNat 0x85, 'b', Char.ofNat 0x2028, 'c', Char.ofNat 0x2029, 'd', '\n']),
   ("interior-bom", "a" ++ bomText ++ "雪\n" ++ bomText ++ "🦀"),
   ("triple-bom", bomText ++ bomText ++ bomText ++ "\r\n"),
   ("many-rows", String.join (List.replicate 35 "é🦀\r\n")),
   ("many-scalars", String.join (List.replicate 31 "é雪🦀") ++ "\ré"),
   ("only-newlines", "\n\r\r\n\n\r"),
   ("combining", "e" ++ acute ++ "a" ++ String.singleton (Char.ofNat 0x308) ++ "\r\nb" ++ acute)]

private def locationJson (location : Location) : Lean.Json :=
  .arr #[Lean.toJson location.line, Lean.toJson location.column]

private def expectedJson (expected : Option Location) : Lean.Json :=
  (expected.map locationJson).getD .null

private def textJson (id input : String) : Lean.Json :=
  let boundaries := trace input
  let offsets := [input.utf8ByteSize] ++ List.range (input.utf8ByteSize + 2) ++ [0, 4294967295]
  let queries := offsets.map fun offset =>
    Lean.Json.mkObj [("offset", Lean.toJson offset), ("location", expectedJson (locate boundaries offset))]
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str id),
    ("source", .str input), ("line_starts", Lean.toJson (starts boundaries)), ("queries", .arr queries.toArray)]

def locationCorpus : String := String.join (texts.map fun (id, input) => (textJson id input).compress ++ "\n")

private inductive Encoding | utf8 | utf8Bom | latin1

private def Encoding.name : Encoding → String
  | .utf8 => "utf8" | .utf8Bom => "utf8-bom" | .latin1 => "latin1"

private def Encoding.header (ending : String) : Encoding → String
  | .utf8 => ""
  | .utf8Bom => bomText
  | .latin1 => "# coding: latin-1" ++ ending

private def Encoding.raw (codec : Encoding) (text : String) : List Nat :=
  match codec with
  | .latin1 => text.toList.map Char.toNat
  | _ => text.toUTF8.toList.map UInt8.toNat

private def Encoding.payload : Encoding → String
  | .latin1 => "éøÿ"
  | _ => "é雪🦀e" ++ acute ++ bomText

private structure Program where
  id : String
  codec : Encoding
  ending : String
  trailingNewline : Bool

private def programs : List Program :=
  ([.utf8, .utf8Bom, .latin1] : List Encoding).flatMap fun codec =>
    [("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r")].flatMap fun (name, ending) =>
      [false, true].map fun trailing =>
        ⟨s!"{codec.name}-{name}-trailing-{trailing}", codec, ending, trailing⟩

private def programParts (program : Program) : String × String × String :=
  let value := "'" ++ program.codec.payload ++ "'"
  let moduleBefore := program.codec.header program.ending ++ "nom = " ++ value ++ "; flag = "
  let functionBefore := moduleBefore ++ "True" ++ program.ending ++ "def vérifier():" ++ program.ending ++
    "    nom = " ++ value ++ "; return "
  let text := functionBefore ++ "False" ++ (if program.trailingNewline then program.ending else "")
  (moduleBefore, functionBefore, text)

private def candidateJson (codec : Encoding) (boundaries : List Boundary)
    (before original replacement : String) (symbol : Option String) : Option Lean.Json := do
  let location ← locate boundaries before.utf8ByteSize
  return Lean.Json.mkObj [("raw_start", Lean.toJson (codec.raw before).length),
    ("decoded_start", Lean.toJson before.utf8ByteSize), ("length", Lean.toJson (codec.raw original).length),
    ("line", Lean.toJson location.line), ("column", Lean.toJson location.column),
    ("original", .str original), ("replacement", .str replacement), ("symbol", Lean.toJson symbol)]

private def programJson (program : Program) : Lean.Json :=
  let (moduleBefore, functionBefore, text) := programParts program
  let boundaries := trace text
  let candidates := (candidateJson program.codec boundaries moduleBefore "True" "False" none).toList ++
    (candidateJson program.codec boundaries functionBefore "False" "True" (some "vérifier")).toList
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str program.id),
    ("raw", Lean.toJson (program.codec.raw text)), ("candidates", .arr candidates.toArray)]

def candidateCorpus : String := String.join (programs.map fun program => (programJson program).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (family, broken) in [
      ("location_count_bytes_as_columns", traceWith endsLine (fun _ scalar => utf8Width scalar)),
      ("location_split_crlf", traceWith (fun scalar _ => scalar == '\r' || scalar == '\n') countsColumn),
      ("location_count_leading_bom", traceWith endsLine (fun _ _ => 1)),
      ("location_hide_interior_bom", traceWith endsLine (fun _ scalar => if scalar == bom then 0 else 1)),
      ("location_forget_column_reset", fun source =>
        (trace source).zipWith (fun boundary running =>
          { boundary with location := { boundary.location with column := running.location.column } })
          (traceWith (fun _ _ => false) countsColumn source))] do
    match texts.find? (fun (_, source) => broken source != trace source) with
    | some (id, _) => IO.println s!"family={family} detected=true case={id}"
    | none =>
      IO.eprintln s!"source location corpus missed broken family {family}"
      return 2
  let interior := texts.findSome? fun (id, source) =>
    let boundaries := trace source
    (List.range source.utf8ByteSize).findSome? fun offset =>
      if (locate boundaries offset).isNone &&
        ((boundaries.filter fun boundary => boundary.offset ≤ offset).getLast?).isSome
      then some s!"{id} offset={offset}" else none
  match interior with
  | none =>
    IO.eprintln "source location corpus missed snapping interior offsets to scalar starts"
    return 2
  | some detail => IO.println s!"family=location_snap_interior_offset detected=true case={detail}"
  match programs.find? (fun program =>
    let (before, _, _) := programParts program
    before.utf8ByteSize != (program.codec.raw before).length) with
  | some program =>
    IO.println s!"family=location_store_decoded_as_raw detected=true case={program.id}"
    return 0
  | none =>
    IO.eprintln "candidate location corpus missed decoded offsets stored as raw"
    return 2

end HoiminOracle.SourceLocation
