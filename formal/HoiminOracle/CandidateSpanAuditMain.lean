import HoiminOracle.CandidateSpanCases
import Lean.Data.Json

open HoiminOracle.CandidateSpan

namespace HoiminOracle.CandidateSpan.Executable

private def bytesJson (bytes : List ByteValue) : Lean.Json :=
  .arr (bytes.toArray.map Lean.toJson)

private def boolsJson (values : List Bool) : Lean.Json :=
  .arr (values.toArray.map Lean.toJson)

private def candidateJson (candidate : Candidate) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson candidate.schema),
    ("path", .str candidate.path),
    ("start", Lean.toJson candidate.start),
    ("length", Lean.toJson candidate.length),
    ("original", bytesJson candidate.original),
    ("replacement", bytesJson candidate.replacement),
    ("operator", .str candidate.operator),
    ("line", Lean.toJson candidate.line),
    ("column", Lean.toJson candidate.column),
    ("symbol", Lean.toJson candidate.symbol),
    ("hash_mode", .str candidate.sourceHash),
    ("sequence", Lean.toJson candidate.sequence) ]

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("source", bytesJson item.environment.source),
    ("boundaries", boolsJson item.environment.boundaries),
    ("scalar_starts", boolsJson item.environment.scalarStarts),
    ("maximum_offset", Lean.toJson item.environment.maximumOffset),
    ("candidate", candidateJson item.candidate),
    ("expected_valid", Lean.toJson item.expectedValid),
    ("expected_bytes", bytesJson item.expectedBytes),
    ("rejection", Lean.toJson item.rejection) ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless cases.all fun item => caseSafe item && caseResult item == (item.expectedValid, item.expectedBytes) do
    IO.eprintln "candidate span cases violate their closed expectations"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "candidate span audit missed a broken family"
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
        IO.eprintln s!"stale candidate span corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check candidate span corpus {path}: {error}"
        return 1

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item && caseResult item == (item.expectedValid, item.expectedBytes)}"
  return if cases.all fun item => caseSafe item && caseResult item == (item.expectedValid, item.expectedBytes)
    then 0 else 2

private def printSensitivity : IO UInt32 := do
  IO.println s!"byte_character_offset_detected={byteCharacterOffsetSensitivity}"
  IO.println s!"byte_column_detected={byteColumnSensitivity}"
  IO.println s!"off_by_one_detected={offByOneSensitivity}"
  IO.println s!"replacement_length_detected={replacementLengthSensitivity}"
  IO.println s!"overflow_boundary_detected={overflowBoundarySensitivity}"
  IO.println s!"snapshot_location_detected={snapshotLocationSensitivity}"
  IO.println s!"transport_field_detected={transportFieldSensitivity}"
  IO.println s!"identity_omission_detected={identityOmissionSensitivity}"
  IO.println s!"apply_before_validation_detected={applyBeforeValidationSensitivity}"
  IO.println s!"truncate_before_validation_detected={truncateBeforeValidationSensitivity}"
  IO.println s!"suffix_detected={suffixSensitivity}"
  IO.println s!"missing_reset_detected={missingResetSensitivity}"
  return if sensitivityPasses then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--cases"] => printCases
  | ["--sensitivity"] => printSensitivity
  | ["--stats"] =>
      IO.println s!"fixed_cases={cases.length}"
      IO.println "sensitivity_families=12"
      IO.println "strict_cases=3"
      return 0
  | _ =>
      IO.eprintln "usage: generate_candidate_span --output PATH | --check PATH | --cases | --sensitivity | --stats"
      return 2

end HoiminOracle.CandidateSpan.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.CandidateSpan.Executable.main args
