import HoiminOracle.WorkspaceSelectionModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceSelection

private def pathName (path : Path) := String.intercalate "/" path

private def glob : Pattern → String
  | .all => "**"
  | .exact path => "/" ++ pathName path
  | .below path => "/" ++ pathName path ++ "/**"
  | .suffix suffix => "*" ++ suffix
  | .directory path => "/" ++ pathName path ++ "/"
  | .invalid => "["

private def dirs (paths : List Path) : List Entry := paths.map fun path => ⟨path, .directory⟩

private def files (items : List (Path × String)) : List Entry :=
  items.map fun (path, content) => ⟨path, .file content⟩

private def tree : List Entry :=
  dirs [["src"], ["src", "deep"], ["src", "deep", "empty"], ["fixtures"], ["fixtures", "nested"],
     ["fixtures", "empty"], ["state"], ["state-backup"], ["file-rule"], [".hidden"], ["case"],
     ["case", "ENV"], ["src", "__pycache__"], [".git"], [".venv"], ["venv"], ["env"], [".pytest_cache"],
     [".ruff_cache"], [".pyre"], [".pytype"], [".tox"], [".nox"]] ++
  files [(["src", "main.py"], "value = 1 + 2\n"), (["src", "deep", "keep.py"], "pass\n"),
     (["src", "deep", "skip.txt"], "deep text"), (["fixtures", "kept.txt"], "keep"),
     (["fixtures", "secret.txt"], "hidden file"), (["fixtures", "nested", "only.py"], "x = 8\n"),
     (["ignored.txt"], "ignored normally"), (["active[1].db"], "session ledger"), (["active1.db"], "ordinary db"),
     (["state", "run.lock"], "lock"), (["state-backup", "run.lock"], "copy"), (["tree-rule"], "ordinary file"),
     (["file-rule", "nested.txt"], "child"), ([".hidden", "kept.txt"], "dot directory"),
     (["case", "ENV", "config.py"], "uppercase default on Windows"), (["src", "__pycache__", "one.pyc"], "cache"),
     ([".git", "config"], ""), ([".venv", "child"], "env"), (["venv", "child"], "env"), (["env", "child"], "env"),
     ([".pytest_cache", "child"], "cache"), ([".mypy_cache"], "default applies to regular files"),
     ([".ruff_cache", "child"], "cache"), ([".pyre", "child"], "cache"), ([".pytype", "child"], "cache"),
     ([".tox", "child"], "cache"), ([".nox", "child"], "cache")]

private def symlinks : List Entry :=
  [ ⟨["external-link.py"], .link "file"⟩, ⟨["outside-dir"], .link "directory"⟩, ⟨["broken"], .link "missing"⟩
  , ⟨["fixtures", "ignored-link.py"], .link "file"⟩, ⟨["state", "link"], .link "file"⟩
  , ⟨[".venv", "excluded-link.py"], .link "file"⟩ ]

private structure Case where
  id : String
  rules : Rules
  links : Bool := false

private def platformName (foldCase : Bool) := if foldCase then "windows" else "unix"

private def matrix : List Case :=
  let ignored : List (List Pattern) := [[], [.exact ["ignored.txt"], .directory ["fixtures"]],
    [.suffix ".txt", .directory ["src", "deep"]]]
  let included : List (List Pattern) := [[], [.exact ["ignored.txt"], .exact ["fixtures", "nested", "only.py"],
    .exact ["fixtures", "empty"], .exact ["ignored.txt"]], [.all]]
  let excluded : List (List Pattern) := [[], [.exact ["fixtures", "secret.txt"], .below ["src", "deep"]], [.suffix ".TXT"]]
  let literals : List (List Literal) := [[], [.file ["active[1].db"], .tree ["state"]],
    [.file ["file-rule"], .tree ["tree-rule"]], [.file [".gitignore"], .tree ["fixtures", "nested"]]]
  [false, true].flatMap fun foldCase => ignored.zipIdx.flatMap fun (ignores, a) =>
    included.zipIdx.flatMap fun (includes, b) => excluded.zipIdx.flatMap fun (excludes, c) =>
      literals.zipIdx.map fun (literalRules, d) =>
        ⟨s!"{platformName foldCase}-{a}-{b}-{c}-{d}", ⟨foldCase, ignores, includes, excludes, literalRules⟩, false⟩

private def boundaries (foldCase : Bool) : List Case :=
  let platform := platformName foldCase
  let cases : List (String × Rules) :=
    [ ("restore-empty-dir-only", { ignores := [.directory ["fixtures"]], includes := [.exact ["fixtures", "empty"]] })
    , ("restore-descendants", { ignores := [.directory ["fixtures"]], includes := [.below ["fixtures"]] })
    , ("include-cannot-restore-excluded-ancestor", { includes := [.all], excludes := [.exact ["fixtures"]] })
    , ("case-folded-literal", { includes := [.all], literals := [.file ["SRC", "MAIN.PY"]] })
    , ("case-folded-override", { includes := [.all], excludes := [.exact ["SRC"]] })
    , ("invalid-include", { includes := [.invalid] })
    , ("invalid-exclude", { excludes := [.invalid] })
    , ("literal-path-error-first", { literals := [.file ["..", "outside"]], includes := [.invalid], excludes := [.invalid] }) ]
  cases.map fun (name, rules) => ⟨s!"{platform}-{name}", { rules with caseInsensitive := foldCase }, false⟩

