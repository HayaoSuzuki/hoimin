import HoiminOracle.MutationScoreExitPolicyCases
import Lean.Data.Json

open HoiminOracle.MutationScoreExitPolicy

namespace HoiminOracle.MutationScoreExitPolicy.Executable

private def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

private def statusesJson (statuses : List Status) : Lean.Json :=
  .arr (statuses.toArray.map fun status => .str (statusName status))

private def countsJson (counts : Counts) : Lean.Json := Lean.Json.mkObj [
  ("killed", Lean.toJson counts.killed),
  ("survived", Lean.toJson counts.survived),
  ("timeout", Lean.toJson counts.timeout),
  ("out_of_memory", Lean.toJson counts.outOfMemory),
  ("process_limit", Lean.toJson counts.processLimit),
  ("error", Lean.toJson counts.error),
  ("not_run", Lean.toJson counts.notRun),
  ("inconclusive", Lean.toJson (inconclusive counts)) ]

private def fractionJson : Option ExactFraction → Lean.Json
  | none => .null
  | some score => Lean.Json.mkObj [
      ("numerator", Lean.toJson score.numerator),
      ("denominator", Lean.toJson score.denominator) ]

private def policyJson (policy : ExitPolicy) : Lean.Json := Lean.Json.mkObj [
  ("infrastructure_error", Lean.toJson policy.infrastructureError),
  ("baseline_failed", Lean.toJson policy.baselineFailed),
  ("incomplete", Lean.toJson policy.incomplete),
  ("survivors", Lean.toJson policy.survivors),
  ("interrupted", Lean.toJson policy.interrupted) ]

private def flagsJson (flags : RunFlags) : Lean.Json := Lean.Json.mkObj [
  ("infrastructure_error", Lean.toJson flags.infrastructureError),
  ("baseline_failed", Lean.toJson flags.baselineFailed),
  ("incomplete", Lean.toJson flags.incomplete),
  ("interrupted", Lean.toJson flags.interrupted) ]

private def caseJson (item : OracleCase) : Lean.Json :=
  let observation := caseObservation item
  let modelOnlyScore := reduceFraction 9007199254740993 9007199254740995
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str item.mode),
    ("scenario", .str item.scenario),
    ("statuses", statusesJson item.statuses),
    ("run_flags", flagsJson item.flags),
    ("direct_policy", policyJson item.directPolicy),
    ("expected_counts", countsJson observation.counts),
    ("expected_score", fractionJson
      (if item.scenario == "exact_fraction" then some modelOnlyScore else observation.score)),
    ("expected_policy", policyJson observation.policy),
    ("expected_complete", Lean.toJson observation.complete),
    ("expected_exit_code", Lean.toJson (caseExitCode item)) ]

def renderCorpus : String :=
  String.join (cases.map fun item => (caseJson item).compress ++ "\n")

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "mutation score audit missed a broken family"
    return .error 2
  unless cases.all caseSafe do
    IO.eprintln "mutation score cases violate the closed contract"
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
        IO.eprintln s!"stale mutation score corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check mutation score corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  IO.println s!"wrong_count_detected={wrongCountSensitivity}"
  IO.println s!"inconclusive_denominator_detected={denominatorSensitivity}"
  IO.println s!"zero_score_detected={zeroScoreSensitivity}"
  IO.println s!"wrong_numerator_detected={numeratorSensitivity}"
  IO.println s!"incomplete_class_detected={incompleteClassSensitivity}"
  IO.println s!"survivor_before_incomplete_detected={survivorBeforeIncompleteSensitivity}"
  IO.println s!"survivor_before_baseline_detected={survivorBeforeBaselineSensitivity}"
  IO.println s!"survivor_before_infrastructure_detected={survivorBeforeInfrastructureSensitivity}"
  IO.println s!"survivor_before_interrupted_detected={survivorBeforeInterruptedSensitivity}"
  IO.println s!"baseline_precedence_detected={baselinePrecedenceSensitivity}"
  IO.println s!"interruption_precedence_detected={interruptionPrecedenceSensitivity}"
  IO.println s!"survivor_complete_detected={survivorCompleteSensitivity}"
  IO.println s!"run_finished_complete_detected={runFinishedCompleteSensitivity}"
  return if sensitivityPasses then 0 else 2

private def printCases : IO UInt32 := do
  for item in cases do IO.println s!"{item.id}={caseSafe item}"
  return if cases.all caseSafe then 0 else 2

private def printStats : IO UInt32 := do
  IO.println s!"fixed_cases={cases.length}"
  IO.println s!"exit_policy_cases={policyCases.length}"
  IO.println "boolean_policy_domain=32"
  IO.println "status_sensitivity_domain=7"
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
      IO.eprintln "usage: generate_mutation_score_exit_policy --output PATH | --check PATH | --sensitivity | --cases | --stats"
      return 2

end HoiminOracle.MutationScoreExitPolicy.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.MutationScoreExitPolicy.Executable.main args
