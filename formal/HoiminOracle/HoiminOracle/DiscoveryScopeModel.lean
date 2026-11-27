import HoiminOracle.ImportRootModel
import HoiminOracle.PathOrderModel

namespace HoiminOracle.DiscoveryScope

abbrev Path := List String

structure Selection where
  sources : List Path := []
  files : List Path := []
  lines : List Path := []
  hasSymbol : Bool := false

structure Scope where
  sources : List Path
  exact : List Path

def prepare (selection : Selection) : Option Scope := do
  if selection.sources.isEmpty && selection.hasSymbol then none else
    let sources ← selection.sources.mapM (ImportRoot.reduce [])
    let exact ← (selection.files ++ selection.lines).mapM (ImportRoot.reduce [])
    if sources.contains [] || (sources.isEmpty && exact.isEmpty) then none else
      some ⟨sources, exact⟩

def spelling (path : Path) : String := String.intercalate "/" path

def strictAncestor (parent child : Path) : Bool :=
  parent.isPrefixOf child && parent.length < child.length

def scopedKeeps (contains : Path → Path → Bool) (scope : Scope) (directory : Bool) (path : Path) : Bool :=
  scope.sources.any (contains · path) ||
    if directory then (scope.sources ++ scope.exact).any (strictAncestor path)
    else scope.exact.contains path

def keeps (scope : Option Scope) (directory : Bool) (path : Path) : Bool :=
  path.isEmpty || scope.all fun active => scopedKeeps List.isPrefixOf active directory path

def ancestors (path : Path) : List Path := (List.range path.length).map (path.take ·)

def reachable (directoryKept : Path → Bool) (path : Path) : Bool :=
  (ancestors path).all directoryKept

def builtInExcluded (path : Path) : Bool :=
  [".git", ".venv", "venv", "env", "__pycache__", ".pytest_cache", ".mypy_cache",
    ".ruff_cache", ".pyre", ".pytype", ".tox", ".nox"].contains (path.getLastD "")

inductive FileKind | python | regular | unsupported

structure File where
  path : Path
  kind : FileKind := .python

structure Record where
  path : String
  isPython : Bool

structure Result where
  scopeActive : Bool
  records : Except String (List Record)
  visited : Nat

def discoverWith (keep : Option Scope → Bool → Path → Bool) (excluded : Path → Bool)
    (selection : Selection) (files : List File) : Result :=
  let active := prepare selection
  let admit directory path := !excluded path && keep active directory path
  let visited := reachable (admit true)
  let dirs := (files.flatMap fun file => ancestors file.path).eraseDups.filter fun path => !path.isEmpty
  let sorted := (files.filter fun file => visited file.path && admit false file.path).mergeSort
    (fun left right => PathOrder.le (spelling left.path) (spelling right.path))
  let records : Except String (List Record) := sorted.mapM fun file =>
    match file.kind with
    | .python => .ok ⟨spelling file.path, true⟩
    | .regular => .ok ⟨spelling file.path, false⟩
    | .unsupported => .error (spelling file.path)
  -- The directory walker always accepts its root before invoking the entry filter.
  ⟨active.isSome, records, (dirs.filter visited).length + (files.filter fun file => visited file.path).length⟩

def discover := discoverWith keeps builtInExcluded

theorem selected_source_keeps_descendant (scope : Scope) (source path : Path) (directory : Bool)
    (selected : source ∈ scope.sources) (below : source.isPrefixOf path = true) :
    keeps (some scope) directory path = true := by
  have h : scope.sources.any (fun src => src.isPrefixOf path) = true :=
    List.any_eq_true.mpr ⟨source, selected, below⟩
  simp [keeps, scopedKeeps, h]

theorem exact_path_keeps_parent (scope : Scope) (parent chosen : Path)
    (selected : chosen ∈ scope.exact) (above : strictAncestor parent chosen = true) :
    keeps (some scope) true parent = true := by
  have h : (scope.sources ++ scope.exact).any (strictAncestor parent) = true :=
    List.any_eq_true.mpr ⟨chosen, List.mem_append_right _ selected, above⟩
  simp only [keeps, Option.all_some, scopedKeeps, if_true, h, Bool.or_true]

theorem pruned_ancestor_blocks_entry (directoryKept : Path → Bool) (path parent : Path)
    (member : parent ∈ ancestors path) (pruned : directoryKept parent = false) :
    reachable directoryKept path = false := by
  cases reached : reachable directoryKept path with
  | false => rfl
  | true =>
    have kept := List.all_eq_true.mp reached parent member
    simp [pruned] at kept

end HoiminOracle.DiscoveryScope
