import HoiminOracle.SessionProofs
import Lean.Data.Json

namespace HoiminOracle.SessionAudit

structure OracleStep where
  event : String
  verdict : String
  errorCode : Option String
  selectedRun : Option Run
  storedResult : Option ResultRow
  runs : List RunRow
  results : List ResultRow
  owners : List (Run × Handler)
  deriving Repr, DecidableEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String := "strict"
  schedule : List String
  expected : List OracleStep
  deriving Repr, DecidableEq

private structure NamedEvent where
  name : String
  event : Event

private structure CaseSpec where
  id : String
  schedule : List NamedEvent

def handlerName : Handler → String
  | .h0 => "h0"
  | .h1 => "h1"

def runName : Run → String
  | .r0 => "r0"
  | .r1 => "r1"

def fingerprintName : Fingerprint → String
  | .f0 => "f0"
  | .f1 => "f1"

def mutantName : Mutant → String
  | .m0 => "m0"
  | .m1 => "m1"

def payloadName : Payload → String
  | .p0 => "p0"
  | .p1 => "p1"

def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

def validityName : PersistValidity → String
  | .valid => "valid"
  | .invalidDiagnostic => "invalid_diagnostic"

def eventName : Event → String
  | .open handler => s!"open:{handlerName handler}"
  | .begin handler run fingerprint =>
      s!"begin:{handlerName handler}:{runName run}:{fingerprintName fingerprint}"
  | .load handler fingerprint =>
      s!"load:{handlerName handler}:{fingerprintName fingerprint}"
  | .lookup handler run mutant =>
      s!"lookup:{handlerName handler}:{runName run}:{mutantName mutant}"
  | .persist handler run mutant status payload validity =>
      s!"persist:{handlerName handler}:{runName run}:{mutantName mutant}:" ++
        s!"{statusName status}:{payloadName payload}:{validityName validity}"
  | .finish handler run complete =>
      s!"finish:{handlerName handler}:{runName run}:{complete}"
  | .drop handler => s!"drop:{handlerName handler}"
  | .crash handler => s!"crash:{handlerName handler}"
  | .loadReadCandidate handler fingerprint =>
      s!"internal-load-read:{handlerName handler}:{fingerprintName fingerprint}"
  | .loadAcquire handler => s!"internal-load-acquire:{handlerName handler}"
  | .loadRecheck handler => s!"internal-load-recheck:{handlerName handler}"
  | .replacementDelete handler run mutant status payload =>
      s!"internal-replacement-delete:{handlerName handler}:{runName run}:" ++
        s!"{mutantName mutant}:{statusName status}:{payloadName payload}"
  | .replacementCommit handler =>
      s!"internal-replacement-commit:{handlerName handler}"
  | .replacementRollback handler =>
      s!"internal-replacement-rollback:{handlerName handler}"

private def named (event : Event) : NamedEvent := {
  name := eventName event
  event := event
}

private def observeNamed (item : NamedEvent) (verdict : Verdict) : OracleStep :=
  let observation := observe item.event verdict
  {
    event := item.name
    verdict := observation.verdict
    errorCode := observation.errorCode
    selectedRun := observation.selectedRun
    storedResult := observation.storedResult
    runs := observation.runs
    results := observation.results
    owners := observation.owners
  }

private def runSchedule : State → List NamedEvent → List OracleStep
  | _, [] => []
  | state, item :: rest =>
      let verdict := step state item.event
      observeNamed item verdict :: runSchedule verdict.state rest

private def toOracleCase (spec : CaseSpec) : OracleCase where
  id := spec.id
  schedule := spec.schedule.map NamedEvent.name
  expected := runSchedule State.initial spec.schedule

private def open0 : NamedEvent := named (.open .h0)
private def open1 : NamedEvent := named (.open .h1)
private def begin0 : NamedEvent := named (.begin .h0 .r0 .f0)

private def replacementCase (id : String) (status : Status) : CaseSpec := {
  id := id
  schedule := [
    open0,
    begin0,
    named (.persist .h0 .r0 .m0 status .p0 .valid),
    named (.persist .h0 .r0 .m0 .killed .p1 .valid),
    named (.lookup .h0 .r0 .m0)
  ]
}

