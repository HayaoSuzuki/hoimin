import HoiminOracle.SourceCoordinatesModel
import Lean.Data.Json

namespace HoiminOracle.SourceCoordinates

private inductive Codec | latin1 | utf8 | ascii
  deriving BEq

private structure Fixture where
  id : String
  codec : Codec
  bytes : List UInt8
  text : String
  widths : List Width

private def scalarWidth (character : Char) : Nat :=
  let code := character.toNat
  if code < 0x80 then 1 else if code < 0x800 then 2 else if code < 0x10000 then 3 else 4

private def latin1 (id : String) (payload : List UInt8) : Fixture :=
  let bytes := "# coding: latin-1\n".toUTF8.toList ++ payload
  ⟨id, .latin1, bytes, String.ofList (bytes.map (Char.ofNat ∘ UInt8.toNat)), latin1Widths bytes⟩

private def identityWidths (id : String) (codec : Codec) (text : String) : Fixture :=
  ⟨id, codec, text.toUTF8.toList, text, text.toList.map fun character =>
    let width := scalarWidth character
    (width, width)⟩

private def combinations (values : List α) : Nat → List (List α)
  | 0 => [[]]
  | more + 1 => values.flatMap fun value => (combinations values more).map (value :: ·)

private def bomPrefix := String.singleton (Char.ofNat 0xfeff)

private def latinCases : List Fixture :=
  let payloads := (List.range 4).flatMap (combinations ([0, 127, 128, 255] : List UInt8))
  payloads.zipIdx.map (fun (payload, index) => latin1 s!"latin1-{index}" payload) ++ [
    latin1 "latin1-every-byte" ((List.range 256).map UInt8.ofNat),
    latin1 "latin1-alternating" [255, 97, 128, 10, 233, 13, 90],
    latin1 "latin1-expansion-at-eof" [97, 255, 255, 255]]

private def utf8Cases : List Fixture :=
  let payloads := (List.range 3).flatMap (combinations ['a', 'é', '雪', '𐐀', '\r'])
  [false, true].flatMap fun bom => payloads.zipIdx.map fun (payload, index) =>
    identityWidths s!"utf8-{bom}-{index}" .utf8
      ((if bom then bomPrefix else "") ++ "# coding: utf-8\n" ++ String.ofList payload)

private def fixtures : List Fixture := latinCases ++ utf8Cases ++ [
  identityWidths "utf8-implicit" .utf8 "é\n雪𐐀",
  identityWidths "utf8-bom-implicit" .utf8 (bomPrefix ++ "é\r\n雪"),
  identityWidths "ascii-empty-payload" .ascii "# coding: ascii\n",
  identityWidths "ascii-controls" .ascii "# coding: ascii\r\n\x00A\t\x7f"]

private def codecLabel : Codec → String
  | .latin1 => "latin1"
  | .utf8 => "utf8"
  | .ascii => "ascii"

private def forward (item : Fixture) := (List.range (inputLength item.widths + 2)).map (translate item.widths)

private def backward (item : Fixture) :=
  (List.range (outputLength item.widths + 2)).map (translate (reverseWidths item.widths))

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("source", Lean.toJson (item.bytes.map UInt8.toNat)), ("codec", .str (codecLabel item.codec)),
  ("text", .str item.text), ("raw_to_utf8", Lean.toJson (forward item)), ("utf8_to_raw", Lean.toJson (backward item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, differs) in [
      ("coordinates_keep_decoded_offset_as_raw", fun item : Fixture =>
        (List.range (outputLength item.widths + 2)).map (fun offset =>
          (translate (reverseWidths item.widths) offset).map (fun _ => offset)) != backward item),
      ("coordinates_accept_interior_of_utf8_scalar", fun item =>
        item.codec == .utf8 && (List.range (inputLength item.widths + 2)).map (fun offset =>
          if offset ≤ inputLength item.widths then some offset else none) != forward item),
      ("coordinates_count_expansion_at_its_start", fun item =>
        item.codec == .latin1 && (List.range (inputLength item.widths + 2)).map (fun offset =>
          (translate item.widths offset).map fun result =>
            if item.widths[offset]?.any (fun width => width.2 == 2) then result + 1 else result) != forward item),
      ("coordinates_reject_end_boundary", fun item =>
        (List.range (inputLength item.widths + 2)).map (fun offset =>
          if offset == inputLength item.widths then none else translate item.widths offset) != forward item),
      ("coordinates_drop_bom_from_layout", fun item =>
        item.text.startsWith bomPrefix && (List.range (inputLength item.widths + 2)).map (translate (item.widths.drop 1)) != forward item)] do
    match fixtures.find? differs with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"source coordinate corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.SourceCoordinates
