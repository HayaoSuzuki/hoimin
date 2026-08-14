import HoiminOracle.MutationScoreExitPolicyProofs

namespace HoiminOracle.MutationScoreExitPolicy

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  statuses : List Status := []
  flags : RunFlags := {}
  directPolicy : ExitPolicy := {}
  deriving Repr, DecidableEq, BEq

def statusCases : List OracleCase :=
  [ { id := "summary_empty", mode := "strict", scenario := "summary" }
  , { id := "summary_killed", mode := "strict", scenario := "summary", statuses := [.killed] }
  , { id := "summary_survived", mode := "strict", scenario := "summary", statuses := [.survived] }
  , { id := "summary_timeout", mode := "strict", scenario := "summary", statuses := [.timeout] }
  , { id := "summary_out_of_memory", mode := "strict", scenario := "summary", statuses := [.outOfMemory] }
  , { id := "summary_process_limit", mode := "strict", scenario := "summary", statuses := [.processLimit] }
  , { id := "summary_error", mode := "strict", scenario := "summary", statuses := [.error] }
  , { id := "summary_not_run", mode := "strict", scenario := "summary", statuses := [.notRun] }
  , { id := "summary_all_statuses", mode := "strict", scenario := "summary",
      statuses := [.killed, .survived, .timeout, .outOfMemory, .processLimit, .error, .notRun] }
  , { id := "summary_score_two_thirds", mode := "strict", scenario := "summary",
      statuses := [.killed, .survived, .killed] } ]

def bools : List Bool := [false, true]

def policyId (policy : ExitPolicy) : String :=
  let bit (value : Bool) := if value then "1" else "0"
  "policy_" ++ bit policy.infrastructureError ++ bit policy.baselineFailed ++
    bit policy.incomplete ++ bit policy.survivors ++ bit policy.interrupted

def policyCases : List OracleCase :=
  bools.flatMap fun infrastructure =>
    bools.flatMap fun baseline =>
      bools.flatMap fun incompleteRun =>
        bools.flatMap fun survivors =>
          bools.map fun interrupted =>
            let policy : ExitPolicy := {
              infrastructureError := infrastructure
              baselineFailed := baseline
              incomplete := incompleteRun
              survivors := survivors
              interrupted := interrupted }
            { id := policyId policy
              mode := "strict"
              scenario := "exit_policy"
              directPolicy := policy }

def composedCases : List OracleCase :=
  [ { id := "composed_summary_error", mode := "internal-fixture", scenario := "composed",
      statuses := [.error] }
  , { id := "composed_run_infrastructure", mode := "internal-fixture", scenario := "composed",
      flags := { infrastructureError := true } }
  , { id := "composed_interrupted_error_survivor", mode := "internal-fixture", scenario := "composed",
      statuses := [.error, .survived], flags := { interrupted := true } }
  , { id := "composed_baseline_timeout_survivor", mode := "internal-fixture", scenario := "composed",
      statuses := [.timeout, .survived], flags := { baselineFailed := true } }
  , { id := "composed_survivor_complete", mode := "internal-fixture", scenario := "composed",
      statuses := [.survived] } ]

def modelOnlyLargeFraction : OracleCase where
  id := "exact_fraction_beyond_binary64"
  mode := "model-only"
  scenario := "exact_fraction"

def cases : List OracleCase :=
  statusCases ++ policyCases ++ composedCases ++ [modelOnlyLargeFraction]

def caseObservation (item : OracleCase) : Observation :=
  observe item.statuses item.flags

def caseExitCode (item : OracleCase) : Nat :=
  if item.scenario == "exit_policy" then exitCode item.directPolicy
  else (caseObservation item).exitCode

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 &&
    (item.mode == "strict" || item.mode == "internal-fixture" || item.mode == "model-only") &&
    (item.scenario == "summary" || item.scenario == "exit_policy" ||
      item.scenario == "composed" || item.scenario == "exact_fraction")

