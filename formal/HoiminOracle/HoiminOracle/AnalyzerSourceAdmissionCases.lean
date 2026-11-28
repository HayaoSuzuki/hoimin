import HoiminOracle.AnalyzerSourceAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.AnalyzerSourceAdmission

private def fixtures (eol : String) : List (String × Source) :=
  let bytes (lines : List String) := ((String.intercalate eol lines) ++ eol).toUTF8.toList
  let flag := "flag = True"
  let bad := "flag = ["
  let highByte := [233] ++ eol.toUTF8.toList
  let bom := SourceDeclaration.leadingBom
  [ ("default", .present (bytes [flag]) .candidate)
  , ("empty", .present (bytes ["value = 1"]) .empty)
  , ("syntax", .present (bytes [bad]) .invalidSyntax)
  , ("missing", .missing)
  , ("default-invalid", .present (bytes [flag, "#"] ++ highByte) .candidate)
  , ("declared-invalid", .present (bytes ["# coding: UTF_8", bad, "#"] ++ highByte) .invalidSyntax)
  , ("ascii", .present (bytes ["# coding: US_ASCII", flag]) .candidate)
  , ("ascii-invalid", .present (bytes ["# coding: US_ASCII", flag, "#"] ++ highByte) .candidate)
  , ("latin1", .present (bytes ["# coding: Latin_1", flag, "#"] ++ highByte) .candidate)
  , ("unknown", .present (bytes ["# coding: Invalid_Codec", bad, "#"] ++ highByte) .invalidSyntax)
  , ("bom", .present (bom ++ bytes ["# coding: UTF_8", flag]) .candidate)
  , ("bom-conflict", .present (bom ++ bytes ["# coding: ASCII", flag]) .candidate)
  , ("ignored-cookie", .present (bytes [flag, "# coding: latin-1", "#"] ++ highByte) .candidate)
  ]

private def cases : List (String × List Target) :=
  let make := fun (id : String) (files : List (String × Source)) =>
    (id, files.zipIdx |>.map fun ((label, source), i) => ⟨s!"{i}-{label}.py", source⟩)
  let single := [("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r")].flatMap fun (ending, eol) =>
    (fixtures eol).map fun file => make s!"{ending}-{file.1}" [file]
  let files := fixtures "\n"
  let paired := files.flatMap fun first => files.map fun second =>
    make s!"{first.1}-then-{second.1}" [first, second]
  let tripled := [["default", "syntax", "latin1"], ["syntax", "default-invalid", "missing"],
      ["ascii", "missing", "bom-conflict"], ["empty", "syntax", "syntax"],
      ["default-invalid", "syntax", "default"], ["bom", "bom-conflict", "latin1"]].map fun labels =>
    make (String.intercalate "-then-" labels) (labels.filterMap fun label =>
      files.find? (fun file => file.1 == label))
  single ++ paired ++ tripled

private def sourceJson (target : Target) : Lean.Json :=
  let bytes := match target.source with
    | .missing => Lean.Json.null
    | .present bytes _ => Lean.toJson (bytes.map UInt8.toNat)
  Lean.Json.mkObj [("path", .str target.path), ("bytes", bytes)]

private def fatalJson (path : String) (fatal : Fatal) : Lean.Json :=
  let (code, message, matchPrefix) := match fatal with
    | .read => ("analyzer.source.read", "root-relative file was not found", false)
    | .encoding failure =>
      let (reason, matchPrefix) := match failure.reason with
        | .bomConflict => ("declaration conflicts with UTF-8 BOM", false)
        | .unsupportedCodec => ("unsupported or unknown codec; supported codecs are UTF-8, ASCII and Latin-1", false)
        | .invalidUtf8 => ("bytes are not valid UTF-8: ", true)
        | .nonAscii index => (s!"non-ASCII byte at original offset {index}", false)
      let encoding := failure.label.getD "utf-8 (default)"
      ("analyzer.source.encoding", s!"{path}: source encoding '{encoding}': {reason}", matchPrefix)
  Lean.Json.mkObj [("path", .str path), ("code", .str code), ("message", .str message),
    ("prefix", Lean.toJson matchPrefix)]

private def expectedJson (result : Scan) : Lean.Json :=
  let paths := fun shape =>
    result.accepted.filterMap (fun (path, parsed) => if parsed == shape then some path else none)
  Lean.Json.mkObj [
    ("accepted", Lean.toJson result.accepted.length),
    ("candidates", Lean.toJson (paths .candidate)),
    ("invalid_syntax", Lean.toJson (paths .invalidSyntax)),
    ("fatal", result.failure.elim .null (fun (path, fatal) => fatalJson path fatal))]

def corpus : String :=
  String.join (cases.map fun (id, targets) =>
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("id", .str id),
      ("targets", .arr (targets.toArray.map sourceJson)),
      ("expected", expectedJson (scan targets))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (List Target → Scan)) :=
    [ ("decode failure becomes syntax diagnostic", scanWith fun source =>
        match inspect source with
        | .error (.encoding _) => .ok .invalidSyntax
        | result => result)
    , ("syntax checked before decode", scanWith fun source =>
        match source with
        | .present _ .invalidSyntax => .ok .invalidSyntax
        | _ => inspect source)
    , ("invalid syntax aborts scanning", scanWith fun source =>
        match inspect source with
        | .ok .invalidSyntax => .error .read
        | result => result)
    , ("last failure takes precedence", fun targets =>
        let result := scan targets
        let lastFailure := targets.reverse.findSome? fun target =>
          match inspect target.source with
          | .error failure => some (target.path, failure)
          | .ok _ => none
        { result with failure := lastFailure })
    ]
  for (name, variant) in variants do
    if let some (id, _) := cases.find? (fun (_, targets) =>
        expectedJson (variant targets) != expectedJson (scan targets)) then
      IO.println s!"analyzer source admission sensitivity: {name}: {id}"
    else
      IO.eprintln s!"analyzer source admission missed variant: {name}"
      return 1
  IO.println s!"analyzer_source_admission_cases={cases.length}"
  return 0

end HoiminOracle.AnalyzerSourceAdmission
