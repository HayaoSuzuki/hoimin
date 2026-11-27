import HoiminOracle.GitNumstatModel
import Lean.Data.Json

namespace HoiminOracle.GitNumstat

open HoiminOracle.GitPatch (Scope)

private structure Fixture where
  id : String
  scope : Scope
  records : List Record
  seed : List String := []
  trailingNul : Bool := true

private def a : String := "pkg/a.py"
private def b : String := "pkg/b.PY"
private def seedPath : String := "pkg/seed.py"
private def unaffected : String := "pkg/other.py"
private def backslashPath : String := "pkg\\b.PY"

private def scopes : List (String × Scope) :=
  [("unscoped", none), ("empty", some []), ("source", some [a]),
    ("destination", some [b]), ("both", some [a,b])]

private def names : List (String × Name) :=
  [("a", .file a), ("b", .file b), ("text", .file "data.ini"), ("backslash", .backslash)]

private def counts : List (String × Counts) :=
  [("text", .text), ("binary", .binary), ("added-dash", .dashAdded), ("deleted-dash", .dashDeleted)]

private def matrix : List Fixture :=
  scopes.flatMap fun (scopeId, scope) => counts.flatMap fun (countId, count) =>
    names.flatMap fun (oldId, old) =>
      { id := s!"{scopeId}-{countId}-single-{oldId}", scope, records := [.ordinary count old] } ::
      names.map fun (newId, new) =>
        { id := s!"{scopeId}-{countId}-rename-{oldId}-{newId}", scope,
          records := [.renamed count old new] }

private def fixtures : List Fixture := matrix ++ [
  { id := "empty", scope := none, records := [] },
  { id := "preserve-existing-exclusion-empty-input", scope := none, records := [], seed := [seedPath] },
  { id := "both-rename-names-and-existing-exclusion", scope := some [a,b],
    records := [.renamed .binary (.file a) (.file b)], seed := [seedPath] },
  { id := "padding-replay-and-duplicates", scope := none, seed := [a,seedPath],
    records := [.padding, .ordinary .binary (.file b), .padding, .renamed .binary (.file b) (.file a),
      .ordinary .binary (.file b), .padding] },
  { id := "text-rename-consumes-fields-that-look-like-records", scope := none,
    records := [.renamed .text (.file "-\t-\tdecoy.py") (.file "2\t3\tother.py"),
      .ordinary .binary (.file a)] },
  { id := "binary-path-preserves-tabs-and-newlines", scope := some ["pkg/tab\tline\ncafé.py"],
    records := [.ordinary .binary (.file "pkg/tab\tline\ncafé.py")] },
  { id := "last-path-without-nul", scope := none,
    records := [.ordinary .binary (.file a)], trailingNul := false },
  { id := "last-rename-without-nul", scope := none,
    records := [.renamed .binary (.file a) (.file b)], trailingNul := false },
  { id := "dotfile-without-extension", scope := none, records := [.ordinary .binary (.file "pkg/.py")] },
  { id := "outside-scope-binary-path-still-decodes-utf8", scope := some [],
    records := [.ordinary .binary .invalidUtf8] },
  { id := "text-names-do-not-decode-utf8", scope := none,
    records := [.ordinary .text .invalidUtf8, .renamed .text .invalidUtf8 .invalidUtf8,
      .ordinary .binary (.file a)] },
  { id := "backslash-source-diagnostic-precedes-destination-utf8", scope := none,
    records := [.renamed .binary .backslash .invalidUtf8] },
  { id := "scoped-backslash-source-does-not-hide-destination-utf8", scope := some [],
    records := [.renamed .binary .backslash .invalidUtf8] },
  { id := "invalid-utf8-source-before-destination-backslash", scope := none,
    records := [.renamed .binary .invalidUtf8 .backslash] },
  { id := "empty-rename-path-before-invalid-utf8", scope := none,
    records := [.incompleteRename .binary .emptyNew] },
  { id := "text-rename-requires-nonempty-old-and-new", scope := some [],
    records := [.incompleteRename .text .emptyOld] },
  { id := "no-rename-names", scope := none, records := [.incompleteRename .binary .missingBoth] },
  { id := "truncated-text-rename", scope := none, records := [.incompleteRename .text .missingNew] },
  { id := "first-malformed-record-ends-observation", scope := none,
    records := [.ordinary .binary (.file a), .missingColumns, .ordinary .binary .invalidUtf8] }]

