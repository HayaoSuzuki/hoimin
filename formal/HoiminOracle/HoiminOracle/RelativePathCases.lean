import HoiminOracle.RelativePathModel
import Lean.Data.Json

namespace HoiminOracle.RelativePath

private structure Fixture where
  id : String
  input : String
  schemaMatches : Bool := true
  hashMatches : Bool := true

private def parts : List String := ["pkg", "cfg.toml", "", ".", "..", "c:d", "a\\b", "bad\x00x", "日本語"]

private def fixtures : List Fixture := Id.run do
  let single : List Fixture := parts.zipIdx.map fun (part, i) => { id := s!"single-{i}", input := part }
  let pairs : List Fixture := parts.zipIdx.flatMap fun (left, i) => parts.zipIdx.map fun (right, j) =>
    { id := s!"pair-{i}-{j}", input := left ++ "/" ++ right }
  let nested : List Fixture := parts.zipIdx.map fun (part, i) =>
    { id := s!"nested-{i}", input := "src/" ++ part ++ "/file.py" }
  let cases := [
    ("drive-native", "C:\\work\\file.py"), ("windows-root", "\\work\\file.py"),
    ("unc", "\\\\server\\share\\file.py"), ("windows-parent", "dir\\..\\file.py"),
    ("windows-dot", ".\\pkg\\.\\file.py"), ("nested-colon", "dir/sub/file.py:stream"),
    ("empty-after-cleaning", "././/."), ("multiple-separators", "./pkg//file.py/"),
    ("absolute", "/pkg/file.py"), ("hidden-and-dots", ".config/..name/..."),
    ("glob-literal", "pkg/*[ab]?.py"), ("extension-and-space", "CON.txt/trailing ."),
    ("unicode-and-control", "é/葉\t\n.py"), ("leading-space", " file.py"),
    ("fullwidth-colon", "pkg/列：名前.py")]
  let metadata : List Fixture := [("valid", "pkg/calc.py"), ("invalid", "pkg/calc.py:stream")].flatMap fun (label, path) =>
    ([false, true].flatMap fun schema => [false, true].map fun hash =>
      { id := s!"candidate-{label}-{schema}-{hash}", input := path,
        schemaMatches := schema, hashMatches := hash })
  return single ++ pairs ++ nested ++ cases.map (fun (id, input) => { id := id, input := input }) ++ metadata

private def candidateLabel : CandidateOutcome → String
  | .accepted => "accepted"
  | .unsupportedSchema => "unsupported_schema"
  | .invalidPath => "invalid_path"
  | .hashMismatch => "hash_mismatch"

private def fixtureJson (item : Fixture) : Lean.Json :=
  let portable := lexical .portable item.input
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
    ("input", .str item.input), ("schema_matches", Lean.toJson item.schemaMatches),
    ("hash_matches", Lean.toJson item.hashMatches), ("portable", Lean.toJson portable),
    ("unix_workspace", Lean.toJson (lexical .unixWorkspace item.input)),
    ("exact_unix", Lean.toJson (exactFile .unix item.input)),
    ("exact_windows", Lean.toJson (exactFile .windows item.input)),
    ("candidate", .str (candidateLabel (candidate portable item.schemaMatches item.hashMatches)))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, differs) in [
      ("path_validate_colon_only_in_first_component", fun item : Fixture =>
        let components := item.input.splitOn "/"
        (allows .unixWorkspace components && !(components.headD "").contains ':') != lexical .portable item.input),
      ("path_normalize_stored_candidate", fun item =>
        (exactFile .unix item.input).isSome != lexical .portable item.input),
      ("path_allow_parent_traversal", fun item =>
        (normalizeParts ((item.input.splitOn "/").filter (· != ".."))).isSome != (exactFile .unix item.input).isSome),
      ("path_treat_native_windows_backslash_as_literal", fun item =>
        exactFile .unix item.input != exactFile .windows item.input),
      ("path_check_candidate_hash_before_path", fun item =>
        let allowed := lexical .portable item.input
        let broken := if item.schemaMatches && !item.hashMatches then .hashMismatch
          else candidate allowed item.schemaMatches item.hashMatches
        candidateLabel broken != candidateLabel (candidate allowed item.schemaMatches item.hashMatches))] do
    match fixtures.find? differs with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"relative path corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.RelativePath
