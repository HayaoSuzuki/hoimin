import HoiminOracle.GitWorktreeModel
import HoiminOracle.PathOrderModel
import Lean.Data.Json

namespace HoiminOracle.GitWorktree

open HoiminOracle.GitPhysicalRows (Span)

private structure Fixture where
  id : String
  inventory : List InventoryEntry
  scope : Scope := none
  files : Files := []
  rootAvailable : Bool := true
  unixOnly : Bool := false

private def a : String := "pkg/a.py"
private def b : String := "pkg/b.py"
private def blockedA : String := "block-a/child.py"
private def blockedZ : String := "block-z/child.py"
private def valid : FileEntry := .file [97,13,98,13,10,99]
private def initial : Changes := [(a, ⟨9,11⟩), ("preserved.py", ⟨5,5⟩)]

private def cases : List Fixture :=
  let scopes : List (String × Scope) := [
    ("all", none), ("empty", some []), ("whole-a", some [⟨a, []⟩]),
    ("past-current-a", some [⟨a, [⟨4294967295,4294967295⟩]⟩]),
    ("zero-only-a", some [⟨a, [⟨0,0⟩]⟩]), ("only-b", some [⟨b, []⟩])]
  let files : List (String × Option FileEntry) := [
    ("missing", none), ("directory", some .directory), ("empty", some (.file [])),
    ("binary", some (.file [97,0,10])), ("text", some valid)]
  let names := [("single", [a]), ("repeat", [a,a]), ("second-first", [b,a]), ("mixed-extension", [a,"data.txt"])]
  scopes.flatMap fun (scopeId, scope) => files.flatMap fun (fileId, file) =>
    names.map fun (nameId, paths) => {
      id := s!"{scopeId}-{fileId}-{nameId}", scope, inventory := paths.map InventoryEntry.path,
      files := (file.map (a, ·)).toList ++ [(b, .file [13,10]), ("data.txt", valid)]
    }

private def fixtures : List Fixture := cases ++ [
  { id := "invalid-utf8-before-empty-scope-and-root", inventory := [.invalidUtf8],
    scope := some [], rootAvailable := false },
  { id := "invalid-utf8-after-valid-record", inventory := [.path a, .invalidUtf8],
    files := [(a,valid)] },
  { id := "literal-backslash-unscoped", inventory := [.path b, .path "pkg\\a.py"],
    files := [(b,valid)] },
  { id := "literal-backslash-does-not-alias-scoped-path", inventory := [.path "pkg\\a.py", .path b],
    scope := some [⟨a, []⟩, ⟨b, []⟩], files := [(a,valid),(b,valid)] },
  { id := "empty-scope-skips-root-open", inventory := [.path a], scope := some [], rootAvailable := false },
  { id := "nonempty-scope-opens-root-with-no-matching-record", inventory := [.path b],
    scope := some [⟨a, []⟩], rootAvailable := false },
  { id := "unscoped-empty-list-opens-root", inventory := [], rootAvailable := false },
  { id := "padding-records-ignored", inventory := [.padding,.path b,.padding,.path a,.padding],
    files := [(b,valid),(a,valid)] },
  { id := "read-error-after-success-is-atomic", inventory := [.path b,.path blockedZ],
    files := [(b,valid),(blockedZ,.blockedParent)], unixOnly := true },
  { id := "unscoped-first-failure-follows-inventory", inventory := [.path b,.path blockedZ,.path blockedA],
    files := [(b,valid),(blockedA,.blockedParent),(blockedZ,.blockedParent)], unixOnly := true },
  { id := "scoped-first-failure-follows-targets", inventory := [.path blockedZ,.path blockedA],
    scope := some [⟨blockedA, []⟩, ⟨blockedZ, []⟩],
    files := [(blockedA,.blockedParent),(blockedZ,.blockedParent)], unixOnly := true },
  { id := "unscoped-nested-errors-follow-git-record-order",
    inventory := [.path "pkg/a.py/child.py", .path "pkg/a/child.py"],
    files := [("pkg/a.py/child.py", .blockedParent), ("pkg/a/child.py", .blockedParent)], unixOnly := true },
  { id := "scoped-nested-errors-follow-explicit-target-order",
    inventory := [.path "pkg/a.py/child.py", .path "pkg/a/child.py"],
    scope := some [⟨"pkg/a/child.py", []⟩, ⟨"pkg/a.py/child.py", []⟩],
    files := [("pkg/a.py/child.py", .blockedParent), ("pkg/a/child.py", .blockedParent)], unixOnly := true },
  { id := "lexical-failure-precedes-read-failure", inventory := [.path blockedZ,.invalidUtf8],
    files := [(blockedZ,.blockedParent)] },
  { id := "scope-filters-blocked-parent-before-read", inventory := [.path blockedZ,.path a],
    scope := some [⟨a, []⟩], files := [(blockedZ,.blockedParent),(a,valid)] },
  { id := "only-a-real-extension-is-admitted", inventory := [.path "pkg/.py",.path "pkg/value.PY"],
    files := [("pkg/.py",valid),("pkg/value.PY",.file [255,13,66])] }]

private def wire : InventoryEntry → List UInt8
  | .path path => path.toUTF8.data.toList ++ [0]
  | .invalidUtf8 => [255,46,112,121,0]
  | .padding => [0]

def spanJson (span : Span) : Lean.Json := Lean.Json.mkObj [
  ("start", Lean.toJson span.start), ("end", Lean.toJson span.finish)]

private def changesJson (changes : Changes) : Lean.Json :=
  Lean.Json.mkObj ((changes.map Prod.fst).eraseDups.map fun path =>
    (path, .arr ((changes.filterMap fun (name, span) =>
      if name == path then some (spanJson span) else none).toArray)))

def targetJson (target : Target) : Lean.Json := Lean.Json.mkObj [
  ("path", .str target.path), ("lines", .arr (target.lines.toArray.map spanJson)), ("symbols", .arr #[])]

def fileJson (entry : String × FileEntry) : Lean.Json :=
  let contents := match entry.2 with
    | .file bytes => [("kind", .str "file"), ("bytes", Lean.toJson (bytes.map UInt8.toNat))]
    | .directory => [("kind", .str "directory")]
    | .blockedParent => [("kind", .str "blocked_parent")]
    | .symlink target => [("kind", .str "symlink"), ("target", .str target)]
  Lean.Json.mkObj (("path", .str entry.1) :: contents)

def diagnosticJson : Diagnostic → Lean.Json
  | .invalidUtf8 => Lean.Json.mkObj [("kind", .str "invalid_utf8")]
  | .rootUnavailable => Lean.Json.mkObj [("kind", .str "root_unavailable")]
  | .literalBackslash path => Lean.Json.mkObj [("kind", .str "literal_backslash"), ("path", .str path)]
  | .readParent path => Lean.Json.mkObj [("kind", .str "read_parent"), ("path", .str path)]

private def observationJson (result : Observation) : Lean.Json := Lean.Json.mkObj [
  ("changed", changesJson result.changed), ("failure", (result.failure.map diagnosticJson).getD .null)]

private def expected (item : Fixture) :=
  publish initial (scan item.rootAvailable item.scope item.inventory item.files)

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("root_available", Lean.toJson item.rootAvailable), ("unix_only", Lean.toJson item.unixOnly),
  ("inventory", Lean.toJson ((item.inventory.flatMap wire).map UInt8.toNat)),
  ("scope", (item.scope.map fun targets => .arr (targets.toArray.map targetJson)).getD .null),
  ("files", .arr (item.files.toArray.map fileJson)), ("initial", changesJson initial),
  ("expected", observationJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def publishEarly (item : Fixture) : Observation :=
  let result := expected item
  match result.failure, selectPaths item.scope item.inventory with
  | some (.readParent path), .ok paths =>
    match collect item.files (paths.takeWhile (· != path)) with
    | .ok added => { result with changed := initial ++ added }
    | .error _ => result
  | _, _ => result

private def intersectOnRead (item : Fixture) : Observation :=
  let clipped := (scan item.rootAvailable item.scope item.inventory item.files).map
    (·.filter fun (path, span) => item.scope.all fun targets =>
      targets.any fun target => target.path == path &&
        (target.lines.isEmpty || target.lines.any fun line => max line.start span.start ≤ min line.finish span.finish))
  publish initial clipped

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("worktree_skip_decode_for_empty_scope", fun item : Fixture =>
        if item.scope.any List.isEmpty then publish initial (.ok []) else expected item),
      ("worktree_ignore_scope", fun item => expected { item with scope := none }),
      ("worktree_intersect_before_returning_whole_file", intersectOnRead),
      ("worktree_accept_nul_content", fun item => expected { item with files := item.files.map fun
        | (path, .file bytes) => (path, .file (bytes.filter (· != 0)))
        | file => file }),
      ("worktree_reorder_unscoped_reads_by_path", fun item =>
        if item.scope.isNone && item.rootAvailable then
          publish initial ((selectPaths item.scope item.inventory).bind fun paths =>
            collect item.files (paths.mergeSort PathOrder.le))
        else expected item),
      ("worktree_publish_before_reads_complete", publishEarly)] do
    match fixtures.find? (fun item =>
      (observationJson (broken item)).compress != (observationJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git worktree corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitWorktree
