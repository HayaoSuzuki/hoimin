import HoiminOracle.ProgressInputCases
import Lean.Data.Json

open HoiminOracle HoiminOracle.ProgressInput HoiminOracle.MutationScoreExitPolicy

private def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

private def dispositionName : Disposition → String
  | .invalid => "invalid"
  | .usable => "usable"
  | .incomplete => "incomplete"
  | .baselineFailed => "baseline_failed"
  | .missingBaseline => "missing_baseline"

private def latestName : ProgressDecision.LatestState → String
  | .saturated => "saturated"
  | .stalled => "stalled"
  | .improving => "improving"
  | .regressing => "regressing"
  | .indeterminate => "indeterminate"

private def caseJson (id : String) (input : Input) : Lean.Json := Id.run do
  let counts := summarize input.statuses
  let score := match exactScore counts with
    | none => Lean.Json.null
    | some score => Lean.Json.mkObj [
        ("numerator", Lean.toJson score.numerator),
        ("denominator", Lean.toJson score.denominator)]
  let history := match toProgressReport input with
    | none => Lean.Json.null
    | some report =>
      let result := ProgressDecision.compareHistory [report, report] 1
      Lean.Json.mkObj [
        ("latest_state", .str (latestName result.latest)),
        ("consecutive_stalls", Lean.toJson result.consecutiveStalls),
        ("comparisons", Lean.toJson result.comparisons.length)]
  return Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str id), ("mode", .str "strict"),
    ("statuses", Lean.toJson (input.statuses.map statusName)),
    ("baseline", .str (match input.baseline with
      | .passed => "passed" | .failed => "failed" | .missing => "missing")),
    ("complete", Lean.toJson input.reportedComplete),
    ("exit_code", Lean.toJson input.reportedExit),
    ("counts", Lean.Json.mkObj [
      ("killed", Lean.toJson counts.killed), ("survived", Lean.toJson counts.survived),
      ("timeout", Lean.toJson counts.timeout), ("out_of_memory", Lean.toJson counts.outOfMemory),
      ("process_limit", Lean.toJson counts.processLimit), ("error", Lean.toJson counts.error),
      ("not_run", Lean.toJson counts.notRun), ("inconclusive", Lean.toJson (inconclusive counts))]),
    ("score", score), ("expected_disposition", .str (dispositionName (classify input))),
    ("expected_history", history)]

private def renderCorpus : String :=
  String.join (cases.map fun (id, input) => (caseJson id input).compress ++ "\n")

def main (args : List String) : IO UInt32 := do
  unless sensitivities.all (·.2) do
    IO.eprintln "progress input sensitivity failed"
    return 2
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path renderCorpus
    return 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) == renderCorpus then return 0
    IO.eprintln s!"stale progress input corpus: {path}"
    return 1
  | ["--sensitivity"] =>
    for (name, detected) in sensitivities do IO.println s!"{name}={detected}"
    return 0
  | ["--stats"] =>
    IO.println s!"profiles={profiles.length} cases={cases.length} exit_codes=8 complete_values=2"
    return 0
  | _ =>
    IO.eprintln "usage: generate_progress_input --output PATH | --check PATH | --sensitivity | --stats"
    return 2