private def linkCases : List Case :=
  [ ⟨"links-default", {}, true⟩
  , ⟨"links-in-both-walks", { includes := [.all] }, true⟩
  , ⟨"links-restored-through-ignore",
      { ignores := [.directory ["fixtures"]], includes := [.exact ["fixtures", "ignored-link.py"]] }, true⟩
  , ⟨"links-under-literal-tree", { includes := [.all], literals := [.tree ["state"]] }, true⟩
  , ⟨"links-literal-file-not-tree", { includes := [.all], literals := [.file ["outside-dir"], .tree ["external-link.py"]] }, true⟩ ]

private def cases := matrix ++ boundaries false ++ boundaries true ++ linkCases

private def sourceTree (item : Case) : List Entry :=
  tree ++ (if item.links then symlinks else []) ++
    (if item.rules.ignores.isEmpty then [] else
      files [([".gitignore"], String.join (item.rules.ignores.map fun pattern => glob pattern ++ "\n"))])

private def entryJson (entry : Entry) : Lean.Json :=
  Lean.Json.mkObj <| ("path", .str (pathName entry.path)) :: match entry.kind with
  | .directory => [("kind", .str "directory")]
  | .file content => [("kind", .str "file"), ("content", .str content)]
  | .link target => [("kind", .str "link"), ("target", .str target)]

private def literalJson (exclusion : Literal) : Lean.Json :=
  Lean.Json.mkObj [("kind", .str (match exclusion with | .file _ => "file" | .tree _ => "tree")),
    ("path", .str (pathName exclusion.path))]

private def resultJson : Result → Lean.Json
  | .invalidLiteral => Lean.Json.mkObj [("kind", .str "invalid_literal")]
  | .invalidGlob => Lean.Json.mkObj [("kind", .str "invalid_glob")]
  | .manifest value => Lean.Json.mkObj
    [ ("kind", .str "manifest"), ("files", Lean.Json.mkObj (value.files.map fun (path, content) => (pathName path, .str content)))
    , ("directories", Lean.toJson (value.directories.map pathName)), ("symlinks", Lean.toJson (value.symlinks.map pathName))
    , ("logical_bytes", Lean.toJson ((value.files.map fun (_, content) => content.utf8ByteSize).sum)) ]

def corpus : String :=
  String.join (cases.map fun item =>
    let source := sourceTree item
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("platform", .str (platformName item.rules.caseInsensitive)), ("source", .arr (source.map entryJson).toArray)
      , ("includes", Lean.toJson (item.rules.includes.map glob)), ("excludes", Lean.toJson (item.rules.excludes.map glob))
      , ("literals", .arr (item.rules.literals.map literalJson).toArray), ("expected", resultJson (run item.rules source)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Rules → Entry → Bool)) :=
    [ ("ignore_explicit_includes", fun rules entry => selected { rules with includes := [] } entry)
    , ("include_overrides_default", fun rules entry => selected rules entry ||
        (defaultExcluded rules.caseInsensitive entry && rules.includes.any (patternMatches rules.caseInsensitive entry.path entry.kind.isDirectory)))
    , ("include_overrides_hard_exclusions", fun rules entry => selected rules entry ||
        (!defaultExcluded rules.caseInsensitive entry && rules.includes.any (patternMatches rules.caseInsensitive entry.path entry.kind.isDirectory)))
    , ("include_overrides_explicit_glob_excludes", fun rules entry =>
        if rules.includes.any (patternMatches rules.caseInsensitive entry.path entry.kind.isDirectory) then
          selected { rules with excludes := [] } entry else selected rules entry)
    , ("treat_file_exclusion_as_tree", fun rules entry => selected { rules with literals := rules.literals.map fun literal => .tree literal.path } entry)
    , ("treat_tree_exclusion_as_file", fun rules entry => selected { rules with literals := rules.literals.map fun literal => .file literal.path } entry)
    , ("ignore_windows_case_rules", fun rules entry => selected { rules with caseInsensitive := false } entry) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        let source := sourceTree item
        decide (runWith variant item.rules source ≠ run item.rules source)) then
      IO.println s!"workspace selection sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace selection missed variant: {name}"
      return 1
  IO.println s!"workspace_selection_cases={cases.length} unix={(cases.filter fun c => !c.rules.caseInsensitive).length}"
  return 0

end HoiminOracle.WorkspaceSelection
