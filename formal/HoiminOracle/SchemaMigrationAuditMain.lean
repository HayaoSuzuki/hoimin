import HoiminOracle.SchemaMigrationCases
import Lean.Data.Json

open HoiminOracle.SchemaMigration

namespace HoiminOracle.SchemaMigration.Executable

private def versionName : Version → String
  | .fresh => "fresh"
  | .v1 => "v1"
  | .v2 => "v2"
  | .current => "current"
  | .future => "future"

private def phaseName : Phase → String
  | .ready => "not_run"
  | .waiting _ | .locked => "incomplete"
  | .succeeded => "ok"
  | .futureRejected => "future_version"
  | .migrationFailed => "database_error"

private def actorName : Actor → String
  | .left => "left"
  | .right => "right"

private def eventName : Event → String
  | .observe actor => s!"observe:{actorName actor}"
  | .begin actor => s!"begin:{actorName actor}"
  | .migrate actor => s!"migrate:{actorName actor}"
  | .commit actor => s!"commit:{actorName actor}"
  | .fail actor => s!"fail:{actorName actor}"

private def stringsJson (items : List String) : Lean.Json :=
  .arr (items.toArray.map Lean.Json.str)

private def caseJson (item : OracleCase) : Lean.Json :=
  let result := caseResult item
  Lean.Json.mkObj [
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

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

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
  return if finiteAuditPasses && fixedCasesPass then 0 else 2

private def printSensitivity : IO UInt32 := do
  IO.println s!"atomicity_detected={atomicitySensitivity}"
  IO.println s!"stale_reread_detected={staleReadSensitivity}"
  IO.println s!"future_precedence_detected={futurePrecedenceSensitivity}"
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
