import HoiminOracle.GitQuoteModel
import Lean.Data.Json

namespace HoiminOracle.GitQuote

private structure Fixture where
  id : String
  chunks : List Chunk
  form : QuoteForm

private def Chunk.wire : Chunk → String
  | .literal text => text
  | .named escape => String.ofList ['\\', escape.code]
  | .octal value =>
    let (first, second, third) := octalDigits value
    String.ofList ['\\', Char.ofNat (48 + first), Char.ofNat (48 + second), Char.ofNat (48 + third)]

private def Fixture.wire (fixture : Fixture) : String :=
  let body := String.join (fixture.chunks.map Chunk.wire)
  match fixture.form with
  | .complete => "\"" ++ body ++ "\""
  | .missingOpeningQuote => body ++ "\""
  | .missingClosingQuote => "\"" ++ body
  | .danglingBackslash => "\"" ++ body ++ "\\\""
  | .unknownEscape code => "\"" ++ body ++ String.ofList ['\\', code, '"']
  | .malformedOctal digits => "\"" ++ body ++ "\\" ++ digits ++ "\""

private def namedEscapes : List NamedEscape :=
  [.backslash, .quote, .bell, .backspace, .tab, .newline, .verticalTab, .formFeed, .carriageReturn]

private def escapedBytes (bytes : List UInt8) : List Chunk :=
  bytes.map fun byte => .octal ⟨byte.toNat, Nat.lt_trans byte.toNat_lt (by decide)⟩

private def fixtures : List Fixture :=
  (List.finRange 512).map (fun value => ⟨s!"octal-{value.val}",
    [.literal "a/prefix-", .octal value, .literal "3.py"], .complete⟩) ++
  namedEscapes.map (fun escape => ⟨s!"named-{escape.byte.toNat}",
    [.literal "a/before", .named escape, .literal "after.py"], .complete⟩) ++ [
  ⟨"empty", [], .complete⟩,
  ⟨"literal-unicode", [.literal "a/café-雪-𐐀.py"], .complete⟩,
  ⟨"consecutive-named", namedEscapes.map Chunk.named, .complete⟩,
  ⟨"named-mixed-with-octal", [.named .quote, .octal 65, .named .backslash, .octal 66], .complete⟩,
  ⟨"utf8-two", escapedBytes [0xc3, 0xa9], .complete⟩,
  ⟨"utf8-three", escapedBytes [0xe9, 0x9b, 0xaa], .complete⟩,
  ⟨"utf8-four", escapedBytes [0xf0, 0x90, 0x90, 0x80], .complete⟩,
  ⟨"utf8-max-scalar", escapedBytes [0xf4, 0x8f, 0xbf, 0xbf], .complete⟩,
  ⟨"utf8-overlong", escapedBytes [0xc0, 0xaf], .complete⟩,
  ⟨"utf8-surrogate", escapedBytes [0xed, 0xa0, 0x80], .complete⟩,
  ⟨"utf8-above-max", escapedBytes [0xf4, 0x90, 0x80, 0x80], .complete⟩,
  ⟨"utf8-interrupted", escapedBytes [0xe9, 0x9b] ++ [.named .newline, .literal "雪"], .complete⟩,
  ⟨"utf8-incomplete", escapedBytes [0xf0, 0x90, 0x90], .complete⟩,
  ⟨"utf8-error-after-valid", escapedBytes [0xc3, 0xa9, 0x80], .complete⟩,
  ⟨"missing-opening", [.literal "a/file.py"], .missingOpeningQuote⟩,
  ⟨"missing-closing", [.literal "a/file.py"], .missingClosingQuote⟩,
  ⟨"framing-before-overflow", [.octal 256], .missingClosingQuote⟩,
  ⟨"trailing-backslash", [.literal "a/file"], .danglingBackslash⟩,
  ⟨"unknown-named", [.literal "a/file"], .unknownEscape 'x'⟩,
  ⟨"non-octal-lead", [], .unknownEscape '8'⟩,
  ⟨"octal-one-digit", [], .malformedOctal "1"⟩,
  ⟨"octal-two-digits", [], .malformedOctal "12"⟩,
  ⟨"octal-invalid-second-digit", [], .malformedOctal "193"⟩,
  ⟨"octal-invalid-third-digit", [], .malformedOctal "12x"⟩,
  ⟨"utf8-validation-after-escapes", escapedBytes [0xff], .unknownEscape 'x'⟩,
  ⟨"octal-overflow-before-bad-escape", [.octal 256], .unknownEscape 'x'⟩]

private def diagnosticText : Diagnostic → String
  | .framing => "invalid quoted Git path"
  | .escape => "invalid Git path escape"
  | .octal => "invalid octal Git path escape"
  | .utf8 => "Git diff path is not valid UTF-8"

private def expected (item : Fixture) : Except Diagnostic String := decode item.chunks item.form

private def observationJson : Except Diagnostic String → Lean.Json
  | .ok text => Lean.Json.mkObj [("kind", .str "decoded"), ("value", .str text)]
  | .error error => Lean.Json.mkObj [("kind", .str "error"), ("message", .str (diagnosticText error))]

private def fixtureJson (item : Fixture) : Lean.Json :=
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("input", .str item.wire), ("expected", observationJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def validateBytesSeparately (bytes : List UInt8) : Except Diagnostic String :=
  (bytes.mapM fun byte => validateUtf8 [byte]).map String.join

def sensitivity : IO UInt32 := do
  let normalBytes := Chunk.bytes NamedEscape.byte octalByte
  for (name, broken) in [
      ("quote_named_escape_keeps_letter", fun item : Fixture =>
        decodeWith (Chunk.bytes (fun escape => UInt8.ofNat escape.code.toNat) octalByte)
          validateUtf8 item.chunks item.form),
      ("quote_octal_wraps_overflow", fun item =>
        decodeWith (Chunk.bytes NamedEscape.byte (fun value => .ok (UInt8.ofNat value.val)))
          validateUtf8 item.chunks item.form),
      ("quote_validates_every_byte_as_utf8", fun item =>
        decodeWith normalBytes validateBytesSeparately item.chunks item.form),
      ("quote_decodes_before_framing", fun item =>
        (assemble normalBytes item.form item.chunks).bind validateUtf8),
      ("quote_does_not_require_closing_quote", fun item =>
        decode item.chunks (match item.form with | .missingClosingQuote => .complete | rest => rest))] do
    match fixtures.find? (fun item =>
      (observationJson (broken item)).compress != (observationJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git quote corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitQuote
