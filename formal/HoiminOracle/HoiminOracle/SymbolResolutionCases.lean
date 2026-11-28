import HoiminOracle.SymbolResolutionModel
import Lean.Data.Json

namespace HoiminOracle.SymbolResolution

private structure Fixture where
  id : String
  sources : List String
  files : List File
  selectors : List Selector
  checkDefinitions : Bool := false

private def choices (path : String) : List (String × List File) :=
  [("absent", []), ("regular", [⟨path, false, []⟩]), ("python", [{ path }])]

private def sourceOrders : List (String × List String) :=
  [("first-before-second", ["first", "second"]), ("second-before-first", ["second", "first"])]

private def matrix : List Fixture := sourceOrders.flatMap fun (order, sources) =>
  (choices "first/calc.py").flatMap fun (firstModule, a) =>
    (choices "first/calc/__init__.py").flatMap fun (firstPackage, b) =>
      (choices "second/calc.py").flatMap fun (secondModule, c) =>
        (choices "second/calc/__init__.py").map fun (secondPackage, d) => {
          id := s!"{order}-{firstModule}-{firstPackage}-{secondModule}-{secondPackage}",
          sources, files := a ++ b ++ c ++ d ++ [{ path := "first/unrelated.py" }, { path := "first_extra/calc.py" }],
          selectors := [⟨"calc", "Widget.run"⟩] }

private def definitionFiles : List File := [
  { path := "first/calc.py", definitions := ["other"] },
  { path := "first/calc/__init__.py", definitions := ["selected"] },
  { path := "second/calc.py", definitions := ["selected"] }]

private def fixtures : List Fixture := matrix ++ [
  { id := "dotted-module", sources := ["first", "second"],
    files := [{ path := "first/pkg/nested/__init__.py" }, { path := "second/pkg/nested.py" },
      { path := "first/nested.py" }], selectors := [⟨"pkg.nested", "run"⟩] },
  { id := "project-root-source", sources := [""], files := [{ path := "calc.py" }, { path := "nested/calc.py" }],
    selectors := [⟨"calc", "f"⟩] },
  { id := "no-symbols", sources := ["first"],
    files := [{ path := "first/calc.py" }, { path := "second/calc.py" }], selectors := [] },
  { id := "outside-source-is-not-a-fallback", sources := ["first"],
    files := [{ path := "first_extra/calc.py" }, { path := "calc.py" }], selectors := [⟨"calc", "f"⟩] },
  { id := "merge-and-sort-per-file", sources := ["first", "second"],
    files := [{ path := "first/calc.py" }, { path := "second/util.py" }, { path := "second/calc.py" }],
    selectors := [⟨"calc", "z"⟩, ⟨"util", "z"⟩, ⟨"calc", "f"⟩, ⟨"calc", "z"⟩] },
  { id := "first-missing-module", sources := ["first"], files := [],
    selectors := [⟨"z", "f"⟩, ⟨"a", "f"⟩] },
  { id := "late-missing-module", sources := ["first"], files := [{ path := "first/calc.py" }],
    selectors := [⟨"calc", "f"⟩, ⟨"missing", "f"⟩] },
  { id := "definition-does-not-change-module-choice", sources := ["first", "second"],
    files := definitionFiles, selectors := [⟨"calc", "selected"⟩], checkDefinitions := true },
  { id := "definition-found-after-reordering-roots", sources := ["second", "first"],
    files := definitionFiles, selectors := [⟨"calc", "selected"⟩], checkDefinitions := true },
  { id := "definition-found-in-first-package", sources := ["first", "second"],
    files := definitionFiles.drop 1, selectors := [⟨"calc", "selected"⟩], checkDefinitions := true }]

private def expected (item : Fixture) := resolve item.sources item.files item.selectors

private def observationJson : Except String (List Target) → Lean.Json
  | .error module => Lean.Json.mkObj [("kind", .str "missing_module"), ("module", .str module)]
  | .ok targets => Lean.Json.mkObj [("kind", .str "ok"), ("targets", .arr (targets.toArray.map fun target =>
      Lean.Json.mkObj [("path", .str target.path), ("lines", .arr #[]), ("symbols", Lean.toJson target.symbols)]))]

private def validatedJson (item : Fixture) : Lean.Json :=
  if !item.checkDefinitions then .null else
  match expected item with
  | .error module => Lean.Json.mkObj [("kind", .str "missing_module"), ("module", .str module)]
  | .ok targets => match firstMissingDefinition item.files targets with
    | none => observationJson (.ok targets)
    | some (path, qualname) =>
      let selectors := item.selectors.filterMap fun selector =>
        if selector.qualname == qualname then some s!"{selector.module}:{selector.qualname}" else none
      let requested := String.intercalate ", " selectors
      Lean.Json.mkObj [("kind", .str "missing_definition"),
        ("message", .str s!"{path}: symbol definition not found: {qualname} (requested --symbol {requested})")]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("sources", Lean.toJson (item.sources.map fun source => if source.isEmpty then "." else source)),
  ("files", .arr (item.files.toArray.map fun file => Lean.Json.mkObj [
    ("path", .str file.path), ("is_python", Lean.toJson file.isPython), ("definitions", Lean.toJson file.definitions)])),
  ("selectors", .arr (item.selectors.toArray.map fun selector => Lean.Json.mkObj [
    ("module", .str selector.module), ("qualname", .str selector.qualname)])),
  ("expected", observationJson (expected item)), ("definition_check", validatedJson item)]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let via (search : Fixture → String → Option String) (item : Fixture) :=
    resolveWith (search item) item.sources item.files item.selectors
  for (name, broken) in [
      ("symbol_reverse_source_order", via fun item module => resolveModule item.sources.reverse item.files module),
      ("symbol_prefer_package_in_same_source", via fun item module =>
        (item.sources.flatMap fun source => [packageFile source module, moduleFile source module]).find? (available item.files)),
      ("symbol_prefer_module_files_across_sources", via fun item module =>
        ((item.sources.map (moduleFile · module)) ++ (item.sources.map (packageFile · module))).find? (available item.files)),
      ("symbol_allow_non_python", via fun item module =>
        (searchPaths module item.sources).find? fun path => item.files.any (·.path == path)),
      ("symbol_drop_dotted_module_prefix", via fun item module =>
        resolveModule item.sources item.files ((module.splitOn ".").getLastD module)),
      ("symbol_sort_targets_as_flat_strings", fun item =>
        (expected item).map (·.mergeSort (fun left right => left.path ≤ right.path)))] do
    match fixtures.find? (fun item =>
      (observationJson (broken item)).compress != (observationJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"symbol resolution corpus missed broken family {name}"
      return 2
  match fixtures.find? (fun item =>
    let definitions := item.files.flatMap (·.definitions)
    let global := { item with files := item.files.map fun file => { file with definitions } }
    (validatedJson global).compress != (validatedJson item).compress) with
  | some item =>
    IO.println s!"family=symbol_validate_definition_in_another_file detected=true case={item.id}"
    return 0
  | none =>
    IO.eprintln "symbol resolution corpus missed cross-file definition validation"
    return 2

end HoiminOracle.SymbolResolution