private def utf8 (text : String) : List UInt8 := text.toUTF8.data.toList

private def bytes : Name → List UInt8
  | .file path => utf8 path
  | .backslash => utf8 backslashPath
  | .invalidUtf8 => [255] ++ utf8 ".py"

private def columns : Counts → String
  | .text => "3\t2\t"
  | .binary => "-\t-\t"
  | .dashAdded => "-\t2\t"
  | .dashDeleted => "3\t-\t"

private def wireRecord : Record → List UInt8
  | .ordinary count name => utf8 (columns count) ++ bytes name ++ [0]
  | .renamed count source target =>
    utf8 (columns count) ++ [0] ++ bytes source ++ [0] ++ bytes target ++ [0]
  | .missingColumns => utf8 "-\t-" ++ [0]
  | .incompleteRename count problem =>
    utf8 (columns count) ++ [0] ++ (match problem with
      | .missingBoth => []
      | .missingNew => bytes (.file a) ++ [0]
      | .emptyOld => [0] ++ bytes (.file b) ++ [0]
      | .emptyNew => bytes .invalidUtf8 ++ [0,0])
  | .padding => [0]

private def wire (item : Fixture) : List Nat :=
  let input := item.records.flatMap wireRecord
  (if item.trailingNul then input else input.dropLast).map UInt8.toNat

private def diagnosticText : Diagnostic → String
  | .record => "invalid Git numstat binary record"
  | .rename => "invalid Git numstat rename record"
  | .invalidUtf8 => "Git numstat path is not valid UTF-8"
  | .backslash => "path contains an unsupported literal backslash: " ++ backslashPath

private def changedPaths : List String := [a,b,seedPath,unaffected]

private def changesJson (paths : List String) : Lean.Json := Lean.Json.mkObj (paths.map fun path =>
  (path, .arr #[Lean.Json.mkObj [("start", Lean.toJson (2 : Nat)), ("end", Lean.toJson (3 : Nat))]]))

private def resultJson (applyExclusions : Bool) : Except Diagnostic (List String) → Lean.Json
  | .error diagnostic => Lean.Json.mkObj [("kind", .str "error"), ("message", .str (diagnosticText diagnostic))]
  | .ok paths => Lean.Json.mkObj [
    ("kind", .str "ok"), ("excluded", Lean.toJson (paths.eraseDups.mergeSort (· ≤ ·))),
    ("surviving", changesJson (if applyExclusions then survivingPaths changedPaths paths else changedPaths))]

private def expected (item : Fixture) := resultJson true (observe item.scope item.seed item.records)

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("scope", Lean.toJson item.scope), ("input", Lean.toJson (wire item)), ("seed", Lean.toJson item.seed),
  ("changed", changesJson changedPaths), ("expected", expected item)]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def destinationOnly : Record → Record
  | .renamed .binary _ destination => .ordinary .binary destination
  | record => record

private def treatLoneDashAsBinary : Record → Record
  | .ordinary .dashAdded name | .ordinary .dashDeleted name => .ordinary .binary name
  | .renamed .dashAdded source target | .renamed .dashDeleted source target => .renamed .binary source target
  | record => record

private def ignoreIncompleteTextRename : Record → Record
  | .incompleteRename .text _ => .padding
  | record => record

def sensitivity : IO UInt32 := do
  let viaRecord (transform : Record → Record) (item : Fixture) :=
    resultJson true (observe item.scope item.seed (item.records.map transform))
  for (name, broken) in [
    ("numstat_use_destination_only", viaRecord destinationOnly),
    ("numstat_single_dash_means_binary", viaRecord treatLoneDashAsBinary),
    ("numstat_skip_text_rename_framing", viaRecord ignoreIncompleteTextRename),
    ("numstat_ignore_scope", fun item => resultJson true (observe none item.seed item.records)),
    ("numstat_drop_patch_exclusions", fun item => resultJson true (observe item.scope [] item.records)),
    ("numstat_skip_final_exclusions", fun item => resultJson false (observe item.scope item.seed item.records))] do
    match fixtures.find? (fun item => (broken item).compress != (expected item).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git numstat corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitNumstat
