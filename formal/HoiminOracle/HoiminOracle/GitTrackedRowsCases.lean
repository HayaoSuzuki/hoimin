import HoiminOracle.GitTrackedRowsModel
import HoiminOracle.GitWorktreeCases

namespace HoiminOracle.GitTrackedRows

open HoiminOracle.GitWorktree (Files FileEntry Scope spanJson fileJson targetJson diagnosticJson)
open HoiminOracle.GitPhysicalRows (Span translate)

private structure Fixture where
  id : String
  rows : Rows
  files : Files := []
  scope : Scope := none
  rootAvailable : Bool := true
  unixOnly : Bool := false

private def a := "pkg/a.py"
private def b := "pkg/b.py"
private def blocked := "z-blocked/child.py"
private def first : List Span := [⟨1, 1⟩]
private def third : List Span := [⟨3, 3⟩]
private def readable : FileEntry := .file [97, 13, 98, 13, 10, 99, 10, 100, 13, 101, 10, 102]

private def matrix : List Fixture :=
  let scopes : List (String × Scope) := [
    ("all", none), ("empty", some []), ("a", some [⟨a, []⟩]), ("b", some [⟨b, []⟩]),
    ("both", some [⟨b, []⟩, ⟨a, []⟩]), ("a-far-line", some [⟨a, [⟨99, 99⟩]⟩])]
  let sources : List (String × Option FileEntry) := [
    ("missing", none), ("directory", some .directory), ("empty", some (.file [])),
    ("binary", some (.file [97, 0, 10])), ("cr", some (.file [97, 13, 98])),
    ("crlf", some (.file [97, 13, 10, 98])), ("mixed", some readable),
    ("non-utf8", some (.file [233, 13, 10, 102]))]
  let intervals : List (String × List Span) := [("first", first), ("third", third), ("gap", first ++ third)]
  scopes.flatMap fun (scopeId, scope) => sources.flatMap fun (sourceId, source) =>
    intervals.map fun (intervalId, spans) => {
      id := s!"{scopeId}-{sourceId}-{intervalId}", scope,
      rows := [(b, [⟨2, 2⟩]), (a, spans)],
      files := [(b, .file [97, 10, 98])] ++ (source.map (a, ·)).toList }

private def fixtures : List Fixture := matrix ++ [
  { id := "empty-input-skips-missing-root", rows := [], rootAvailable := false },
  { id := "no-matching-path-skips-missing-root", rows := [(a, first)],
    scope := some [⟨b, []⟩], rootAvailable := false },
  { id := "selected-path-opens-root", rows := [(a, first)], rootAvailable := false },
  { id := "line-selector-applies-after-translation", rows := [(a, first)],
    scope := some [⟨a, [⟨99, 99⟩]⟩], files := [(a, readable)] },
  { id := "second-file-outside-aborts", rows := [(a, first), (b, third)],
    files := [(a, readable), (b, .file [13, 10])] },
  { id := "second-file-read-error-aborts", rows := [(a, first), (blocked, first)],
    files := [(a, readable), (blocked, .blockedParent)], unixOnly := true },
  { id := "sorted-range-error-precedes-out-of-order-read-error", rows := [(blocked, first), (a, third)],
    files := [(a, .file [10]), (blocked, .blockedParent)],
    scope := some [⟨blocked, []⟩, ⟨a, []⟩], unixOnly := true },
  { id := "scope-skips-error-before-read", rows := [(a, first), (blocked, first)],
    files := [(a, readable), (blocked, .blockedParent)], scope := some [⟨a, []⟩] },
  { id := "symlink-is-skipped-even-with-live-destination", rows := [(a, third), (b, first)],
    files := [(a, .symlink "b.py"), (b, readable)], unixOnly := true },
  { id := "dangling-symlink-is-skipped", rows := [(a, third), (b, first)],
    files := [(a, .symlink "missing.py"), (b, readable)], unixOnly := true }]

private def rowsJson (rows : Rows) : Lean.Json := Lean.Json.mkObj (rows.map fun (path, ranges) =>
  (path, .arr (ranges.toArray.map spanJson)))

private def failureJson : Failure → Lean.Json
  | .source reason => diagnosticJson reason
  | .range .malformed => Lean.Json.mkObj [("kind", .str "malformed_range")]
  | .range .outside => Lean.Json.mkObj [("kind", .str "range_outside_source")]

private def observationJson : Except Failure Rows → Lean.Json
  | .ok rows => Lean.Json.mkObj [("kind", .str "ok"), ("changed", rowsJson rows)]
  | .error failure => Lean.Json.mkObj [("kind", .str "error"), ("failure", failureJson failure)]

private def expected (item : Fixture) := scan item.rootAvailable item.scope item.rows item.files

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("unix_only", Lean.toJson item.unixOnly), ("root_available", Lean.toJson item.rootAvailable),
  ("scope", (item.scope.map fun targets => .arr (targets.toArray.map targetJson)).getD .null),
  ("files", .arr (item.files.toArray.map fileJson)), ("ranges", rowsJson item.rows),
  ("expected", observationJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("tracked_rows_ignore_scope", fun item : Fixture => expected { item with scope := none }),
      ("tracked_rows_open_root_before_scope", fun item =>
        if item.rootAvailable then expected item else .error (.source .rootUnavailable)),
      ("tracked_rows_keep_git_coordinates", fun item =>
        let unchanged bytes spans := (translate bytes spans).map (fun _ => spans)
        scanWith (projectFile item.files unchanged) item.rootAvailable item.scope item.rows),
      ("tracked_rows_accept_nul_as_text", fun item => expected { item with files := item.files.map fun
        | (path, .file bytes) => (path, .file (bytes.filter (· != 0)))
        | file => file }),
      ("tracked_rows_publish_despite_stale_ranges", fun item =>
        let ignoreStale entry := match projectFile item.files translate entry with
          | .error (.range .outside) => .ok none
          | result => result
        scanWith ignoreStale item.rootAvailable item.scope item.rows),
      ("tracked_rows_use_input_order_for_errors", fun item =>
        if item.rootAvailable then collect (projectFile item.files translate) (selected item.scope item.rows)
        else expected item)] do
    match fixtures.find? (fun item =>
      (observationJson (broken item)).compress != (observationJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git tracked rows corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitTrackedRows
