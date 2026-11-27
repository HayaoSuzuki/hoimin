import HoiminOracle.SessionUriModel
import Lean.Data.Json

namespace HoiminOracle.SessionUri

private structure Case where
  id : String
  bytes : List UInt8
  unixOnly : Bool := false
  sqliteFile : Bool := false

private def text (id spelling : String) (sqliteFile := false) (unixOnly := false) : Case :=
  ⟨id, spelling.toUTF8.toList, unixOnly, sqliteFile⟩

private def cases : List Case :=
  [ text "empty-codec" ""
  , text "unix-absolute" "/tmp/session #%.db"
  , text "unix-leading-double-slash" "//project/session.db"
  , text "windows-drive" "C:\\project\\session #%.db"
  , text "windows-drive-relative" "C:session.db"
  , text "windows-unc" "\\\\server\\share\\session.db"
  , text "windows-verbatim-drive" "\\\\?\\C:\\project\\session.db"
  , text "windows-verbatim-unc" "\\\\?\\UNC\\server\\share\\session.db"
  , text "ordinary-file" "ordinary.sqlite3" true
  , text "space-plus-hash" "with space+#sqlite.db" true
  , text "literal-escapes" "native%2F%3F%23%00.sqlite3" true
  , text "percent-in-neighbor-name" "session%25.sqlite3" true
  , text "utf8-file" "セッション-é-🦉.sqlite3" true
  , text "decomposed-utf8-file" ("e" ++ String.singleton (Char.ofNat 769) ++ ".sqlite3") true
  , text "nested-file" "東京 +%/session[1].sqlite3" true
  , text "literal-query-and-fragment" "session?name=neighbor#keep.sqlite3" true true
  , text "literal-unix-backslash" "session\\notes.sqlite3" true true
  , text "literal-unix-colon" "notes:session.sqlite3" true true
  , ⟨"non-utf8-file", "byte-".toUTF8.toList ++ [255] ++ ".sqlite3".toUTF8.toList, true, true⟩
  , ⟨"non-utf8-sequences", [192, 175, 237, 160, 128, 244, 144, 128, 128], true, false⟩
  , ⟨"ascii-byte-range", (List.range 128).map UInt8.ofNat, false, false⟩
  , ⟨"complete-byte-range", (List.range 256).map UInt8.ofNat, true, false⟩ ]

def corpus : String :=
  String.join (cases.map fun item =>
    let encoded := immutableUri item.bytes
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("unix_only", .bool item.unixOnly), ("sqlite_file", .bool item.sqliteFile)
      , ("bytes", Lean.toJson (item.bytes.map UInt8.toNat)), ("uri", .str encoded) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (List UInt8 → List Nat)) :=
    [ ("leave_query_delimiter_raw", fun bytes => scheme ++ bytes.flatMap (fun byte =>
          if byte.toNat = question then [question] else escapeByte byte) ++ options)
    , ("leave_fragment_delimiter_raw", fun bytes => scheme ++ bytes.flatMap (fun byte =>
          if byte.toNat = 35 then [35] else escapeByte byte) ++ options)
    , ("leave_percent_raw", fun bytes => scheme ++ bytes.flatMap (fun byte =>
          if byte.toNat = percent then [percent] else escapeByte byte) ++ options)
    , ("leave_authority_separator_raw", fun bytes => scheme ++ bytes.flatMap (fun byte =>
          if byte.toNat = 47 then [47] else escapeByte byte) ++ options)
    , ("reverse_nibbles", fun bytes => scheme ++ bytes.flatMap (fun byte =>
          [percent, hexDigit (byte.toNat % 16), hexDigit (byte.toNat / 16)]) ++ options)
    , ("replace_non_ascii_bytes", fun bytes => rawUri (bytes.map fun byte =>
          if byte.toNat ≥ 128 then 63 else byte))
    , ("omit_immutable_option", fun bytes => scheme ++ escapePath bytes) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => parseImmutable (variant item.bytes) != some item.bytes) then
      IO.println s!"session URI sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session URI missed variant: {name}"
      return 1
  IO.println s!"session_uri_cases={cases.length} sqlite_files={(cases.filter Case.sqliteFile).length}"
  return 0

end HoiminOracle.SessionUri
