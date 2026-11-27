import HoiminOracle.SessionAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.SessionAdmission

private inductive Journal where
  | rollback | wal
  deriving DecidableEq, BEq

private def Journal.name : Journal → String
  | .rollback => "rollback"
  | .wal => "wal"

private def Companion.name : Companion → String
  | .missing => "missing"
  | .regular => "regular"
  | .shared => "shared"
  | .directory => "directory"
  | .symlink => "symlink"

private def Objects.name : Objects → String
  | .empty => "empty"
  | .canonical => "canonical"
  | .foreign => "foreign"

private structure Case where
  id : String
  database : Database
  companions : Companions := {}
  journal : Journal := .rollback
  unixOnly : Bool := false

private def put (files : Companions) (slot : Slot) (kind : Companion) : Companions :=
  match slot with
  | .journal => { files with journal := kind }
  | .wal => { files with wal := kind }
  | .shm => { files with shm := kind }

private def current := Database.sqlite ⟨currentVersion, 0, .canonical⟩

private def imageMatrix : List Case :=
  ([.rollback, .wal] : List Journal).flatMap fun journal =>
    ([.empty, .canonical, .foreign] : List Objects).flatMap fun objects =>
      [0, currentVersion, -1, 99].flatMap fun version =>
        [0, 173].map fun appId => {
          id := s!"{journal.name}-{objects.name}-version-{version}-app-{appId}"
          database := .sqlite ⟨version, appId, objects⟩
          journal
        }

private def badCompanions : List Case :=
  inspectionOrder.flatMap fun slot =>
    ([.directory, .shared, .symlink] : List Companion).flatMap fun kind =>
      let databases :=
        [("missing", .missing), ("current", current), ("shared", .shared),
          ("unsupported", Database.sqlite ⟨99, 173, .foreign⟩)]
      databases.map fun (name, database) => {
        id := s!"{slot.name}-{kind.name}-with-{name}"
        database
        companions := put {} slot kind
        unixOnly := kind == .symlink
      }

private def precedence : List Case :=
  ([ (.journal, .wal), (.wal, .shm), (.journal, .shm) ] : List (Slot × Slot)).flatMap fun (first, second) =>
    [false, true].map fun firstShared =>
      let kind := if firstShared then Companion.shared else .directory
      let other := if firstShared then Companion.directory else .shared
      { id := s!"{first.name}-{kind.name}-before-{second.name}-{other.name}",
        database := .malformed, companions := put (put {} first kind) second other }

private def ordinary : List Case :=
  [⟨"no-database", .missing, {}, .rollback, false⟩,
    ⟨"zero-byte", .zeroByte, {}, .rollback, false⟩,
    ⟨"nonregular-database", .directory, {}, .rollback, false⟩,
    ⟨"shared-database", .shared, {}, .rollback, false⟩,
    ⟨"malformed-database", .malformed, {}, .rollback, false⟩] ++
  inspectionOrder.map fun slot => {
    id := s!"regular-{slot.name}-allows-wal-reopen", database := current,
    companions := put {} slot .regular, journal := .wal }

private def cases := ordinary ++ imageMatrix ++ badCompanions ++ precedence

private def databaseJson : Database → Lean.Json
  | .missing => Lean.Json.mkObj [("kind", .str "missing")]
  | .zeroByte => Lean.Json.mkObj [("kind", .str "zero_byte")]
  | .directory => Lean.Json.mkObj [("kind", .str "directory")]
  | .shared => Lean.Json.mkObj [("kind", .str "shared")]
  | .malformed => Lean.Json.mkObj [("kind", .str "malformed")]
  | .sqlite image => Lean.Json.mkObj
    [ ("kind", .str "sqlite"), ("version", Lean.toJson image.version)
    , ("application_id", Lean.toJson image.applicationId), ("objects", .str image.objects.name) ]

private def reasonJson : Failure → Lean.Json
  | .nonregularCompanion slot => Lean.Json.mkObj
    [("kind", .str "nonregular_companion"), ("slot", .str slot.name)]
  | .nonregularDatabase => Lean.Json.mkObj [("kind", .str "nonregular_database")]
  | .sharedFile => Lean.Json.mkObj [("kind", .str "shared_file")]
  | .malformed => Lean.Json.mkObj [("kind", .str "malformed")]
  | .unsupportedVersion version => Lean.Json.mkObj
    [("kind", .str "unsupported_version"), ("version", Lean.toJson version)]
  | .invalidIdentity => Lean.Json.mkObj [("kind", .str "invalid_identity")]

private def outcome (item : Case) : Lean.Json :=
  match rejectReason item.companions item.database with
  | some failure => Lean.Json.mkObj [("kind", .str "rejected"), ("reason", reasonJson failure)]
  | none =>
    let rows : Nat := match item.database with
      | .sqlite image => if image.objects == .canonical then 1 else 0
      | _ => 0
    Lean.Json.mkObj [("kind", .str "opened"), ("version", Lean.toJson currentVersion),
      ("foreign_keys", .bool true), ("journal", .str "wal"), ("fingerprint_rows", Lean.toJson rows)]

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("unix_only", .bool item.unixOnly), ("database", databaseJson item.database)
      , ("journal", .str item.journal.name)
      , ("companions", Lean.Json.mkObj
          (inspectionOrder.map fun slot => (slot.name, .str (item.companions.get slot).name)))
      , ("expected", outcome item) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Option Failure)) :=
    [ ("ignore_companions", fun item => inspectDatabase item.database)
    , ("inspect_database_first", fun item =>
        (inspectDatabase item.database).orElse fun _ => inspectCompanions inspectionOrder item.companions)
    , ("reverse_companion_order", fun item =>
        (inspectCompanions inspectionOrder.reverse item.companions).orElse fun _ => inspectDatabase item.database)
    , ("accept_shared_files", fun item =>
        (rejectReason item.companions item.database).filter (· != .sharedFile))
    , ("ignore_application_id", fun item =>
        let database := match item.database with
          | .sqlite image => .sqlite { image with applicationId := 0 }
          | other => other
        rejectReason item.companions database)
    , ("accept_version_without_schema_identity", fun item =>
        let database := match item.database with
          | .sqlite image => .sqlite { image with objects := if image.version = 0 then .empty else .canonical }
          | other => other
        rejectReason item.companions database)
    , ("check_identity_before_unsupported_version", fun item =>
        match item.database with
        | .sqlite image =>
          if image.applicationId ≠ 0 && (inspectCompanions inspectionOrder item.companions).isNone then
            some .invalidIdentity
          else rejectReason item.companions item.database
        | _ => rejectReason item.companions item.database) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => variant item != rejectReason item.companions item.database) then
      IO.println s!"session admission sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session admission missed variant: {name}"
      return 1
  IO.println s!"session_admission_cases={cases.length} unix_only={(cases.filter Case.unixOnly).length}"
  return 0

end HoiminOracle.SessionAdmission
