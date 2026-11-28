import HoiminOracle.BoundedFileDiscoveryModel
import HoiminOracle.PathOrderModel
import Lean.Data.Json

namespace HoiminOracle.BoundedFileDiscovery

private structure Shape where
  id : String
  files : List String := []
  ignored : List String := []
  includes : List String := []
  rootAvailable : Bool := true
  unixOnly : Bool := false
  normal : List Entry := [.other]
  restored : List Entry := []

private def shapes : List Shape := [
  { id := "empty" },
  { id := "non-python", files := ["settings.txt"], normal := [.other, .other] },
  { id := "directory-with-python-suffix", files := ["folder.py/settings.txt"], normal := [.other, .other, .other] },
  { id := "two-distinct-python", files := ["one.py", "two.PY"], normal := [.other, .python "one.py", .python "two.PY"] },
  { id := "duplicate-in-restore", files := ["app.py"], includes := ["app.py"],
    normal := [.other, .python "app.py"], restored := [.other, .python "app.py"] },
  { id := "restore-an-ignored-file", files := ["new.py"], ignored := ["new.py"], includes := ["new.py"],
    restored := [.other, .python "new.py"] },
  { id := "new-path-in-second-pass", files := ["base.py", "new.py"], ignored := ["new.py"], includes := ["new.py"],
    normal := [.other, .python "base.py"], restored := [.other, .python "new.py"] },
  { id := "unavailable-root", rootAvailable := false, normal := [.walkFailure] },
  { id := "malformed-python-path", files := ["odd\\name.py"], unixOnly := true,
    normal := [.other, .malformed "odd\\name.py" true] },
  { id := "malformed-non-python-path-is-ignored", files := ["odd\\name.txt"], unixOnly := true,
    normal := [.other, .malformed "odd\\name.txt" false] }]

private structure Fixture where
  shape : Shape
  limit : Nat
  cancelAt : Option Nat

private def Fixture.id (item : Fixture) : String :=
  s!"{item.shape.id}-{item.limit}-{(item.cancelAt.map toString).getD "no-cancel"}"

private def fixtures : List Fixture := shapes.flatMap fun shape =>
  (List.range 4).flatMap fun limit => (none :: (List.range 5).map (some ∘ (· + 1))).map fun cancelAt =>
    ⟨shape, limit, cancelAt⟩

private def expected (item : Fixture) : State := scan item.limit item.cancelAt item.shape.normal item.shape.restored

private def resultJson (state : State) : Lean.Json :=
  let fields := match state.outcome with
    | .ok paths => [("kind", Lean.Json.str "ok"), ("paths", Lean.toJson (paths.mergeSort PathOrder.le))]
    | .error (.unsupportedPath path) => [("kind", .str "unsupported_path"), ("path", .str path)]
    | .error .walk => [("kind", .str "walk")]
    | .error .cancelled | .error .overLimit => [("kind", .str "analysis_limit")]
  Lean.Json.mkObj (("polls", Lean.toJson state.polls) :: fields)

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("files", Lean.toJson item.shape.files), ("ignored", Lean.toJson item.shape.ignored),
  ("includes", Lean.toJson item.shape.includes), ("root_available", Lean.toJson item.shape.rootAvailable),
  ("unix_only", Lean.toJson item.shape.unixOnly), ("limit", Lean.toJson item.limit),
  ("cancel_at", Lean.toJson item.cancelAt), ("expected", resultJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def scanWith (process : State → Entry → State) (item : Fixture) : State :=
  (item.shape.normal ++ item.shape.restored).foldl process {}

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("bounded_discovery_checks_capacity_before_duplicates", fun item : Fixture =>
        let early paths entry := match entry with
          | .python _ => if item.limit ≤ paths.length then .error .overLimit else admit item.limit paths entry
          | _ => admit item.limit paths entry
        scanWith (stepWith early item.cancelAt) item),
      ("bounded_discovery_renews_capacity_for_restore", fun item =>
        let initial := item.shape.normal.foldl (step item.limit item.cancelAt) {}
        let consumed := match initial.outcome with | .ok paths => paths.length | _ => 0
        item.shape.restored.foldl (step (item.limit + consumed) item.cancelAt) initial),
      ("bounded_discovery_skips_cancel_poll_for_other_entries", fun item =>
        let process state entry := match entry with
          | .other => state
          | _ => step item.limit item.cancelAt state entry
        scanWith process item),
      ("bounded_discovery_reports_error_before_cancel", fun item =>
        let process (state : State) entry := match state.outcome with
          | .error _ => state
          | .ok paths => match admit item.limit paths entry with
            | .error reason => ⟨state.polls + 1, .error reason⟩
            | .ok _ => step item.limit item.cancelAt state entry
        scanWith process item),
      ("bounded_discovery_checks_non_python_spelling", fun item =>
        let process state entry := match entry with
          | .malformed path false => step item.limit item.cancelAt state (.malformed path true)
          | _ => step item.limit item.cancelAt state entry
        scanWith process item)] do
    match fixtures.find? (fun item => (resultJson (broken item)).compress != (resultJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"bounded discovery corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.BoundedFileDiscovery
