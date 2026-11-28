import HoiminOracle.SourceDeclarationModel
import Lean.Data.Json

namespace HoiminOracle.SourceDeclaration

private structure Fixture where
  id : String
  bytes : List UInt8

private def bytes (text : String) : List UInt8 := text.toUTF8.toList

private def endings : List (String × String) := [("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r")]

private def payloads : List (String × List UInt8) := [
  ("ascii", bytes "# plain\n"), ("latin", bytes "# " ++ [233, 10]), ("utf8", bytes "# é\n")]

private def layouts : List (String × (String → String → String)) := [
  ("first", fun a b => " # coding: latin-1" ++ a ++ "# explanation" ++ b),
  ("after-blank", fun a b => "\t\x0c " ++ a ++ "# coding: latin-1" ++ b),
  ("after-comment", fun a b => "# explanation" ++ a ++ "# coding: latin-1" ++ b),
  ("after-shebang", fun a b => "#!/usr/bin/python" ++ a ++ "# coding=latin-1" ++ b),
  ("after-code", fun a b => "value = 1" ++ a ++ "# coding: latin-1" ++ b),
  ("inline", fun a b => "value = 1 # coding: latin-1" ++ a ++ "# explanation" ++ b),
  ("third", fun a b => "# first" ++ a ++ "# second" ++ b ++ "# coding: latin-1\n"),
  ("competing", fun a b => "# coding: ASCII" ++ a ++ "# coding: latin-1" ++ b)]

private def headerCases : List Fixture := layouts.flatMap fun (placement, header) =>
  endings.flatMap fun (first, a) => endings.flatMap fun (second, b) =>
    payloads.map fun (payloadId, data) =>
      ⟨s!"{placement}-{first}-{second}-{payloadId}", bytes (header a b) ++ data⟩

private def aliases : List String := ["utf-8", "UTF_8", "utf8", "utf-8-sig", "ascii", "US_ASCII", "646",
  "latin-1", "latin1", "ISO_8859_1", "iso8859-1", "iso-latin-1", "LATIN", "l1", "CP819", "ibm819",
  "latin-1-extra", "iso-8859-1-extra", "iso-latin-1-extra", "utf.8", "cp1252", "UTF-80"]

private def aliasCases : List Fixture := aliases.flatMap fun alias =>
  [false, true].flatMap fun bom => payloads.map fun (payloadId, data) =>
    ⟨s!"alias-{alias}-bom-{bom}-{payloadId}",
      (if bom then leadingBom else []) ++ bytes ("# coding: " ++ alias ++ "\n") ++ data⟩

private def fixtures : List Fixture := headerCases ++ aliasCases ++ [
  ⟨"declaration-without-line-ending", bytes "# coding: ISO_8859_1"⟩,
  ⟨"empty-label-then-valid-line", bytes "# coding: \n# coding: LATIN_1\n# " ++ [233]⟩,
  ⟨"empty-label-then-valid-marker", bytes "# coding: ; coding=latin1\n# " ++ [233]⟩,
  ⟨"space-between-marker-and-colon", bytes "# coding : latin-1\n# " ++ [233]⟩,
  ⟨"form-feed-after-marker", bytes "# coding:\x0clatin-1\n# " ++ [233]⟩,
  ⟨"uppercase-marker", bytes "# Coding: latin-1\n# " ++ [233]⟩,
  ⟨"punctuation-terminates-label", bytes "# explanation coding:\tlatin-1; remainder\r\n# " ++ [233]⟩,
  ⟨"partial-bom", [239, 187]⟩,
  ⟨"bom-with-undeclared-invalid-payload", leadingBom ++ [255]⟩,
  ⟨"non-ascii-terminates-label-and-fails-ascii", bytes "# coding: ASCII" ++ [233, 10, 255]⟩,
  ⟨"first-of-two-markers-is-unsupported", bytes "# coding: missing-codec, coding=latin-1\n"⟩,
  ⟨"empty-headers-default-to-utf8", bytes " \t\n\x0c\r\n# " ++ [233]⟩]

private def codecLabel : Codec → String
  | .utf8 => "utf8" | .ascii => "ascii" | .latin1 => "latin1"

private def resultJson : Except Failure (Codec × String) → Lean.Json
  | .ok (codec, text) => Lean.Json.mkObj [("kind", .str "decoded"), ("codec", .str (codecLabel codec)), ("text", .str text)]
  | .error failure =>
    let (reason, offset) := match failure.reason with
      | .bomConflict => ("bom_conflict", none)
      | .unsupportedCodec => ("unsupported_codec", none)
      | .invalidUtf8 => ("invalid_utf8", none)
      | .nonAscii offset => ("non_ascii", some offset)
    Lean.Json.mkObj [("kind", .str "error"), ("label", .str (failure.label.getD "utf-8 (default)")),
      ("implicit", Lean.toJson failure.label.isNone), ("reason", .str reason), ("offset", Lean.toJson offset)]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("source", Lean.toJson (item.bytes.map UInt8.toNat)), ("expected", resultJson (decode item.bytes))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def lookup := firstLabel ∘ headersWith breakLine 2

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("declaration_look_at_third_line", decodeWith (firstLabel ∘ headersWith breakLine 3) selectCodec),
      ("declaration_continue_after_code", decodeWith (fun input => firstLabel ((headersWith breakLine 2 input).map fun
        | .code => .blank
        | header => header)) selectCodec),
      ("declaration_split_crlf_twice", decodeWith (firstLabel ∘ headersWith (fun input =>
        let header := (breakLine input).1
        (header, input.drop (header.length + 1))) 2) selectCodec),
      ("declaration_accept_generic_utf8_alias_with_bom", decodeWith lookup (fun bom label =>
        selectCodec (bom && normalize (label.getD "") != "utf8") label)),
      ("declaration_report_unknown_codec_before_bom", decodeWith lookup (fun bom label =>
        if (supportedCodec (normalize (label.getD "utf-8"))).isNone then .error ⟨label, .unsupportedCodec⟩
        else selectCodec bom label)),
      ("declaration_lose_original_error_label", fun input => (decode input).mapError fun failure =>
        { failure with label := failure.label.map normalize }),
      ("declaration_prefer_second_label", decodeWith (fun input =>
        let headers := headersWith breakLine 2 input
        match headers with
        | .comment (some _) :: .comment (some label) :: _ => some label
        | _ => firstLabel headers) selectCodec)] do
    match fixtures.find? (fun item => (resultJson (broken item.bytes)).compress != (resultJson (decode item.bytes)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"source declaration corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.SourceDeclaration