private def specs : List CaseSpec := [
  { id := "newest_compatible_incomplete_run"
    schedule := [
      open0,
      begin0,
      named (.finish .h0 .r0 false),
      named (.begin .h0 .r1 .f0),
      named (.load .h0 .f0)
    ] },
  { id := "live_owner_rejects_second_handler"
    schedule := [open0, open1, begin0, named (.load .h1 .f0)] },
  { id := "incomplete_finish_releases_for_resume"
    schedule := [
      open0, open1, begin0,
      named (.finish .h0 .r0 false),
      named (.load .h1 .f0)
    ] },
  { id := "handler_drop_releases_for_resume"
    schedule := [open0, open1, begin0, named (.drop .h0), named (.load .h1 .f0)] },
  { id := "completed_run_is_final"
    schedule := [
      open0, open1, begin0,
      named (.finish .h0 .r0 true),
      named (.load .h1 .f0),
      named (.lookup .h1 .r0 .m0),
      named (.persist .h1 .r0 .m0 .killed .p0 .valid),
      named (.finish .h1 .r0 true)
    ] },
  { id := "released_handler_cannot_finish_again"
    schedule := [
      open0, begin0,
      named (.finish .h0 .r0 false),
      named (.finish .h0 .r0 false),
      named (.finish .h0 .r0 true),
      named (.finish .h0 .r0 false)
    ] },
  { id := "determinate_killed_is_immutable"
    schedule := [
      open0, begin0,
      named (.persist .h0 .r0 .m0 .killed .p0 .valid),
      named (.persist .h0 .r0 .m0 .timeout .p1 .valid),
      named (.lookup .h0 .r0 .m0)
    ] },
  { id := "determinate_survived_is_immutable"
    schedule := [
      open0, begin0,
      named (.persist .h0 .r0 .m0 .survived .p0 .valid),
      named (.persist .h0 .r0 .m0 .killed .p1 .valid),
      named (.lookup .h0 .r0 .m0)
    ] },
  replacementCase "timeout_replacement_commits_atomically" .timeout,
  replacementCase "oom_replacement_commits_atomically" .outOfMemory,
  replacementCase "process_limit_replacement_commits_atomically" .processLimit,
  replacementCase "error_replacement_commits_atomically" .error,
  replacementCase "not_run_replacement_commits_atomically" .notRun,
  { id := "invalid_replacement_rolls_back"
    schedule := [
      open0, begin0,
      named (.persist .h0 .r0 .m0 .timeout .p0 .valid),
      named (.persist .h0 .r0 .m0 .killed .p1 .invalidDiagnostic),
      named (.lookup .h0 .r0 .m0)
    ] },
  { id := "missing_run_persist_requires_ownership"
    schedule := [open0, named (.persist .h0 .r0 .m0 .killed .p0 .valid)] },
  { id := "lookup_missing_and_completed_reject"
    schedule := [
      open0, open1,
      named (.lookup .h1 .r0 .m0),
      begin0,
      named (.finish .h0 .r0 true),
      named (.lookup .h1 .r0 .m0)
    ] },
  { id := "different_runs_have_independent_owners"
    schedule := [
      open0, open1, begin0,
      named (.begin .h1 .r1 .f1),
      named (.load .h1 .f0),
      named (.load .h0 .f1)
    ] },
  { id := "non_owner_incomplete_finish_is_rejected"
    schedule := [
      open0, open1, begin0,
      named (.finish .h1 .r0 false),
      named (.load .h1 .f0)
    ] },
  { id := "result_operations_require_the_requested_runs_owner"
    schedule := [
      open0, open1, begin0,
      named (.begin .h1 .r1 .f1),
      named (.persist .h0 .r0 .m0 .timeout .p0 .valid),
      named (.load .h1 .f0),
      named (.lookup .h1 .r0 .m0),
      named (.persist .h1 .r0 .m0 .killed .p1 .valid),
      named (.persist .h1 .r0 .m1 .killed .p1 .valid),
      named (.lookup .h0 .r0 .m0)
    ] },
  { id := "result_operations_require_reacquisition_after_finish"
    schedule := [
      open0, open1, begin0,
      named (.finish .h0 .r0 false),
      named (.persist .h0 .r0 .m0 .killed .p0 .valid),
      named (.lookup .h0 .r0 .m0),
      named (.load .h1 .f0),
      named (.persist .h0 .r0 .m0 .killed .p0 .valid),
      named (.lookup .h0 .r0 .m0),
      named (.persist .h1 .r0 .m0 .killed .p1 .valid),
      named (.finish .h1 .r0 false),
      named (.load .h0 .f0),
      named (.lookup .h0 .r0 .m0),
      named (.persist .h0 .r0 .m1 .killed .p0 .valid)
    ] }
]

def cases : List OracleCase := specs.map toOracleCase

private def optionalJson (render : α → Lean.Json) : Option α → Lean.Json
  | none => .null
  | some value => render value

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def runRowJson (row : RunRow) : Lean.Json := Lean.Json.mkObj [
  ("run", .str (runName row.run)),
  ("fingerprint", .str (fingerprintName row.fingerprint)),
  ("ordinal", Lean.toJson row.ordinal),
  ("finished", Lean.toJson row.finished),
  ("complete", Lean.toJson row.complete)
]

private def resultRowJson (row : ResultRow) : Lean.Json := Lean.Json.mkObj [
  ("run", .str (runName row.run)),
  ("mutant", .str (mutantName row.mutant)),
  ("status", .str (statusName row.status)),
  ("payload", .str (payloadName row.payload))
]

private def ownerJson (owner : Run × Handler) : Lean.Json := Lean.Json.mkObj [
  ("run", .str (runName owner.1)),
  ("handler", .str (handlerName owner.2))
]

private def runsJson (rows : List RunRow) : Lean.Json :=
  .arr (rows.toArray.map runRowJson)

private def resultsJson (rows : List ResultRow) : Lean.Json :=
  .arr (rows.toArray.map resultRowJson)

private def ownersJson (owners : List (Run × Handler)) : Lean.Json :=
  .arr (owners.toArray.map ownerJson)

def oracleStepJson (item : OracleStep) : Lean.Json := Lean.Json.mkObj [
  ("event", .str item.event),
  ("verdict", .str item.verdict),
  ("error_code", optionalJson Lean.Json.str item.errorCode),
  ("selected_run", optionalJson (Lean.Json.str ∘ runName) item.selectedRun),
  ("stored_result", optionalJson resultRowJson item.storedResult),
  ("runs", runsJson item.runs),
  ("results", resultsJson item.results),
  ("owners", ownersJson item.owners)
]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("schedule", stringsJson item.schedule),
  ("expected", .arr (item.expected.toArray.map oracleStepJson))
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

end HoiminOracle.SessionAudit
