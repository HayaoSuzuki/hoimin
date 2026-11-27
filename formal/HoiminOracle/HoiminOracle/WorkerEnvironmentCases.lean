import HoiminOracle.WorkerEnvironmentModel
import Lean.Data.Json

namespace HoiminOracle.WorkerEnvironment

private def tree : Tree :=
  { directories := [["project"], ["project", "pkg"], ["project", "pkg", "nested"], ["project", "vendor"],
      ["worker"], ["worker", "pkg"], ["worker", "pkg", "nested"], ["worker", "vendor"],
      ["outside"], ["outside", "lib"], ["project-neighbor"], ["project-neighbor", "pkg"]] }

private def withLinks : Tree :=
  { tree with links := [(["project", "inside-link"], ["project", "pkg", "nested"]),
      (["project", "outside-link"], ["outside", "lib"]),
      (["project", "broken-link"], ["missing-external"]), (["project-link"], ["project"])] }

private def relative (parts : Path) : PathInput := ⟨false, parts⟩
private def absolute (parts : Path) : PathInput := ⟨true, parts⟩

private def platformName : Platform → String
  | .unix => "unix"
  | .windows => "windows"

private structure Case where
  id : String
  platform : Platform
  input : Input
  links : Bool := false
  modules : List Path := []

private def matrix : List Case :=
  let imports : List (List Path) := [[], [["pkg"], ["."], ["vendor"], ["pkg"], ["pkg", "nested"]],
    [["ghost"], ["pkg"], ["ghost"]], [["Missing"], ["missing"]]]
  let sources : List (List PathInput) :=
    [ [], [relative ["pkg"], absolute ["project", "vendor"], relative [".", "pkg", "nested"]]
    , [relative ["ghost", "..", "vendor"], absolute ["project-neighbor", "pkg"], absolute ["outside", "lib"],
        relative ["..", "outside", "lib"]]
    , [absolute ["PROJECT", "Missing"], relative ["Missing"], relative ["missing"], absolute ["worker", "pkg"]] ]
  let inherit : List (List Variable) :=
    [ [], [⟨"PYTHONPATH", [absolute ["project", "pkg"], relative [], relative ["ghost"], absolute ["outside", "lib"]]⟩]
    , [⟨"pythonpath", [relative ["vendor"]]⟩, ⟨"PYTHONPATH", [absolute ["outside", "lib"]]⟩,
        ⟨"PythonPath", [absolute ["project-neighbor", "pkg"]]⟩]
    , [⟨"PYTHONPATH", []⟩, ⟨"pythonpath", [relative ["pkg"]]⟩] ]
  [.unix, .windows].flatMap fun platform => imports.zipIdx.flatMap fun (importRoots, i) =>
    sources.zipIdx.flatMap fun (sourceRoots, s) => inherit.zipIdx.map fun (inherited, e) =>
      ⟨s!"{platformName platform}-{i}-{s}-{e}", platform,
        { imports := importRoots, sources := sourceRoots, inherited }, false, []⟩

private def nativeCases : List Case :=
  [ ⟨"unix-join-error", .unix, { imports := [["colon:name"]] }, false, []⟩
  , ⟨"windows-join-error", .windows, { imports := [["quote\"name"]] }, false, []⟩
  , ⟨"unix-spaces-and-semicolon", .unix,
      { imports := [["path with spaces"]], inherited := [⟨"PYTHONPATH", [relative ["semi;colon"]]⟩] }, false, []⟩
  , ⟨"windows-spaces-and-semicolon", .windows,
      { imports := [["path with spaces"]], inherited := [⟨"PYTHONPATH", [relative ["semi;colon"]]⟩] }, false, []⟩ ]

private def linkCases : List Case :=
  [ ⟨"inside-link-sources", .unix, { sources := [relative ["inside-link"], relative ["pkg", "nested"]] }, true, []⟩
  , ⟨"explicit-import-retains-link-name", .unix,
      { imports := [["inside-link"]], sources := [relative ["inside-link"]] }, true, []⟩
  , ⟨"outside-link-inherited", .unix,
      { inherited := [⟨"PYTHONPATH", [relative ["outside-link"], absolute ["outside", "lib"]]⟩] }, true, []⟩
  , ⟨"outside-link-before-parent", .unix, { sources := [relative ["outside-link", ".."]] }, true, []⟩
  , ⟨"broken-link-and-missing-descendant", .unix,
      { sources := [relative ["broken-link"], relative ["inside-link", "missing"], relative ["outside-link", "missing"]] }, true, []⟩
  , ⟨"canonical-original-alias", .unix,
      { original := ["project-link"], sources := [relative ["pkg"], absolute ["project", "pkg", "nested"],
          absolute ["project-neighbor", "pkg"]] }, true, []⟩ ]

