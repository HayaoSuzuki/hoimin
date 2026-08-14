import HoiminOracle.OutputRetentionCases
import Lean.Data.Json

open HoiminOracle.OutputRetention

namespace HoiminOracle.OutputRetention.Executable

private def bytesJson (bytes : List ByteValue) : Lean.Json :=
  .arr (bytes.toArray.map Lean.toJson)

private def chunksJson (chunks : List Chunk) : Lean.Json :=
  .arr (chunks.toArray.map bytesJson)

private def caseJson (item : OracleCase) : Lean.Json :=
  let result := caseResult item
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("capacity", Lean.toJson item.capacity),
    ("chunks", chunksJson item.chunks),
    ("observed_seed", Lean.toJson item.observedSeed),
    ("arithmetic_increment", Lean.toJson item.arithmeticIncrement),
    ("error_code", Lean.toJson item.errorCode),
    ("expected_observed", Lean.toJson result.observed),
    ("expected_retained", Lean.toJson result.retained),
    ("expected_position", Lean.toJson result.position),
    ("expected_drained_chunks", Lean.toJson result.drainedChunks),
    ("expected_bytes", bytesJson result.bytes),
    ("expected_error_code", Lean.toJson result.errorCode) ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "output retention audit missed a broken family"
    return .error 2
  unless cases.all caseSafe do
    IO.eprintln "output retention cases violate the closed contract"
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
        IO.eprintln s!"stale output retention corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check output retention corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"oldest_retention_detected={oldestSensitivity}"
  IO.println s!"retained_advance_detected={retainedAdvanceSensitivity}"
  IO.println s!"oversized_chunk_detected={oversizedChunkSensitivity}"
  IO.println s!"wrap_skip_detected={wrapSkipSensitivity}"
  IO.println s!"wrap_duplicate_detected={wrapDuplicateSensitivity}"
  IO.println s!"marker_threshold_detected={markerThresholdSensitivity}"
  IO.println s!"marker_tail_detected={markerTailSensitivity}"
  IO.println s!"retained_from_writes_detected={retainedWrittenSensitivity}"
  IO.println s!"observed_saturation_detected={observedSaturationSensitivity}"
  IO.println s!"stop_drain_detected={stopDrainSensitivity}"
  IO.println s!"first_error_detected={firstErrorSensitivity}"
  IO.println s!"partition_dependence_detected={partitionSensitivity}"
  IO.println s!"cross_pipe_order_claim_detected={crossPipeOrderSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if cases.all caseSafe then 0 else 2

private def printStats : IO UInt32 := do
  IO.println s!"fixed_cases={cases.length}"
  IO.println "sensitivity_families=13"
  IO.println s!"marker_bytes={truncationMarker.length}"
  IO.println "maximum_capacity_enumerated=36"
  IO.println "maximum_stream_bytes_enumerated=40"
  return 0

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--sensitivity"] => printSensitivity
  | ["--cases"] => printCases
  | ["--stats"] => printStats
  | _ =>
      IO.eprintln "usage: generate_output_retention --output PATH | --check PATH | --sensitivity | --cases | --stats"
      return 2

end HoiminOracle.OutputRetention.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.OutputRetention.Executable.main args
