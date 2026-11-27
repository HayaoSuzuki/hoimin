import HoiminOracle.PathOrderModel

namespace HoiminOracle.WorkspaceIntegrity

inductive EntryKind
  | file (content : String)
  | directory
  deriving DecidableEq, BEq

structure Entry where
  path : String
  kind : EntryKind
  deriving DecidableEq, BEq

abbrev Tree := List Entry

def containsTree (root path : String) : Bool :=
  path == root || path.startsWith (root ++ "/")

def included (path : String) : Bool :=
  path != "ignored.txt" && !containsTree "hidden-fixture" path

private def ancestors (path : String) : List String :=
  let parts := path.splitOn "/"
  (List.range' 1 (parts.length - 1)).map (fun length => String.intercalate "/" (parts.take length))

structure Manifest where
  directories : List String
  files : List (String × String)
  deriving DecidableEq

def snapshot (tree : Tree) : Manifest :=
  let selected := tree.filter (included ∘ Entry.path)
  let directories := selected.flatMap fun entry =>
    match entry.kind with
    | .directory => entry.path :: ancestors entry.path
    | .file _ => ancestors entry.path
  let files := selected.filterMap fun entry =>
    match entry.kind with
    | .file content => some (entry.path, content)
    | .directory => none
  ⟨directories.eraseDups.mergeSort PathOrder.le,
    files.mergeSort (fun left right => PathOrder.le left.1 right.1)⟩

def firstDirectoryDifference (saved current : Manifest) : Option String :=
  (((saved.directories ++ current.directories).eraseDups).mergeSort PathOrder.le).find?
    (fun path => saved.directories.contains path != current.directories.contains path)

private def contents (files : List (String × String)) (path : String) : Option String :=
  (files.find? (·.1 == path)).map Prod.snd

def firstSavedFileDifference (saved current : Manifest) : Option String :=
  (saved.files.find? fun (path, content) => contents current.files path != some content).map Prod.fst

def firstNewFile (saved current : Manifest) : Option String :=
  (current.files.find? fun (path, _) => (contents saved.files path).isNone).map Prod.fst

def firstDifference (saved current : Manifest) : Option String :=
  (firstDirectoryDifference saved current).orElse fun _ =>
    (firstSavedFileDifference saved current).orElse fun _ =>
      firstNewFile saved current

inductive Edit
  | write (path content : String)
  | makeDirectory (path : String)
  | remove (path : String)

def applyEdit (tree : Tree) : Edit → Tree
  | .remove path => tree.filter fun item => !containsTree path item.path
  | .write path content => ⟨path, .file content⟩ :: tree.filter (·.path != path)
  | .makeDirectory path => ⟨path, .directory⟩ :: tree.filter (·.path != path)

def applyEdits (tree : Tree) (edits : List Edit) : Tree := edits.foldl applyEdit tree

structure Run where
  postCopy : List Edit := []
  baseline : List Edit := []
  mutant : List Edit := []
  baselineExit : Nat := 0
  mutantExit : Nat := 0

structure Observation where
  baselineExit : Option Nat
  mutantStatus : Option String
  changed : Option String
  exitCode : Nat
  deriving DecidableEq

def runWith (compare : Manifest → Manifest → Option String) (savedTree : Tree) (input : Run) : Observation :=
  let saved := snapshot savedTree
  let copied := applyEdits savedTree input.postCopy
  match compare saved (snapshot copied) with
  | some path => ⟨none, none, some path, 2⟩
  | none =>
    let afterBaseline := applyEdits copied input.baseline
    let succeeded := input.baselineExit == 0
    let finalTree := if succeeded then applyEdits afterBaseline input.mutant else afterBaseline
    let status := if succeeded then some (if input.mutantExit == 0 then "survived" else "killed") else none
    let changed := compare saved (snapshot finalTree)
    ⟨some input.baselineExit, status, changed, if changed.isSome then 2 else if succeeded then 0 else 3⟩

def run := runWith firstDifference

theorem directory_difference_takes_priority (saved current : Manifest) (path : String)
    (diff : firstDirectoryDifference saved current = some path) :
    firstDifference saved current = some path := by
  simp [firstDifference, diff]

theorem saved_file_difference_precedes_any_added_file (saved current : Manifest) (path : String)
    (dirs : firstDirectoryDifference saved current = none)
    (diff : firstSavedFileDifference saved current = some path) :
    firstDifference saved current = some path := by
  simp [firstDifference, dirs, diff]

theorem excluded_edit_leaves_selected_inputs_unchanged (tree : Tree) (path content : String)
    (excluded : included path = false) :
    snapshot (applyEdit tree (.write path content)) = snapshot tree := by
  have unchanged : (tree.filter (fun item => item.path != path)).filter (included ∘ Entry.path) =
      tree.filter (included ∘ Entry.path) := by
    induction tree with
    | nil => rfl
    | cons item rest ih =>
      by_cases same : item.path = path
      · simp [same, excluded, ih]
      · simp [List.filter_cons, same, ih]
  simp [snapshot, applyEdit, excluded, unchanged]

theorem changed_original_before_baseline_stops_processing (saved : Tree) (input : Run) (path : String)
    (changed : firstDifference (snapshot saved) (snapshot (applyEdits saved input.postCopy)) = some path) :
    run saved input = ⟨none, none, some path, 2⟩ := by
  simp [run, runWith, changed]

theorem final_change_keeps_failed_baseline_observation (saved : Tree) (input : Run) (path : String)
    (copied : firstDifference (snapshot saved) (snapshot (applyEdits saved input.postCopy)) = none)
    (failed : input.baselineExit ≠ 0)
    (changed : firstDifference (snapshot saved)
      (snapshot (applyEdits (applyEdits saved input.postCopy) input.baseline)) = some path) :
    run saved input = ⟨some input.baselineExit, none, some path, 2⟩ := by
  simp [run, runWith, copied, failed, changed]

theorem final_change_keeps_finished_mutant_observation (saved : Tree) (input : Run) (path : String)
    (copied : firstDifference (snapshot saved) (snapshot (applyEdits saved input.postCopy)) = none)
    (passed : input.baselineExit = 0)
    (changed : firstDifference (snapshot saved)
      (snapshot (applyEdits (applyEdits (applyEdits saved input.postCopy) input.baseline)
        input.mutant)) = some path) :
    run saved input = ⟨some 0, some (if input.mutantExit == 0 then "survived" else "killed"), some path, 2⟩ := by
  simp [run, runWith, copied, passed, changed]

end HoiminOracle.WorkspaceIntegrity
