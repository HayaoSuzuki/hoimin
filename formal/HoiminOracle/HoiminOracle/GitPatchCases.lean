import HoiminOracle.GitPatchModel
import Lean.Data.Json

namespace HoiminOracle.GitPatch

private structure Fixture where
  id : String
  scope : Scope
  sections : List (List Line)
  prelude : List Line := []

private def a : String := "pkg/a.py"
private def b : String := "pkg/b.py"
private def plainA : Header := .file a
private def plainB : Header := .file b
private def textFile : Header := .file "pkg/file.txt"

private def hunk (oldStart newStart : Nat) (count : Option Nat := none) : Line :=
  .hunk (.coordinates oldStart (.decimal newStart) (count.map .decimal))

private def frame (old new : Header) (body : List Line) : List Line :=
  .old old :: .new new :: body

private def matrix : List Fixture :=
  let headers := [("a", plainA), ("b", plainB), ("null", .absent), ("text", textFile)]
  let scopes := [("all", none), ("empty", some []), ("a", some [a]),
    ("b", some [b]), ("both", some [a,b])]
  scopes.flatMap fun (label, scope) => headers.flatMap fun (source, old) =>
    headers.map fun (target, new) =>
      ⟨s!"scope-{label}-{source}-to-{target}", scope,
        [frame old new [hunk 7 2, .payload, .new plainA, .old plainB, hunk 14 6 (some 2)]], []⟩

private def fixtures : List Fixture := matrix ++ [
  ⟨"empty", none, [], []⟩,
  ⟨"preamble-lookalikes", none, [frame plainB plainB [hunk 1 2]],
    [.old .badQuote, .new plainA, .hunk .missingDestination]⟩,
  ⟨"one-section-cannot-complete-previous-header", none,
    [[.old plainA], [.new plainB, hunk 1 2], frame plainA plainA [hunk 7 3]], []⟩,
  ⟨"reset-destination-across-empty-section", none,
    [frame plainA plainA [hunk 1 2], [], [hunk 7 3], frame plainB plainB [hunk 8 4]], []⟩,
  ⟨"out-of-order-new-then-pair", none,
    [[.new .badQuote, .old plainA, .new plainB, hunk 8 4]], []⟩,
  ⟨"hunks-before-new-header", none,
    [[.old plainA, .hunk .missingDestination, .new plainB, hunk 1 3]], []⟩,
  ⟨"old-header-without-new-still-decodes", none, [[.old .badQuote]], []⟩,
  ⟨"empty-scope-validates-old-before-new", some [], [frame .badEscape .badOctal [hunk 1 2]], []⟩,
  ⟨"empty-scope-validates-new-before-hunk", some [],
    [frame plainA .badOctal [.hunk .missingDestination]], []⟩,
  ⟨"empty-scope-does-not-hide-invalid-hunks", some [],
    [frame plainA plainB [.hunk .missingDestination]], []⟩,
  ⟨"non-python-does-not-hide-invalid-hunks", none,
    [frame textFile textFile [.hunk (.coordinates 1 .invalid none)]], []⟩,
  ⟨"deleted-destination-does-not-hide-invalid-hunks", none,
    [frame plainA .absent [.hunk (.coordinates 1 (.decimal 1) (some .invalid))]], []⟩,
  ⟨"payload-headers-are-never-decoded", none,
    [frame plainA plainB [hunk 2 7, .old .badQuote, .new .badEscape, .payload, hunk 3 8]], []⟩,
  ⟨"quoted-utf8-destination", some ["pkg/café.py"], [frame plainA .quotedCafe [hunk 3 5]], []⟩,
  ⟨"quoted-path-decoding-precedes-scope", some [], [frame plainA .badUtf8 [hunk 3 5]], []⟩,
  ⟨"unscoped-literal-backslash", none, [frame plainA .backslash [hunk 3 5]], []⟩,
  ⟨"scoped-literal-backslash-is-not-alias", some [a,b], [frame plainA .backslash [hunk 3 5]], []⟩,
  ⟨"timestamp-and-ascii-case", some ["pkg/space name.PY"],
    [frame plainA (.timestamped "pkg/space name.PY") [hunk 3 5]], []⟩,
  ⟨"dotfile-has-no-extension", none, [frame plainA (.file "pkg/.py") [hunk 3 5]], []⟩,
  ⟨"zero-new-count", none, [frame plainA plainA [hunk 3 0 (some 0), hunk 4 5 (some 0), hunk 5 6]], []⟩,
  ⟨"inclusive-end-at-u32-max", none,
    [frame plainA plainB [hunk 1 (maximumLine - 1) (some 2), hunk 2 maximumLine]], []⟩,
  ⟨"range-arithmetic-overflow", none,
    [frame plainA plainA [hunk 1 maximumLine (some 2)]], []⟩,
  ⟨"new-start-too-wide", none, [frame plainA plainA [hunk 1 (maximumLine + 1) (some 0)]], []⟩,
  ⟨"count-too-wide", none, [frame plainA plainA [hunk 1 1 (some (maximumLine + 1))]], []⟩,
  ⟨"invalid-start-precedes-invalid-count", none,
    [frame plainA plainA [.hunk (.coordinates 1 .invalid (some .invalid))]], []⟩,
  ⟨"range-order-with-repeated-destination", none,
    [frame plainA plainB [hunk 1 9, hunk 2 6], frame plainB plainB [hunk 3 2, hunk 4 2]], []⟩,
  ⟨"excluded-path-and-range-are-distinct-observations", none,
    [frame plainA plainA [hunk 1 2], frame plainA .absent [hunk 1 2 (some 0)]], []⟩,
  ⟨"first-section-failure-precedes-later-failure", none,
    [frame plainA plainB [hunk 1 maximumLine (some 2)], frame .badQuote .badEscape []], []⟩]

