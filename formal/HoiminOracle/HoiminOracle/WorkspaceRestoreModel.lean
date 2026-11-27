namespace HoiminOracle.WorkspaceRestore

structure FileState where
  content : String
  readOnly : Bool
  deriving BEq, DecidableEq, Repr

inductive Kind where
  | directory
  | file (state : FileState)
  | link
  deriving DecidableEq, Repr

structure Entry where
  path : String
  kind : Kind
  deriving DecidableEq, Repr

structure Snapshot where
  directories : List String
  files : List (String × FileState)

structure Slot where
  path : String
  saved : FileState
  worker : Option FileState

inductive FailurePoint where
  | rewind
  | write
  | permissions
  deriving DecidableEq

structure Failure where
  skip : Nat
  point : FailurePoint

structure Progress where
  files : List (Option FileState) := []
  replaced : List String := []
  failed : Bool := false
  deriving DecidableEq

def failedFile (point : FailurePoint) (slot : Slot) : Option FileState :=
  match point with
  | .rewind => slot.worker
  | .write => some ⟨"", false⟩
  | .permissions => some { slot.saved with readOnly := false }

def restoreFilesWith (fileMatches : Slot → Bool) (fault : Option Failure) : List Slot → Progress
  | [] => {}
  | slot :: rest =>
    if fileMatches slot then
      let next := restoreFilesWith fileMatches fault rest
      { next with files := slot.worker :: next.files }
    else
      match fault with
      | some ⟨0, point⟩ =>
        { files := failedFile point slot :: rest.map Slot.worker,
          replaced := if point = .rewind then [] else [slot.path], failed := true }
      | _ =>
        let next := restoreFilesWith fileMatches (fault.map fun f => { f with skip := f.skip - 1 }) rest
        { next with files := some slot.saved :: next.files, replaced := slot.path :: next.replaced }

def restoreFiles := restoreFilesWith (fun slot => decide (slot.worker = some slot.saved))

def workerFile (tree : List Entry) (path : String) : Option FileState :=
  match tree.find? (fun entry => entry.path == path) with
  | some ⟨_, .file state⟩ => some state
  | _ => none

def fileSlots (saved : Snapshot) (worker : List Entry) : List Slot :=
  saved.files.map fun (path, state) => ⟨path, state, workerFile worker path⟩

def inventory (directories : List String) (slots : List Slot) (result : Progress) : List Entry :=
  directories.map (fun path => ⟨path, .directory⟩) ++
    (slots.zip result.files).filterMap (fun (slot, file) => file.map fun state => ⟨slot.path, .file state⟩)

theorem complete_files (slots : List Slot) :
    (restoreFiles none slots).files = slots.map (fun slot => some slot.saved) := by
  induction slots with
  | nil => rfl
  | cons slot rest ih =>
    by_cases equal : slot.worker = some slot.saved
    · simpa [restoreFiles, restoreFilesWith, equal] using
        congrArg (List.cons (some slot.saved)) ih
    · simpa [restoreFiles, restoreFilesWith, equal] using
        congrArg (List.cons (some slot.saved)) ih

theorem complete_succeeds (slots : List Slot) : (restoreFiles none slots).failed = false := by
  induction slots with
  | nil => rfl
  | cons slot rest ih =>
    by_cases equal : slot.worker = some slot.saved <;>
      simpa [restoreFiles, restoreFilesWith, equal] using ih

theorem complete_replaces_only_changed (slots : List Slot) :
    (restoreFiles none slots).replaced =
      (slots.filter fun slot => decide (slot.worker ≠ some slot.saved)).map Slot.path := by
  induction slots with
  | nil => rfl
  | cons slot rest ih =>
    by_cases equal : slot.worker = some slot.saved
    · simpa [restoreFiles, restoreFilesWith, equal] using ih
    · simpa [restoreFiles, restoreFilesWith, equal] using congrArg (List.cons slot.path) ih

theorem matching_slots_need_no_writes (fault : Option Failure) (slots : List Slot) :
    restoreFiles fault (slots.map fun slot => { slot with worker := some slot.saved }) =
      { files := slots.map (fun slot => some slot.saved) } := by
  induction slots with
  | nil => rfl
  | cons slot rest ih =>
    simp [restoreFiles, restoreFilesWith] at ih ⊢
    simp [ih]

theorem interruption_preserves_remaining (point : FailurePoint) (slot : Slot) (rest : List Slot)
    (changed : slot.worker ≠ some slot.saved) :
    restoreFiles (some ⟨0, point⟩) (slot :: rest) =
      { files := failedFile point slot :: rest.map Slot.worker,
        replaced := if point = .rewind then [] else [slot.path], failed := true } := by
  simp [restoreFiles, restoreFilesWith, changed]

end HoiminOracle.WorkspaceRestore
