import HoiminOracle.SchemaMigrationCases
import Lean.Data.Json

open HoiminOracle.SchemaMigration

namespace HoiminOracle.SchemaMigration.Executable

private def versionName : Version → String
  | .fresh => "fresh"
  | .v1 => "v1"
  | .v2 => "v2"
  | .v3 => "v3"
  | .v4 => "v4"
  | .current => "current"
  | .future => "future"

private def phaseName : Phase → String
  | .ready => "not_run"
  | .waiting _ | .locked => "incomplete"
  | .succeeded => "ok"
  | .unsupportedRejected => "unsupported_version"
  | .initializationFailed => "database_error"

private def actorName : Actor → String
  | .left => "left"
  | .right => "right"

private def eventName : Event → String
  | .observe actor => s!"observe:{actorName actor}"
  | .begin actor => s!"begin:{actorName actor}"
  | .initializeSchema actor => s!"initialize:{actorName actor}"
  | .commit actor => s!"commit:{actorName actor}"
  | .fail actor => s!"fail:{actorName actor}"

private def stringsJson (items : List String) : Lean.Json :=
  .arr (items.toArray.map Lean.Json.str)

private def writerVersions (item : OracleCase) : List String :=
  let before := initial item.initialVersion item.legacyRow
  let states := item.trace.scanl step before
  (item.trace.zip (states.drop 1)).filterMap fun (event, state) =>
    match event with
    | .begin _ => state.txVersion.map versionName
    | _ => none

private def caseJson (item : OracleCase) : Lean.Json :=
  let result := caseResult item
  let fields := [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("initial_version", .str (versionName item.initialVersion)),
    ("initial_legacy_row", Lean.toJson item.legacyRow),
    ("trace", stringsJson (item.trace.map eventName)),
    ("expected_version", .str (versionName result.version)),
    ("expected_legacy_row", Lean.toJson result.legacyRow),
    ("expected_open_results", stringsJson [
      phaseName result.leftPhase, phaseName result.rightPhase
    ])
  ]
  Lean.Json.mkObj (if item.mode == "internal-fixture" then
    fields ++ [("expected_writer_versions", stringsJson (writerVersions item))]
    else fields)

private def snapshotJson (version : Version) (applicationId : Int) (catalog : Catalog) : Lean.Json :=
  let admission := admitSnapshot version applicationId catalog
  let versionLabel := versionName version
  let catalogLabel := match catalog with
    | .empty => "empty"
    | .canonical => "canonical"
    | .foreign => "foreign"
  let result := match admission with
    | .initialize | .reopen => "ok"
    | .invalidIdentity => "invalid_identity"
    | .unsupportedVersion => "unsupported_version"
  let storedVersion := if admission == .initialize then .current else version
  let events := if admission == .initialize then
    ["observe:left", "begin:left", "initialize:left", "commit:left"] else ["observe:left"]
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)),
    ("id", .str s!"snapshot-{versionLabel}-{applicationId}-{catalogLabel}"),
    ("mode", .str "strict"), ("scenario", .str "admission_snapshot"),
    ("initial_version", .str versionLabel), ("expected_version", .str (versionName storedVersion)),
    ("initial_legacy_row", Lean.toJson (catalog != .empty)),
    ("expected_legacy_row", Lean.toJson (catalog != .empty)),
    ("trace", stringsJson events), ("expected_open_results", stringsJson [result, "not_run"]),
    ("snapshot", Lean.Json.mkObj [("application_id", Lean.toJson applicationId),
      ("catalog", .str catalogLabel)])
  ]

def renderCorpus : String :=
  let snapshots := snapshotInputs.map fun (version, identity, catalog) =>
    snapshotJson version identity catalog
  String.join ((cases.map caseJson ++ snapshots).map fun item => item.compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "schema migration audit did not distinguish every broken family"
    return .error 2
  unless fixedCasesPass do
    IO.eprintln "schema migration fixed cases violate the modeled contract"
    return .error 2
  unless finiteAuditPasses do
    IO.eprintln "schema migration bounded state audit found a counterexample or exceeded its ceiling"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      IO.FS.writeFile path renderCorpus
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then return 0
        IO.eprintln s!"stale Lean schema migration corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean schema migration corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  IO.println s!"actors={actors.length}"
  IO.println s!"events={events.length}"
  IO.println s!"depth={auditDepth}"
  IO.println s!"states={auditedStates.length}"
  IO.println s!"state_ceiling={stateCeiling}"
  IO.println s!"cases={cases.length}"
  IO.println s!"snapshot_cases={snapshotInputs.length}"
  return if finiteAuditPasses && fixedCasesPass then 0 else 2

private def printSensitivity : IO UInt32 := do
  IO.println s!"atomicity_detected={atomicitySensitivity}"
  IO.println s!"stale_reread_detected={staleReadSensitivity}"
  IO.println s!"unsupported_precedence_detected={unsupportedPrecedenceSensitivity}"
  IO.println s!"schema_separator_detected={schemaSeparatorSensitivity}"
  IO.println s!"snapshot_identity_detected={snapshotIdentitySensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if fixedCasesPass then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | _ => do
      IO.eprintln "usage: generate_schema_migration --output PATH | --check PATH | --stats | --sensitivity | --cases"
      return 2

end HoiminOracle.SchemaMigration.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.SchemaMigration.Executable.main args