private def probes (platform : Platform) : List Case :=
  let name := platformName platform
  [ ⟨s!"{name}-probe-root", platform, { imports := [["pkg"]] }, false, [["worker"], ["worker", "pkg"]]⟩
  , ⟨s!"{name}-probe-import", platform,
      { imports := [["vendor"]], sources := [relative ["pkg"]],
        inherited := [⟨"PYTHONPATH", [absolute ["outside", "lib"]]⟩] }, false,
      [["worker", "vendor"], ["worker", "pkg"], ["outside", "lib"]]⟩
  , ⟨s!"{name}-probe-source", platform,
      { sources := [relative ["pkg", "nested"]], inherited := [⟨"PYTHONPATH", [relative ["vendor"]]⟩] }, false,
      [["worker", "pkg", "nested"], ["worker", "vendor"]]⟩
  , ⟨s!"{name}-probe-external", platform,
      { sources := [relative ["vendor"]], inherited := [⟨"PYTHONPATH", [absolute ["outside", "lib"]]⟩] }, false,
      [["project", "vendor"], ["outside", "lib"]]⟩ ]

private def cases := matrix ++ nativeCases ++ linkCases ++ probes .unix ++ probes .windows

private def fixtureTree (item : Case) := if item.links then withLinks else tree

private def fixtureInput (item : Case) : Input :=
  { item.input with inherited := ⟨"KEEP", [relative ["literal", "unmodified"]]⟩ :: item.input.inherited }

private def pathInputJson (path : PathInput) : Lean.Json :=
  Lean.Json.mkObj [("absolute", Lean.toJson path.absolute), ("parts", Lean.toJson path.parts)]

private def inputJson (input : Input) : Lean.Json :=
  Lean.Json.mkObj
    [ ("original", Lean.toJson input.original), ("worker", Lean.toJson input.worker)
    , ("imports", Lean.toJson input.imports), ("sources", .arr (input.sources.map pathInputJson).toArray)
    , ("inherited", .arr (input.inherited.map (fun item => Lean.Json.mkObj
        [("name", .str item.name), ("paths", .arr (item.paths.map pathInputJson).toArray)])).toArray) ]

private def resultJson (result : Option Environment) : Lean.Json := match result with
  | none => .null
  | some env => Lean.Json.mkObj [("cwd", Lean.toJson env.cwd), ("paths", Lean.toJson env.searchPath),
      ("retained", Lean.toJson env.retained)]

private def evaluate (item : Case) := build item.platform (fixtureTree item) (fixtureInput item)

def corpus : String :=
  String.join (cases.map fun item =>
    let layout := fixtureTree item
    let result := evaluate item
    let winner := result.bind fun env => env.searchPath.find? (fun path => path ∈ item.modules)
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id)
      , ("platform", .str (platformName item.platform)), ("directories", Lean.toJson layout.directories)
      , ("links", Lean.toJson layout.links), ("input", inputJson (fixtureInput item))
      , ("expected", resultJson result), ("modules", Lean.toJson item.modules)
      , ("winner", Lean.toJson winner) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Option Environment)) :=
    [ ("inherit_before_explicit_roots", fun item =>
        let input := fixtureInput item
        let first := input.worker :: (orderedPaths item.platform (fixtureTree item) { input with imports := [] }).drop 1
        let imports := (orderedPaths item.platform (fixtureTree item) { input with sources := [], inherited := [] }).drop 1
        compose item.platform (first ++ imports) input)
    , ("ignore_resolved_symlinks", fun item => build item.platform tree (fixtureInput item))
    , ("treat_case_sensitively_on_windows", fun item => build .unix (fixtureTree item) (fixtureInput item))
    , ("drop_external_paths", fun item =>
        let input := fixtureInput item
        let paths := orderedPaths item.platform (fixtureTree item) input
        compose item.platform (paths.filter (fun path => (inside item.platform input.worker path).isSome)) input)
    , ("permit_unjoinable_path", fun item =>
        let input := fixtureInput item
        let paths := orderedPaths item.platform (fixtureTree item) input
        (evaluate item).orElse fun _ => some ⟨input.worker, orderedUnique item.platform [] paths, ["KEEP"]⟩)
    , ("last_spelling_wins", fun item =>
        let input := fixtureInput item
        compose item.platform (orderedUnique item.platform [] (orderedPaths item.platform (fixtureTree item) input).reverse).reverse input) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ evaluate item)) then
      IO.println s!"worker environment sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"worker environment missed variant: {name}"
      return 1
  IO.println s!"worker_environment_cases={cases.length} unix={(cases.filter fun item => item.platform = .unix).length} probes={(cases.filter fun item => !item.modules.isEmpty).length}"
  return 0

end HoiminOracle.WorkerEnvironment
