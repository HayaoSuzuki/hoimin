import HoiminOracle.SessionWalAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.SessionWalAdmission

open HoiminOracle.SessionAdmission

private structure Case where
  id : String
  seed : Image
  change : Change
  checkpointAfterCommit : Bool

private def cases : List Case :=
  let current : Image := ⟨currentVersion, 0, .canonical⟩
  let fresh : Image := ⟨0, 0, .empty⟩
  let transitions : List (String × Image × Change) :=
    [ ("upgrade-committed-version", { current with version := 99 }, .version currentVersion)
    , ("commit-unsupported-version", current, .version 99)
    , ("replace-unsupported-version", { current with version := 99 }, .version (-1))
    , ("unsupported-becomes-invalid", { current with version := 99 }, .version 0)
    , ("repair-application-id", { current with applicationId := 173 }, .applicationId 0)
    , ("commit-foreign-application-id", current, .applicationId 173)
    , ("repair-fresh-application-id", { fresh with applicationId := 173 }, .applicationId 0)
    , ("repair-fresh-schema", { fresh with objects := .foreign }, .dropForeign)
    , ("add-foreign-object-to-fresh", fresh, .createForeign)
    , ("add-foreign-object-to-current", current, .createForeign)
    , ("other-initializer-committed", fresh, .initialize)
    , ("rollback-version-with-current-schema", current, .version 0) ]
  transitions.flatMap fun (id, seed, change) =>
    [⟨id ++ "-in-wal", seed, change, false⟩, ⟨id ++ "-checkpointed", seed, change, true⟩]

private def stateOf (item : Case) : State :=
  let state := commit (initial item.seed) item.change
  if item.checkpointAfterCommit then checkpoint state else state

private def objectsName : Objects → String
  | .empty => "empty"
  | .canonical => "canonical"
  | .foreign => "foreign"

private def imageFields (image : Image) : List (String × Lean.Json) :=
  [ ("version", Lean.toJson image.version), ("application_id", Lean.toJson image.applicationId)
  , ("objects", .str (objectsName image.objects)) ]

private def changeJson : Change → Lean.Json
  | .version value => Lean.Json.mkObj [("kind", .str "version"), ("value", Lean.toJson value)]
  | .applicationId value => Lean.Json.mkObj [("kind", .str "application_id"), ("value", Lean.toJson value)]
  | .createForeign => Lean.Json.mkObj [("kind", .str "create_foreign")]
  | .dropForeign => Lean.Json.mkObj [("kind", .str "drop_foreign")]
  | .initialize => Lean.Json.mkObj [("kind", .str "initialize")]

private def resultJson : Option Failure → Lean.Json
  | none => Lean.Json.mkObj [("kind", .str "admitted")]
  | some (.unsupportedVersion version) =>
    Lean.Json.mkObj [("kind", .str "unsupported_version"), ("version", Lean.toJson version)]
  | some .invalidIdentity => Lean.Json.mkObj [("kind", .str "invalid_identity")]
  | some _ => Lean.Json.mkObj [("kind", .str "artifact_error")]

def corpus : String :=
  String.join (cases.map fun item =>
    let state := stateOf item
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("seed", Lean.Json.mkObj (("kind", .str "sqlite") :: imageFields item.seed))
      , ("change", changeJson item.change), ("checkpoint", .bool item.checkpointAfterCommit)
      , ("checkpointed", Lean.Json.mkObj (imageFields state.checkpointed))
      , ("committed", Lean.Json.mkObj (imageFields state.committed))
      , ("advisory_result", resultJson (advisoryCheck state))
      , ("committed_result", resultJson (committedCheck state))
      , ("opener_result", resultJson (openCheck state)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Option Failure)) :=
    [ ("skip_writable_recheck", fun item => advisoryCheck (stateOf item))
    , ("use_only_committed_view", fun item => committedCheck (stateOf item))
    , ("omit_checkpoint", fun item => openCheck (commit (initial item.seed) item.change)) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => variant item != openCheck (stateOf item)) then
      IO.println s!"session WAL admission sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session WAL admission missed variant: {name}"
      return 1
  let disagreements := cases.filter fun item => committedCheck (stateOf item) != openCheck (stateOf item)
  IO.println s!"session_wal_admission_cases={cases.length} committed_view_disagreements={disagreements.length}"
  return 0

end HoiminOracle.SessionWalAdmission
