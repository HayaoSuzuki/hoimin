import HoiminOracle.ProgressInputModel

namespace HoiminOracle.ProgressInput

open MutationScoreExitPolicy

def profiles : List (String × List Status) := [
  ("empty", []), ("killed", [.killed]), ("survived", [.survived]),
  ("timeout", [.timeout]), ("oom", [.outOfMemory]), ("process_limit", [.processLimit]),
  ("error", [.error]), ("not_run", [.notRun]),
  ("killed_timeout", [.killed, .timeout]), ("killed_survived", [.killed, .survived]),
  ("survived_error", [.survived, .error]) ]

def cases : List (String × Input) :=
  (profiles.flatMap fun (name, statuses) =>
    [false, true].flatMap fun done =>
      ([-1, 0, 1, 2, 3, 4, 130, 255] : List Int).map fun code =>
        (s!"{name}_{done}_{code}", ⟨statuses, .passed, done, code⟩)) ++
  ([Baseline.failed, .missing].flatMap fun baseline =>
    ([2, 3, 4, 130] : List Int).map fun code =>
      (s!"baseline_{if baseline == .failed then "failed" else "missing"}_{code}",
        ⟨[], baseline, false, code⟩))

def brokenBlindCompleteTrust (input : Input) : Bool := input.reportedComplete

def brokenOmitInconclusive (input : Input) : Bool :=
  input.reportedComplete &&
    input.reportedExit == (if (summarize input.statuses).survived > 0 then 1 else 0)

def brokenReversedErrorPrecedence (input : Input) : Bool :=
  coherent (summarize input.statuses) input.reportedComplete input.reportedExit ||
    (!input.reportedComplete && input.reportedExit == 4)

def sensitivities : List (String × Bool) := [
  ("blind_complete", cases.any fun (_, input) =>
    brokenBlindCompleteTrust input && classify input == .invalid),
  ("omitted_inconclusive", cases.any fun (_, input) =>
    brokenOmitInconclusive input && classify input == .invalid),
  ("reversed_error_precedence", cases.any fun (_, input) =>
    brokenReversedErrorPrecedence input && classify input == .invalid) ]

end HoiminOracle.ProgressInput
