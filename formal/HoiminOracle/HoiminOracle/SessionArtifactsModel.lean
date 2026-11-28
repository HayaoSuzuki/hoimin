import HoiminOracle.WorkspaceSelectionModel

namespace HoiminOracle.SessionArtifacts

open HoiminOracle.WorkspaceSelection

structure DatabasePath where
  parent : Path
  name : String
  deriving DecidableEq

def DatabasePath.path (database : DatabasePath) : Path :=
  database.parent ++ [database.name]

def DatabasePath.files (database : DatabasePath) : List Path :=
  ["", "-wal", "-shm", "-journal"].map (fun suffix => database.parent ++ [database.name ++ suffix])

def DatabasePath.lexicalLock (database : DatabasePath) : Path :=
  database.parent ++ ["." ++ database.name ++ ".hoimin-locks"]

inductive LockResolution where
  | missing
  | existing (path : Path)
  | failed

inductive SessionResolution where
  | disabled
  | databaseFailed
  | resolved (database : DatabasePath) (lock : LockResolution)

structure Prepared where
  database : Option Path
  exclusions : List Literal

def relativeInside (foldCase : Bool) (root path : Path) : Option Path :=
  if isPrefix foldCase root path then some (path.drop root.length) else none

def lockPaths (database : DatabasePath) (lock : LockResolution) : Option (List Path) :=
  match lock with
  | .failed => none
  | .missing => some [database.lexicalLock]
  | .existing path =>
    some (if path = database.lexicalLock then [path] else [database.lexicalLock, path])

def projectExclusions (relative : Path → Option Path) (paths : List Path) (kind : Path → Literal) :
    List Literal :=
  paths.filterMap (fun path => (relative path).map kind)

def prepare (foldCase : Bool) (root : Path) : SessionResolution → Option Prepared
  | .disabled => some ⟨none, []⟩
  | .databaseFailed => none
  | .resolved database lock => (lockPaths database lock).map fun trees =>
    let inside := relativeInside foldCase root
    ⟨some database.path, projectExclusions inside database.files .file ++ projectExclusions inside trees .tree⟩

theorem proper_siblings_are_outside (foldCase : Bool) (root path : Path)
    (outside : isPrefix foldCase root path = false) :
    relativeInside foldCase root path = none := by simp [relativeInside, outside]

theorem failed_lock_resolution_rejects_preparation (foldCase : Bool) (root : Path)
    (database : DatabasePath) :
    prepare foldCase root (.resolved database .failed) = none := rfl

theorem lexical_lock_is_reserved_when_missing (foldCase : Bool) (root : Path)
    (database : DatabasePath) (relative : Path)
    (inside : relativeInside foldCase root database.lexicalLock = some relative) :
    ∃ prepared, prepare foldCase root (.resolved database .missing) = some prepared ∧
      .tree relative ∈ prepared.exclusions := by
  simp [prepare, lockPaths, projectExclusions, inside]

theorem database_files_are_reserved_without_an_existence_check (foldCase : Bool) (root path : Path)
    (database : DatabasePath) (lock : LockResolution) (trees : List Path) (relative : Path)
    (resolves : lockPaths database lock = some trees) (owned : path ∈ database.files)
    (inside : relativeInside foldCase root path = some relative) :
    ∃ prepared, prepare foldCase root (.resolved database lock) = some prepared ∧
      .file relative ∈ prepared.exclusions := by
  simp only [prepare, resolves, Option.map_some, Option.some.injEq, exists_eq_left']
  apply List.mem_append_left
  apply List.mem_filterMap.mpr
  exact ⟨path, owned, by simp [inside]⟩

theorem canonical_lock_is_reserved_across_the_root_boundary (foldCase : Bool) (root path : Path)
    (database : DatabasePath) (relative : Path)
    (lexicalOutside : relativeInside foldCase root database.lexicalLock = none)
    (canonicalInside : relativeInside foldCase root path = some relative) :
    ∃ prepared, prepare foldCase root (.resolved database (.existing path)) = some prepared ∧
      .tree relative ∈ prepared.exclusions := by
  have distinct : path ≠ database.lexicalLock := by
    intro equal
    rw [equal, lexicalOutside] at canonicalInside
    contradiction
  simp [prepare, lockPaths, distinct, projectExclusions, lexicalOutside, canonicalInside]

theorem reserved_lock_descendant_is_never_selected (activeRules : Rules) (entry : Entry) (tree : Path)
    (reservation : .tree tree ∈ activeRules.literals)
    (beneath : isPrefix activeRules.caseInsensitive tree entry.path = true)
    (proper : tree.length < entry.path.length) :
    selected activeRules entry = false := by
  apply literals_apply_to_both_walks
  apply List.any_eq_true.mpr
  exact ⟨.tree tree, reservation, by simp [literalExcluded, beneath, proper]⟩

end HoiminOracle.SessionArtifacts
