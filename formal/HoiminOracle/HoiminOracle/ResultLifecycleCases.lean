import HoiminOracle.ResultLifecycleProofs
import Lean.Data.Json

namespace HoiminOracle.ResultLifecycle

structure ExpectedObservation where
  accepted : List Result
  durable : List Result
  reported : List Result
  summary : List Status
  metricsExecuted : Nat
  stopped : Bool
  sessionFinished : Bool
  sessionComplete : Bool
  metricsFinished : Bool
  runComplete : Bool
  returned : Bool
  diagnostics : List Diagnostic
  deriving Repr, DecidableEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  setup : Setup
  schedule : List String
  expected : ExpectedObservation
  deriving Repr, DecidableEq

private structure CaseSpec where
  id : String
  mode : String
  scenario : String
  setup : Setup
  schedule : List Event

def mutantName : Mutant → String
  | .m0 => "m0"
  | .m1 => "m1"

def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

def diagnosticName : Diagnostic → String
  | .persistenceFailed => "persistence_failed"
  | .reportFailed => "report_failed"
  | .metricsFailed => "metrics_failed"

def eventName : Event → String
  | .discover mutant => s!"discover:{mutantName mutant}"
  | .accept mutant status => s!"accept:{mutantName mutant}:{statusName status}"
  | .persistOk mutant => s!"persist_ok:{mutantName mutant}"
  | .persistFailed mutant => s!"persist_failed:{mutantName mutant}"
  | .recordResult mutant => s!"record_result:{mutantName mutant}"
  | .reportOk mutant => s!"report_ok:{mutantName mutant}"
  | .reportFailed mutant => s!"report_failed:{mutantName mutant}"
  | .stop => "stop"
  | .markNotRun mutant => s!"mark_not_run:{mutantName mutant}"
  | .finishSession complete => s!"finish_session:{complete}"
  | .finishMetrics => "finish_metrics"
  | .metricsFailed => "metrics_failed"
  | .returnRun => "return_run"

private def noSessionSetup : Setup where
  session := false
  metrics := true
  discovered := [.m0]
  seededDurable := []

private def stopSetup : Setup where
  session := true
  metrics := true
  discovered := [.m0, .m1]
  seededDurable := []

private def observe (state : State) : ExpectedObservation where
  accepted := state.accepted
  durable := state.durable
  reported := state.reported
  summary := state.summary
  metricsExecuted := state.metricsExecuted
  stopped := state.stopped
  sessionFinished := state.sessionFinished
  sessionComplete := state.sessionComplete
  metricsFinished := state.metricsFinished
  runComplete := state.complete
  returned := state.returned
  diagnostics := state.diagnostics

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  mode := spec.mode
  scenario := spec.scenario
  setup := spec.setup
  schedule := spec.schedule.map eventName
  expected := observe (run (State.initial spec.setup) spec.schedule)

private def specs : List CaseSpec := [
  { id := "sessionless_complete"
    mode := "strict"
    scenario := "sessionless_complete"
    setup := noSessionSetup
    schedule := [
      .accept .m0 .killed,
      .recordResult .m0,
      .reportOk .m0,
      .finishMetrics,
      .returnRun
    ] },
  { id := "session_complete"
    mode := "strict"
    scenario := "session_complete"
    setup := oneMutantSetup
    schedule := acceptedTrace },
  { id := "resume_reuses_determinate"
    mode := "strict"
    scenario := "resume_reuses_determinate"
    setup := resumeSetup
    schedule := [
      .recordResult .m0,
      .reportOk .m0,
      .finishSession true,
      .finishMetrics,
      .returnRun
    ] },
  { id := "stop_preserves_accepted"
    mode := "strict"
    scenario := "stop_preserves_accepted"
    setup := stopSetup
    schedule := [
      .accept .m0 .timeout,
      .persistOk .m0,
      .recordResult .m0,
      .reportOk .m0,
      .stop,
      .markNotRun .m1,
      .finishSession false,
      .finishMetrics,
      .returnRun
    ] },
  { id := "metrics_write_failure"
    mode := "strict"
    scenario := "metrics_write_failure"
    setup := oneMutantSetup
    schedule := metricsFailureTrace },
  { id := "session_persistence_failure"
    mode := "strict"
    scenario := "session_persistence_failure"
    setup := oneMutantSetup
    schedule := [
      .accept .m0 .killed,
      .persistFailed .m0,
      .recordResult .m0,
      .reportOk .m0,
      .finishSession false,
      .finishMetrics,
      .returnRun
    ] },
  { id := "stop_during_persist"
    mode := "internal-fixture"
    scenario := "stop_during_persist"
    setup := oneMutantSetup
    schedule := stopDuringPersistTrace },
  { id := "duplicate_completion"
    mode := "model-only"
    scenario := "duplicate_completion"
    setup := oneMutantSetup
    schedule := duplicateReportTrace },
  { id := "stop_after_summary_before_report"
    mode := "model-only"
    scenario := "stop_after_summary_before_report"
    setup := oneMutantSetup
    schedule := stopAfterSummaryBeforeReportTrace }
]

def cases : List OracleCase := specs.map toOracleCase

def caseStatesSafe : Bool :=
  specs.all fun spec => safe (run (State.initial spec.setup) spec.schedule)

private def resultJson (result : Result) : Lean.Json := Lean.Json.mkObj [
  ("mutant", .str (mutantName result.mutant)),
  ("status", .str (statusName result.status)),
  ("executed", Lean.toJson result.executed)
]

private def resultsJson (results : List Result) : Lean.Json :=
  .arr (results.toArray.map resultJson)

private def mutantsJson (mutants : List Mutant) : Lean.Json :=
  .arr (mutants.toArray.map fun mutant => .str (mutantName mutant))

private def statusesJson (statuses : List Status) : Lean.Json :=
  .arr (statuses.toArray.map fun status => .str (statusName status))

private def diagnosticsJson (diagnostics : List Diagnostic) : Lean.Json :=
  .arr (diagnostics.toArray.map fun diagnostic => .str (diagnosticName diagnostic))

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def setupJson (setup : Setup) : Lean.Json := Lean.Json.mkObj [
  ("session", Lean.toJson setup.session),
  ("metrics", Lean.toJson setup.metrics),
  ("discovered", mutantsJson setup.discovered),
  ("seeded_durable", resultsJson setup.seededDurable)
]

private def observationJson (observation : ExpectedObservation) : Lean.Json :=
  Lean.Json.mkObj [
    ("accepted", resultsJson observation.accepted),
    ("durable", resultsJson observation.durable),
    ("reported", resultsJson observation.reported),
    ("summary", statusesJson observation.summary),
    ("metrics_executed", Lean.toJson observation.metricsExecuted),
    ("stopped", Lean.toJson observation.stopped),
    ("session_finished", Lean.toJson observation.sessionFinished),
    ("session_complete", Lean.toJson observation.sessionComplete),
    ("metrics_finished", Lean.toJson observation.metricsFinished),
    ("run_complete", Lean.toJson observation.runComplete),
    ("returned", Lean.toJson observation.returned),
    ("diagnostics", diagnosticsJson observation.diagnostics)
  ]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("scenario", .str item.scenario),
  ("setup", setupJson item.setup),
  ("schedule", stringsJson item.schedule),
  ("expected", observationJson item.expected)
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

end HoiminOracle.ResultLifecycle