private def renderHeader (side : String) : Header → String
  | .file path => side ++ "/" ++ path
  | .timestamped path => side ++ "/" ++ path ++ "\t2026-08-15"
  | .quotedCafe => "\"" ++ side ++ "/pkg/caf\\303\\251.py\""
  | .absent => "/dev/null"
  | .backslash => "\"" ++ side ++ "/pkg\\\\b.py\""
  | .badQuote => "\"unterminated"
  | .badEscape => "\"pkg/invalid\\q.py\""
  | .badOctal => "\"pkg/invalid\\400.py\""
  | .badUtf8 => "\"pkg/invalid\\377.py\""

private def renderNumber : Number → String
  | .decimal value => toString value
  | .invalid => "invalid"

private def renderLine : Line → String
  | .old header => "--- " ++ renderHeader "a" header ++ "\n"
  | .new header => "+++ " ++ renderHeader "b" header ++ "\n"
  | .hunk (.coordinates old start count) =>
    let new := renderNumber start ++ (count.map (fun n => "," ++ renderNumber n)).getD ""
    s!"@@ -{old},1 +{new} @@\n"
  | .hunk .missingDestination => "@@ -1,1 @@\n"
  | .payload => "+@@ -1 +99 @@\n diff --git a/decoy.py b/decoy.py\n"

private def wire (fixture : Fixture) : String :=
  String.join (fixture.prelude.map renderLine) ++
    String.join (fixture.sections.map fun lines =>
      "diff --git a/from b/to\n" ++ String.join (lines.map renderLine))

private def diagnosticText : Diagnostic → String
  | .quotedPath => "invalid quoted Git path"
  | .pathEscape => "invalid Git path escape"
  | .octalEscape => "invalid octal Git path escape"
  | .pathUtf8 => "Git diff path is not valid UTF-8"
  | .backslash => "path contains an unsupported literal backslash: pkg\\b.py"
  | .hunkHeader => "invalid Git hunk header"
  | .hunkStart => "invalid Git hunk start"
  | .hunkCount => "invalid Git hunk count"
  | .hunkOverflow => "Git hunk line range overflow"

private def resultJson : Except Diagnostic (List Effect) → Lean.Json
  | .error reason => Lean.Json.mkObj [("kind", .str "error"), ("message", .str (diagnosticText reason))]
  | .ok effects =>
    let spans := effects.filterMap fun
      | .changed path span => some (path, span)
      | _ => none
    let excluded := (effects.filterMap fun
      | .excluded path => some path
      | _ => none).eraseDups.mergeSort (· ≤ ·)
    let changes := (spans.map Prod.fst).eraseDups.map fun path =>
      (path, Lean.toJson (spans.filterMap fun (file, span) =>
        if file == path then some (Lean.Json.mkObj
          [("start", Lean.toJson span.start), ("end", Lean.toJson span.finish)]) else none))
    Lean.Json.mkObj [("kind", .str "ok"), ("changed", Lean.Json.mkObj changes),
      ("excluded", Lean.toJson excluded)]

private def expected (item : Fixture) := observe item.scope item.sections

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("scope", Lean.toJson item.scope), ("patch", .str (wire item)),
  ("expected", resultJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def oldCoordinates : Line → Line
  | .hunk (.coordinates old _ count) => .hunk (.coordinates old (.decimal old) count)
  | line => line

private def includeZeroHunk : Line → Line
  | .hunk (.coordinates old start (some (.decimal 0))) => .hunk (.coordinates old start none)
  | line => line

private def useLastNew (lines : List Line) : List Line :=
  match lines.reverse.findSome? (fun | .new header => some header | _ => none) with
  | none => lines
  | some header =>
    let rec replace : List Line → List Line
      | [] => []
      | .new _ :: rest => .new header :: rest
      | line :: rest => line :: replace rest
    replace lines

def sensitivity : IO UInt32 := do
  let viaLine (transform : Line → Line) (item : Fixture) :=
    observe item.scope (item.sections.map (·.map transform))
  for (name, broken) in [
      ("patch_use_old_coordinates", viaLine oldCoordinates),
      ("patch_keep_zero_count", viaLine includeZeroHunk),
      ("patch_allow_header_override_in_body", fun item => observe item.scope (item.sections.map useLastNew)),
      ("patch_merge_sections", fun item => observe item.scope [item.sections.flatten]),
      ("patch_ignore_scope", fun item => observe none item.sections),
      ("patch_drop_exclusions", fun item => (expected item).map (·.filter
        (fun | .excluded _ => false | _ => true)))] do
    match fixtures.find? (fun item =>
      (resultJson (broken item)).compress != (resultJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git patch corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitPatch
