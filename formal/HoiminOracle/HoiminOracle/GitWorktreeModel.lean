import HoiminOracle.GitPatchModel
import HoiminOracle.GitPhysicalRowsModel

namespace HoiminOracle.GitWorktree

open HoiminOracle.GitPatch (isPython)
open HoiminOracle.GitPhysicalRows (Span physicalCount)

structure Target where
  path : String
  lines : List Span := []

abbrev Scope := Option (List Target)
abbrev Changes := List (String × Span)

inductive InventoryEntry
  | path (name : String)
  | invalidUtf8
  | padding

inductive FileEntry
  | file (bytes : List UInt8)
  | directory
  | blockedParent

abbrev Files := List (String × FileEntry)

inductive Diagnostic
  | invalidUtf8
  | literalBackslash (path : String)
  | rootUnavailable
  | readParent (path : String)
  deriving BEq

private def inScope (scope : Scope) (path : String) : Bool :=
  scope.all fun targets => targets.any (·.path == path)

def admit (scope : Scope) : InventoryEntry → Except Diagnostic (Option String)
  | .invalidUtf8 => .error .invalidUtf8
  | .padding => .ok none
  | .path path =>
    if path.contains '\\' then
      if scope.isSome then .ok none else .error (.literalBackslash path)
    else .ok (if inScope scope path && isPython path then some path else none)

private def hasPositiveLine (target : Target) : Bool :=
  target.lines.isEmpty || target.lines.any fun span => max 1 span.start ≤ span.finish

/-- The scoped read order is the order of the resolved targets. -/
def selectPaths (scope : Scope) (inventory : List InventoryEntry) : Except Diagnostic (List String) := do
  let admitted := (← inventory.mapM (admit scope)).filterMap id
  return match scope with
  | none => admitted
  | some targets => (targets.filter fun target => admitted.contains target.path && hasPositiveLine target).map (·.path)

def contribution (files : Files) (path : String) : Except Diagnostic (Option Span) :=
  match files.find? (·.1 == path) with
  | some (_, .file bytes) =>
    .ok (if bytes.isEmpty || bytes.contains 0 then none else some ⟨1, physicalCount bytes⟩)
  | some (_, .blockedParent) => .error (.readParent path)
  | some (_, .directory) | none => .ok none

def collect (files : Files) (paths : List String) : Except Diagnostic Changes := do
  let observations ← paths.mapM fun path => (contribution files path).map (·.map (path, ·))
  return observations.filterMap id

def scan (rootAvailable : Bool) (scope : Scope) (inventory : List InventoryEntry) (files : Files) :
    Except Diagnostic Changes := do
  let paths ← selectPaths scope inventory
  if scope.any List.isEmpty then return []
  if !rootAvailable then throw .rootUnavailable
  collect files paths

structure Observation where
  changed : Changes
  failure : Option Diagnostic

def publish (initial : Changes) : Except Diagnostic Changes → Observation
  | .ok added => ⟨initial ++ added, none⟩
  | .error failure => ⟨initial, some failure⟩

theorem empty_scope_still_validates (rootAvailable : Bool) (inventory : List InventoryEntry)
    (files : Files) : scan rootAvailable (some []) inventory files =
      (selectPaths (some []) inventory).map (fun _ => []) := by
  unfold scan
  cases selectPaths (some []) inventory <;> rfl

theorem collect_append (files : Files) (first second : List String) :
    collect files (first ++ second) = (do
      let preceding ← collect files first
      let following ← collect files second
      return preceding ++ following) := by
  unfold collect
  rw [List.mapM_append]
  cases first.mapM (fun path => (contribution files path).map (·.map (path, ·))) <;>
    cases second.mapM (fun path => (contribution files path).map (·.map (path, ·))) <;>
    simp [bind, pure, Except.bind, Except.pure]

theorem late_read_failure_is_atomic (initial : Changes) (files : Files) (first rest : List String)
    (path : String) (added : Changes) (reason : Diagnostic)
    (succeeded : collect files first = .ok added) (failed : contribution files path = .error reason) :
    publish initial (collect files (first ++ path :: rest)) = ⟨initial, some reason⟩ := by
  have laterFailure : collect files (path :: rest) = .error reason := by
    simp [collect, failed, bind, Except.bind, Except.map]
  simp [collect_append, succeeded, laterFailure, publish, bind, Except.bind]

theorem regular_file_contributes_whole_range (files : Files) (path : String) (bytes : List UInt8)
    (existsFile : files.find? (·.1 == path) = some (path, .file bytes))
    (nonempty : bytes ≠ []) (noNul : 0 ∉ bytes) :
    contribution files path = .ok (some ⟨1, physicalCount bytes⟩) := by
  simp [contribution, existsFile, nonempty, noNul]

end HoiminOracle.GitWorktree
