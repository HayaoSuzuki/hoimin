namespace HoiminOracle.SessionAdmission

inductive Slot where
  | journal | wal | shm
  deriving DecidableEq, BEq, Repr

def Slot.name : Slot → String
  | .journal => "journal"
  | .wal => "wal"
  | .shm => "shm"

inductive Companion where
  | missing | regular | shared | directory | symlink
  deriving DecidableEq, BEq, Repr

structure Companions where
  journal : Companion := .missing
  wal : Companion := .missing
  shm : Companion := .missing

def inspectionOrder : List Slot := [.journal, .wal, .shm]

def Companions.get (files : Companions) : Slot → Companion
  | .journal => files.journal
  | .wal => files.wal
  | .shm => files.shm

def currentVersion : Int := 5

inductive Objects where
  | empty | canonical | foreign
  deriving DecidableEq, BEq

structure Image where
  version : Int
  applicationId : Int
  objects : Objects

inductive Database where
  | missing | zeroByte | directory | shared | malformed
  | sqlite (image : Image)

inductive Failure where
  | nonregularCompanion (slot : Slot)
  | nonregularDatabase
  | sharedFile
  | malformed
  | unsupportedVersion (version : Int)
  | invalidIdentity
  deriving DecidableEq, BEq, Repr

def companionFailure (slot : Slot) : Companion → Option Failure
  | .missing | .regular => none
  | .shared => some .sharedFile
  | .directory | .symlink => some (.nonregularCompanion slot)

def inspectCompanions (order : List Slot) (companions : Companions) : Option Failure :=
  order.findSome? fun slot => companionFailure slot (companions.get slot)

def admitImage (image : Image) : Option Failure :=
  if image.version ≠ 0 ∧ image.version ≠ currentVersion then
    some (.unsupportedVersion image.version)
  else if image.applicationId ≠ 0 then some .invalidIdentity
  else if (image.version = 0 ∧ image.objects = .empty) ∨
    (image.version = currentVersion ∧ image.objects = .canonical) then none
  else some .invalidIdentity

def inspectDatabase : Database → Option Failure
  | .missing | .zeroByte => none
  | .directory => some .nonregularDatabase
  | .shared => some .sharedFile
  | .malformed => some .malformed
  | .sqlite image => admitImage image

def rejectReason (companions : Companions) (database : Database) : Option Failure :=
  (inspectCompanions inspectionOrder companions).orElse fun _ => inspectDatabase database

def admitted (image : Image) : Prop :=
  image.applicationId = 0 ∧
    ((image.version = 0 ∧ image.objects = .empty) ∨
      (image.version = currentVersion ∧ image.objects = .canonical))

theorem admitted_image_iff (image : Image) : admitImage image = none ↔ admitted image := by
  simp only [admitImage, admitted]
  split
  · rename_i unsupported
    simp_all
  · split
    · rename_i foreign
      simp_all
    · rename_i applicationId
      have identity : image.applicationId = 0 := Classical.not_not.mp applicationId
      by_cases schema : (image.version = 0 ∧ image.objects = .empty) ∨
          (image.version = currentVersion ∧ image.objects = .canonical)
      <;> simp [identity, schema]

theorem companion_failure_precedes_database (companions : Companions) (database : Database)
    (failure : Failure) (first : inspectCompanions inspectionOrder companions = some failure) :
    rejectReason companions database = some failure := by
  simp [rejectReason, first]

theorem earlier_companion_failure_wins (companions : Companions) (before after : List Slot)
    (slot : Slot) (failure : Failure)
    (earlier : inspectCompanions before companions = none)
    (rejected : companionFailure slot (companions.get slot) = some failure) :
    inspectCompanions (before ++ slot :: after) companions = some failure := by
  induction before with
  | nil => simp [inspectCompanions, rejected]
  | cons head tail ih =>
    cases first : companionFailure head (companions.get head) with
    | some found => simp [inspectCompanions, first] at earlier
    | none =>
      have rest : inspectCompanions tail companions = none := by
        simpa only [inspectCompanions, List.findSome?_cons, first] using earlier
      simpa [inspectCompanions, first] using ih rest

theorem unsupported_version_precedes_identity (image : Image)
    (nonzero : image.version ≠ 0) (different : image.version ≠ currentVersion) :
    admitImage image = some (.unsupportedVersion image.version) := by
  simp [admitImage, nonzero, different]

end HoiminOracle.SessionAdmission