def brokenWrongCount (statuses : List Status) : Counts :=
  let result := summarize statuses
  { result with survived := result.survived + result.killed }

def brokenInconclusiveDenominator (counts : Counts) : Option ExactFraction :=
  let denominator := decidable counts + inconclusive counts
  if denominator = 0 then none else some (reduceFraction counts.killed denominator)

def brokenZeroScore (counts : Counts) : Option ExactFraction :=
  if decidable counts = 0 then some { numerator := 0, denominator := 1 }
  else exactScore counts

def brokenSurvivorNumerator (counts : Counts) : Option ExactFraction :=
  if decidable counts = 0 then none
  else some (reduceFraction counts.survived (decidable counts))

def brokenIncomplete (counts : Counts) : Bool :=
  counts.timeout > 0 || counts.outOfMemory > 0 || counts.processLimit > 0

def brokenSurvivorsFirst (policy : ExitPolicy) : Nat :=
  if policy.survivors then 1 else exitCode { policy with survivors := false }

def brokenBaselineBeforeInfrastructure (policy : ExitPolicy) : Nat :=
  if policy.interrupted then 130
  else if policy.baselineFailed then 3
  else if policy.infrastructureError then 2
  else if policy.incomplete then 4
  else if policy.survivors then 1 else 0

def brokenInfrastructureBeforeInterrupted (policy : ExitPolicy) : Nat :=
  if policy.infrastructureError then 2 else exitCode policy

def brokenSurvivorCompleteness (policy : ExitPolicy) : Bool :=
  complete policy && !policy.survivors

def brokenRunFinishedComplete (counts : Counts) (flags : RunFlags) : Bool :=
  let policy := composePolicy counts flags
  !policy.infrastructureError && !policy.baselineFailed && !flags.incomplete && !policy.interrupted

def wrongCountSensitivity : Bool :=
  brokenWrongCount [.killed] != summarize [.killed]

def denominatorSensitivity : Bool :=
  brokenInconclusiveDenominator (summarize [.killed, .timeout]) !=
    exactScore (summarize [.killed, .timeout])

def zeroScoreSensitivity : Bool := brokenZeroScore {} != exactScore {}

def numeratorSensitivity : Bool :=
  brokenSurvivorNumerator (summarize [.killed, .survived, .survived]) !=
    exactScore (summarize [.killed, .survived, .survived])

def incompleteClassSensitivity : Bool :=
  brokenIncomplete (summarize [.notRun]) != (policyFromCounts (summarize [.notRun])).incomplete

def survivorPrecedenceSensitivity : Bool :=
  brokenSurvivorsFirst { incomplete := true, survivors := true } !=
    exitCode { incomplete := true, survivors := true }

def baselinePrecedenceSensitivity : Bool :=
  brokenBaselineBeforeInfrastructure { infrastructureError := true, baselineFailed := true } !=
    exitCode { infrastructureError := true, baselineFailed := true }

def interruptionPrecedenceSensitivity : Bool :=
  brokenInfrastructureBeforeInterrupted { infrastructureError := true, interrupted := true } !=
    exitCode { infrastructureError := true, interrupted := true }

def survivorCompleteSensitivity : Bool :=
  brokenSurvivorCompleteness { survivors := true } != complete { survivors := true }

def runFinishedCompleteSensitivity : Bool :=
  let counts := summarize [.notRun]
  brokenRunFinishedComplete counts {} != complete (composePolicy counts {})

def sensitivityPasses : Bool :=
  wrongCountSensitivity && denominatorSensitivity && zeroScoreSensitivity &&
    numeratorSensitivity && incompleteClassSensitivity && survivorPrecedenceSensitivity &&
    baselinePrecedenceSensitivity && interruptionPrecedenceSensitivity &&
    survivorCompleteSensitivity && runFinishedCompleteSensitivity

end HoiminOracle.MutationScoreExitPolicy

