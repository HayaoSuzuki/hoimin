import HoiminOracle.SourceEscapeModel
import Lean.Data.Json

namespace HoiminOracle.SourceEscape

private structure Fixture where
  id : String
  tail : String
  bytes : Bool
  lead : String := "\\"
  knownName : Option (String × Char) := none
  signedHex : Bool := false

private def bothModes (id : String) (tail : String) : List Fixture :=
  [⟨id ++ "-text", tail, false, "雪\\", none, false⟩, ⟨id ++ "-bytes", tail, true, "\\", none, false⟩]

private def ordinary : List Fixture :=
  let namedControls := standardEscapes.map fun (marker, _) =>
    (s!"standard-{marker.toNat}", String.singleton marker ++ "Z")
  let other := [
    ("at-end", ""), ("lf", "\nZ"), ("cr", "\rZ"), ("crlf", "\r\nZ"), ("lfcr", "\n\rZ"),
    ("unknown-ascii", "qZ"), ("unknown-two-byte", "éZ"), ("unknown-three-byte", "雪Z"),
    ("unknown-four-byte", "🦀Z"), ("non-ascii-digit", "９Z")]
  (other ++ namedControls).flatMap fun (id, tail) => bothModes id tail

private def octal : List Fixture :=
  ([("zero", "0"), ("two", "07Z"), ("max-byte", "377Z"), ("wrap-start", "400Z"),
    ("max-octal", "777Z"), ("ignores-fourth", "1777Z"), ("stops-at-eight", "078Z"),
    ("leading-eight", "877Z"), ("after-zero", "0９7")] : List (String × String)).flatMap fun (id, tail) =>
      bothModes ("octal-" ++ id) tail

private def fixed : List Fixture :=
  ([
    ("hex-min", "x00f"), ("hex-mixed-case", "xaF9"), ("hex-max", "xFFZ"),
    ("hex-empty", "x"), ("hex-short", "x1"), ("hex-invalid", "xG0Z"),
    ("hex-prefix", "x0xAB"), ("hex-minus", "x-A"), ("hex-whitespace", "x A"),
    ("hex-utf8-crosses-width", "x1é"), ("hex-utf8-exact-width", "xé"),
    ("short-unicode-ascii", "u0041Z"), ("short-unicode-nonascii", "u96EaZ"),
    ("short-unicode-short", "u123"), ("short-unicode-space", "u 123"), ("short-unicode-invalid", "u12G4"),
    ("before-surrogates", "uD7FFZ"), ("first-surrogate", "uD800Z"),
    ("last-surrogate", "uDFFFZ"), ("after-surrogates", "uE000Z"),
    ("long-unicode-ascii", "U00000041Z"), ("long-unicode-short", "U0000000"),
    ("long-unicode-invalid", "U000000Ga"), ("long-surrogate", "U0000D800Z"),
    ("max-scalar", "U0010FFFFZ"), ("above-max-scalar", "U00110000Z"),
    ("max-u32", "UFFFFFFFFZ")] : List (String × String)).flatMap fun (id, tail) => bothModes id tail

private def name127 := String.ofList (List.replicate 125 'a' ++ ['é'])
private def name128 := String.ofList (List.replicate 126 'a' ++ ['é'])

private def named : List Fixture :=
  [⟨"known-name", "N{LATIN SMALL LETTER A}Z", false, "🦀\\", some ("LATIN SMALL LETTER A", 'a'), false⟩,
   { id := "unknown-name", tail := "N{MISSING}Z", bytes := false },
   { id := "empty-name", tail := "N{}Z", bytes := false },
   { id := "missing-open", tail := "Nname}", bytes := false },
   ⟨"missing-close", "N{name", false, "\\", some ("name", 'n'), false⟩,
   ⟨"first-close", "N{A}B}Z", false, "\\", some ("A", 'a'), false⟩,
   ⟨"name-127-bytes", "N{" ++ name127 ++ "}Z", false, "\\", some (name127, 'n'), false⟩,
   ⟨"name-128-bytes", "N{" ++ name128 ++ "}Z", false, "\\", some (name128, 'n'), false⟩,
   ⟨"bytes-skip-lookup", "N{LATIN SMALL LETTER A}Z", true, "\\", some ("LATIN SMALL LETTER A", 'a'), false⟩]

private def knownDifferences : List Fixture :=
  [⟨"plus-x-text", "x+A", false, "\\", none, true⟩,
   ⟨"plus-x-bytes", "x+A", true, "\\", none, true⟩,
   ⟨"plus-u-text", "u+041", false, "\\", none, true⟩,
   ⟨"plus-U-text", "U+0000041", false, "\\", none, true⟩]

private def cases := ordinary ++ octal ++ fixed ++ named ++ knownDifferences

private def signedNumber : List Char → Option Nat
  | '+' :: digits => number 16 digits
  | digits => number 16 digits

private def run (item : Fixture) (rules : Rules := {}) : Observation :=
  decodeWith rules item.bytes (fun name =>
    item.knownName.bind fun (known, value) => if known == name then some value else none) item.tail.toList

private def charJson (value : Char) : Lean.Json := .str (String.singleton value)

private def observationJson (initial : Nat) (observed : Observation) : Lean.Json :=
  let (kind, value) := match observed.result with
    | .rejected => ("rejected", .null)
    | .character value => ("character", charJson value)
    | .preserved value => ("preserved", charJson value)
    | .continuation => ("continuation", .null)
  Lean.Json.mkObj [("kind", .str kind), ("value", value),
    ("next", Lean.toJson (initial + observed.consumed)), ("lookup", Lean.toJson observed.lookup)]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let initial := item.lead.utf8ByteSize
  let render := observationJson initial
  let provider := match item.knownName with
    | some (name, value) => Lean.Json.mkObj [("name", .str name), ("value", charJson value)]
    | none => .null
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("source", .str (item.lead ++ item.tail)), ("start", Lean.toJson initial),
    ("bytes", Lean.toJson item.bytes), ("provider", provider), ("expected", render (run item)),
    ("known_actual", if item.signedHex then render (run item { hexadecimal := signedNumber }) else .null)]

def corpus : String := String.join (cases.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (family, changed) in [
      ("escape_accept_signed_hex", fun item => run item { hexadecimal := signedNumber }),
      ("escape_consume_four_octal_digits", fun item => run item { octalWidth := 4 }),
      ("escape_omit_octet_wrapping", fun item => run item { wrapOctalBytes := false }),
      ("escape_split_crlf", fun item => run item { joinCrlf := false }),
      ("escape_include_128_byte_name", fun item => run item { scanNameBytes := 129 }),
      ("escape_reject_127_byte_name", fun item => run item { scanNameBytes := 127 }),
      ("escape_rewind_after_invalid_scalar", fun item =>
        let observation := run item
        if observation.result == .rejected && 1 < observation.consumed then
          { observation with consumed := 1 }
        else observation),
      ("escape_count_preserved_as_one_byte", fun item =>
        let observation := run item
        match observation.result with
        | .preserved _ => { observation with consumed := 1 }
        | _ => observation)] do
    match cases.find? (fun item => changed item != run item) with
    | some item => IO.println s!"family={family} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"source escape corpus missed broken family {family}"
      return 2
  return 0

end HoiminOracle.SourceEscape
