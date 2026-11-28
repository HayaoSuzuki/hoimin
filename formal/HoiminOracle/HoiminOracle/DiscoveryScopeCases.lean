import HoiminOracle.DiscoveryScopeModel
import Lean.Data.Json

namespace HoiminOracle.DiscoveryScope

private structure Fixture where
  id : String
  selection : Selection
  files : List File
  unixOnly : Bool := false

private def python (path : String) : File := ⟨path.splitOn "/", .python⟩

private def tree : List File :=
  ["top.py", "selected/app.py", "selected/nested/keep.py", "selected/nested/neighbor.PY",
    "selected/sibling/drop.py", "selected-other/shadow.py", "outside/nested/test.py",
    "venv/built_in.py", "selected/venv/built_in.py"].map python ++
  [⟨["selected", "data.bin"], .regular⟩]

private def matrix : List Fixture :=
  let sources : List (String × List Path) := [
    ("no-source", []), ("source", [["selected"]]), ("nested-source", [["selected", "nested"]]),
    ("two-sources", [["selected"], ["outside"]]), ("root-source", [["."]]), ("invalid-source", [["..", "outside"]])]
  let exact : List (String × List Path × List Path) := [
    ("no-exact", [], []), ("file", [[".", "selected", "nested", "..", "app.py"]], []),
    ("line", [], [["selected", "nested", "keep.py"]]),
    ("prefix-sibling", [["selected-other", "shadow.py"]], []),
    ("file-and-line", [["top.py"]], [["outside", "nested", "test.py"]]),
    ("invalid-exact", [["..", "missing.py"]], [])]
  sources.flatMap fun (sourceId, roots) => exact.flatMap fun (exactId, files, lines) =>
    [false, true].map fun hasSymbol => ⟨s!"{sourceId}-{exactId}-symbol-{hasSymbol}",
      ⟨roots, files, lines, hasSymbol⟩, tree, false⟩

private def odd : File := ⟨["selected", "odd\\part", "app.py"], .unsupported⟩
private def nativeTree : List File := tree ++ [odd]

private def fixtures : List Fixture := matrix ++ [
  ⟨"source-points-to-regular-python-file", { sources := [["top.py"]] }, tree, false⟩,
  ⟨"source-reduces-to-project-root", { sources := [["selected", ".."]] }, tree, false⟩,
  ⟨"exact-file-does-not-open-named-directory", { files := [["selected", "nested"]] }, tree, false⟩,
  ⟨"exact-file-prunes-malformed-sibling", { files := [["selected", "app.py"]] }, nativeTree, true⟩,
  ⟨"broad-walk-diagnoses-malformed-path", {}, nativeTree, true⟩,
  ⟨"source-walk-diagnoses-malformed-path", { sources := [["selected"]] }, nativeTree, true⟩,
  ⟨"exact-malformed-file-keeps-native-parents", { files := [odd.path] }, nativeTree, true⟩,
  ⟨"native-backslash-does-not-alias-portable-separator", { files := [["selected", "odd", "part", "app.py"]] },
    nativeTree, true⟩,
  ⟨"built-in-exclusion-rejects-explicit-source", { sources := [["venv"]] }, tree, false⟩]

private def recordJson (record : Record) : Lean.Json :=
  .mkObj [("path", .str record.path), ("is_python", Lean.toJson record.isPython)]

private def resultJson (result : Result) : Lean.Json :=
  let outcome := match result.records with
    | .ok files => [("kind", .str "ok"), ("files", .arr (files.toArray.map recordJson)),
        ("visited", Lean.toJson result.visited)]
    | .error path => [("kind", .str "unsupported_path"), ("path", .str path)]
  .mkObj (("scoped", Lean.toJson result.scopeActive) :: outcome)

private def expected (item : Fixture) := discover item.selection item.files

private def fixtureJson (item : Fixture) : Lean.Json := .mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("unix_only", Lean.toJson item.unixOnly), ("sources", Lean.toJson (item.selection.sources.map spelling)),
  ("files", Lean.toJson (item.selection.files.map spelling)), ("lines", Lean.toJson (item.selection.lines.map spelling)),
  ("has_symbol", Lean.toJson item.selection.hasSymbol),
  ("paths", Lean.toJson (item.files.map (spelling ∘ File.path))), ("expected", resultJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("discovery_omit_line_selector", fun item : Fixture => expected { item with selection := { item.selection with lines := [] } }),
      ("discovery_symbol_without_source_still_prunes", fun item =>
        expected { item with selection := { item.selection with hasSymbol := false } }),
      ("discovery_use_string_prefix_for_source", fun item =>
        let prefixKeep scope directory path := path.isEmpty || scope.all fun active =>
          scopedKeeps (fun source child => (spelling child).startsWith (spelling source)) active directory path
        discoverWith prefixKeep builtInExcluded item.selection item.files),
      ("discovery_drop_exact_parent_directories", fun item =>
        let noParent scope (directory : Bool) path := path.isEmpty || scope.all fun active =>
          if directory then scopedKeeps List.isPrefixOf { active with exact := [] } true path
          else keeps scope false path
        discoverWith noParent builtInExcluded item.selection item.files),
      ("discovery_ignore_built_in_exclusions", fun item => discoverWith keeps (fun _ => false) item.selection item.files),
      ("discovery_silently_skip_malformed_file", fun item =>
        let recovered := expected { item with files := item.files.filter fun file => match file.kind with
          | .unsupported => false
          | _ => true }
        { recovered with visited := (expected item).visited })] do
    match fixtures.find? (fun item => (resultJson (broken item)).compress != (resultJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"discovery scope corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.DiscoveryScope
