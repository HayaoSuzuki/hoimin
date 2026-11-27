import HoiminOracle.ReviewSourceModel
import Lean.Data.Json

namespace HoiminOracle.ReviewSource

private structure Fixture where
  path : String
  file : File
  bytes : Option (List UInt8)
  start : Nat := 0
  line : Nat := 1

private def declared (path : String) (file : File) (label : String) (tail : List UInt8) : Fixture :=
  let header := s!"# coding: {label}\n".toUTF8.toList
  ⟨path, file, some (header ++ "1\n".toUTF8.toList ++ tail), header.length, 2⟩

private def fixtures : List Fixture :=
  [ ⟨"zeta.py", .utf8, some "1\n".toUTF8.toList, 0, 1⟩
  , ⟨"alpha.py", .utf8, some "1\n# second\n".toUTF8.toList, 0, 1⟩
  , declared "latin.py" .latin1 "latin-1" [35, 32, 233, 10]
  , ⟨"bad-utf8.py", .invalidUtf8, some [49, 10, 255], 0, 1⟩
  , declared "bad-codec.py" .unsupportedEncoding "Invalid_Codec" []
  , ⟨"missing.py", .missing, none, 0, 1⟩
  , ⟨"../invalid.py", .invalidPath, none, 0, 1⟩
  ]

private def fileAt (path : String) : File :=
  ((fixtures.find? (fun file => file.path == path)).map Fixture.file).getD .missing

private def shapeName : Shape → String
  | .valid => "valid"
  | .mutation => "mutation"
  | .span => "span"
  | .original => "original"
  | .location => "location"
  | .identity => "identity"

private def cases : List (String × List Candidate) :=
  let singles := fixtures.flatMap fun file => [true, false].flatMap fun matchingHash =>
    ([.valid, .mutation, .span, .original, .location, .identity] : List Shape).map fun shape =>
      (s!"single-{file.path}-{matchingHash}-{shapeName shape}", [⟨file.path, matchingHash, shape⟩])
  let zeta := Candidate.mk "zeta.py" true .valid
  let alpha := Candidate.mk "alpha.py" true .valid
  let focus := [
    zeta, { zeta with shape := .identity }, { zeta with hashMatches := false, shape := .original },
    { alpha with shape := .original }, alpha,
    Candidate.mk "bad-utf8.py" true .original, Candidate.mk "bad-utf8.py" true .location,
    Candidate.mk "bad-codec.py" true .valid, Candidate.mk "latin.py" true .valid,
    Candidate.mk "missing.py" true .valid, Candidate.mk "../invalid.py" true .span]
  let interleaved := focus.zipIdx.flatMap fun (a, i) => focus.zipIdx.map fun (b, j) =>
    (s!"interleaved-{i}-{j}", [zeta, a, alpha, b, zeta])
  singles ++ interleaved ++ [
    ("empty", []),
    ("earliest-input-error-among-groups", [zeta, { alpha with shape := .original },
        { zeta with shape := .span }])]

private def errorMessage : Failure → String
  | .path => "candidate path must be root-relative"
  | .mutation => "candidate operator is empty or replacement is unchanged"
  | .span => "candidate span is outside the source"
  | .original => "candidate original text does not match the source span"
  | .utf8 => "candidate source is not valid UTF-8"
  | .encoding => "candidate source encoding 'Invalid_Codec': unsupported or unknown codec; supported codecs are UTF-8, ASCII and Latin-1"
  | .location => "candidate line or column does not match its byte span"
  | .identity => "candidate identity conflict"

private def outcomeJson : Outcome → Lean.Json
  | .fresh => Lean.Json.mkObj [("kind", .str "fresh")]
  | .stale => Lean.Json.mkObj [("kind", .str "stale")]
  | .unavailable => Lean.Json.mkObj [("kind", .str "unavailable")]
  | .invalid reason => Lean.Json.mkObj [("kind", .str "invalid"), ("message", .str (errorMessage reason))]

private def fileJson (file : Fixture) : Lean.Json :=
  Lean.Json.mkObj [
    ("path", .str file.path),
    ("bytes", Lean.toJson (file.bytes.map (List.map UInt8.toNat))),
    ("start", Lean.toJson file.start),
    ("line", Lean.toJson file.line)]

private def candidateJson (candidate : Candidate) : Lean.Json :=
  Lean.Json.mkObj [
    ("path", .str candidate.path),
    ("hash_matches", Lean.toJson candidate.hashMatches),
    ("shape", .str (shapeName candidate.shape))]

private def attempts (orderedPaths : List String) : List String :=
  orderedPaths.filter fun path => (fileAt path).attemptsRead

def corpus : String :=
  String.join (cases.map fun (id, candidates) =>
    let results := outcomes fileAt candidates
    let firstError := results.findIdx? (fun | .invalid _ => true | _ => false)
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("mode", .str "internal-fixture"),
      ("id", .str id),
      ("files", .arr (fixtures.toArray.map fileJson)),
      ("candidates", .arr (candidates.toArray.map candidateJson)),
      ("visits", Lean.toJson (attempts (paths candidates))),
      ("contexts", Lean.toJson ((paths candidates).filter (fun path => (fileAt path).hasContext)).length),
      ("first_error", Lean.toJson firstError),
      ("outcomes", .arr (results.toArray.map outcomeJson))
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (List Candidate → List String × List Outcome)) :=
    [ ("validate_before_hash", fun candidates => (attempts (paths candidates), candidates.map fun candidate =>
        let checked := classify (fileAt candidate.path) { candidate with hashMatches := true }
        match checked with
        | .invalid _ => checked
        | _ => classify (fileAt candidate.path) candidate))
    , ("decode_before_descriptor_checks", fun candidates => (attempts (paths candidates), candidates.map fun candidate =>
        classifyWith (fun file shape => if file = .invalidUtf8 then .invalid .utf8 else validate file shape)
          (fileAt candidate.path) candidate))
    , ("missing_is_stale", fun candidates => (attempts (paths candidates), candidates.map fun candidate =>
        if fileAt candidate.path = .missing then .stale else classify (fileAt candidate.path) candidate))
    , ("ignore_candidate_identity", fun candidates => (attempts (paths candidates), candidates.map fun candidate =>
        classify (fileAt candidate.path)
          { candidate with shape := if candidate.shape = .identity then .valid else candidate.shape }))
    , ("return_outcomes_in_group_order", fun candidates =>
        let ordered := paths candidates
        let grouped := ordered.flatMap fun path => candidates.filter (fun candidate => candidate.path == path)
        (attempts ordered, outcomes fileAt grouped))
    , ("sort_file_visits", fun candidates =>
        (attempts ((paths candidates).mergeSort (fun a b => a ≤ b)), outcomes fileAt candidates))
    , ("reuse_first_candidate_result", fun candidates => (attempts (paths candidates), candidates.map fun candidate =>
        let first := candidates.find? (fun item => item.path == candidate.path)
        classify (fileAt candidate.path) (first.getD candidate)))
    ]
  for (name, variant) in variants do
    if let some (id, _) := cases.find? (fun (_, candidates) =>
        variant candidates != (attempts (paths candidates), outcomes fileAt candidates)) then
      IO.println s!"review source sensitivity: {name}: {id}"
    else
      IO.eprintln s!"review source missed variant: {name}"
      return 1
  IO.println s!"review_source_cases={cases.length}"
  return 0

end HoiminOracle.ReviewSource
