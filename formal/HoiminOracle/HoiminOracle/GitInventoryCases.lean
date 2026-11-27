import HoiminOracle.GitInventoryModel
import HoiminOracle.PathOrderModel
import Lean.Data.Json

namespace HoiminOracle.GitInventory

private structure Fixture where
  id : String
  scope : List String
  inventory : List Record

private def statusToken : Status → String
  | .added => "A" | .modified => "M" | .deleted => "D" | .typeChanged => "T"
  | .renamed => "R099" | .copied => "C100" | .unmerged => "U" | .unknown => "X" | .broken => "B"

private def statusCases : List Fixture :=
  let source := "src/old.py"
  let target := "src/new.py"
  let scopes := [("neither", []), ("source", [source]), ("destination", [target]),
    ("both", [source, target])]
  ([.added, .modified, .deleted, .typeChanged, .renamed, .copied, .unmerged, .unknown, .broken] :
    List Status).flatMap fun status => scopes.map fun (suffix, scope) => {
    id := s!"status-{statusToken status}-{suffix}"
    scope := "src/ordinary.py" :: scope
    inventory := [(.modified, "src/ordinary.py", none),
      (status, source, if status == .renamed || status == .copied then some target else none)]
  }

private def ordinary (id : String) (paths : List String) : Fixture :=
  ⟨id, paths, paths.map fun path => (.modified, path, none)⟩

private def padded (stem : String) (length : Nat) (padding : Char := 'x') : String :=
  stem ++ String.ofList (List.replicate length padding) ++ ".py"

private def numbered (count padding : Nat) : List String :=
  (List.range count).map fun index => padded (toString (1000 + index)) padding

private def fixtures : List Fixture := statusCases ++ [
  ordinary "empty-inventory" [],
  ⟨"empty-scope", [], [(.renamed, "old.py", some "new.py"), (.modified, "src/file.py", none)]⟩,
  ordinary "deduplicated-and-sorted"
    ["z.py", "literal[1].py", "space name.py", "z.py", "-first.py", "a.py", "literal[1].py"],
  ordinary "nested-pathspec-string-order" ["pkg/a/child.py", "pkg/a.py"],
  ordinary "128-paths" (numbered 128 0).reverse,
  ordinary "129-paths" (numbered 129 0).reverse,
  ordinary "129-paths-at-64-bytes-each" (numbered 129 57).reverse,
  ordinary "two-paths-at-exact-byte-limit" [padded "a" 4092, padded "b" 4092],
  ordinary "two-paths-over-byte-limit" [padded "a" 4092, padded "b" 4093],
  ordinary "utf8-byte-limit" [padded "a" 2046 'é', padded "b" 2047 'é'],
  ordinary "8192-byte-single-path" [padded "a" 8188, "b.py"],
  ordinary "8193-byte-single-path" ["a.py", padded "b" 8189],
  ⟨"oversized-unselected", ["src/a.py"],
    [(.modified, padded "a" 8189, none), (.modified, "src/a.py", none)]⟩,
  ⟨"excluded-literal-backslash", ["src/a.py"],
    [(.renamed, "old.py", some "src\\a.py"), (.modified, "src/a.py", none)]⟩,
  ordinary "three-batches" (padded "0" 8188 :: (numbered 129 0).reverse)]

private def wire (record : Record) : String :=
  statusToken record.1 ++ "\x00" ++ record.2.1 ++ "\x00" ++
    (record.2.2.map (· ++ "\x00")).getD ""

private def resultJson : Plan → Lean.Json
  | .legacy => Lean.Json.mkObj [("kind", .str "legacy")]
  | .scoped batches => Lean.Json.mkObj [("kind", .str "scoped"), ("batches", Lean.toJson batches)]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("scope", Lean.toJson item.scope), ("inventory", .str (String.join (item.inventory.map wire))),
  ("expected", resultJson (plan item.scope item.inventory))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("inventory_use_origin", fun item : Fixture =>
        plan item.scope (item.inventory.map fun record => (record.1, record.2.1, none))),
      ("inventory_fallback_before_scope", fun item =>
        if item.inventory.any (requiresLegacy ∘ Prod.fst) then .legacy else plan item.scope item.inventory),
      ("inventory_keep_duplicates", fun item =>
        match plan item.scope item.inventory with
        | .legacy => .legacy
        | .scoped _ => .scoped (pack gitLimits (((item.inventory.map destination).filter
            item.scope.contains).mergeSort (· ≤ ·)))),
      ("inventory_exclusive_path_limit", fun item =>
        planWith { gitLimits with paths := gitLimits.paths - 1 } item.scope item.inventory),
      ("inventory_exclusive_byte_limit", fun item =>
        planWith { gitLimits with bytes := gitLimits.bytes - 1 } item.scope item.inventory),
      ("inventory_use_character_count", fun item =>
        planWith { gitLimits with measure := String.length } item.scope item.inventory),
      ("inventory_sort_pathspecs_by_components", fun item =>
        match plan item.scope item.inventory with
        | .legacy => .legacy
        | .scoped batches => .scoped (pack gitLimits (batches.flatten.mergeSort PathOrder.le))),
      ("inventory_omit_oversized_fallback", fun item =>
        let selected := item.inventory.filter fun record => item.scope.contains (destination record)
        if selected.any (requiresLegacy ∘ Prod.fst) then .legacy
        else .scoped (pack gitLimits (((selected.map destination).eraseDups).mergeSort (· ≤ ·))))] do
    match fixtures.find? (fun item => broken item != plan item.scope item.inventory) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git inventory corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitInventory
