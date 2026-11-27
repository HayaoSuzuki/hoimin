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

private def loadRecheckCase (finishBeforeAcquire : Option Bool) : Lean.Json :=
  let afterRead := run State.initial [
    .open .h0, .open .h1, .begin .h0 .r0 .f0, .loadReadCandidate .h1 .f0 ]
  let beforeAcquire := match finishBeforeAcquire with
    | none => afterRead
    | some complete => (step afterRead (.finish .h0 .r0 complete)).state
  let acquired := step beforeAcquire (.loadAcquire .h1)
  let (event, outcome) := if acquired.rejection.isSome then (.loadAcquire .h1, acquired)
    else (.loadRecheck .h1, step acquired.state (.loadRecheck .h1))
  let status := match outcome.rejection, outcome.selectedRun with
    | some error, _ => rejectionCode event error
    | none, some _ => "resumed"
    | none, none => "candidate_changed"
  let row := findRun outcome.state .r0
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)),
    ("mode", .str "internal-fixture"),
    ("finish_before_acquire", optionalJson Lean.toJson finishBeforeAcquire),
    ("expected_status", .str status),
    ("expected_loader_owns", Lean.toJson (owns outcome.state .r0 .h1)),
    ("expected_lock_held", Lean.toJson (ownerCount outcome.state .r0 != 0)),
    ("expected_run_finished", optionalJson Lean.toJson (row.map RunRow.finished)),
    ("expected_run_complete", optionalJson Lean.toJson (row.map RunRow.complete)) ]

def renderLoadRecheckCorpus : String :=
  String.join ([none, some false, some true].map fun action =>
    (loadRecheckCase action).compress ++ "\n")

private def finishWaitSeed (priorFinished : Bool) : State :=
  let seed := run State.initial [
    .open .h0, .open .h1, .begin .h0 .r0 .f0, .persist .h0 .r0 .m0 .notRun .p0 .valid]
  if priorFinished then run seed [.finish .h0 .r0 false, .load .h0 .f0] else seed

private def finishWaitSnapshot (state : State) : Lean.Json :=
  let row := findRun state .r0
  let observer := loadRun state .h1 .f0
  let resume := match observer.rejection, observer.selectedRun with
    | some error, _ => rejectionCode (.load .h1 .f0) error
    | none, some _ => "resumed"
    | none, none => "matching_run_complete"
  Lean.Json.mkObj [
    ("finished", Lean.toJson (row.any RunRow.finished)),
    ("complete", Lean.toJson (row.any RunRow.complete)),
    ("resume", .str resume),
    ("result_count", Lean.toJson state.results.length)]

private def finishWaitCase (waiter : String) (arrange : PendingFinishWait → PendingFinishWait)
    (priorFinished complete commit : Bool) : Lean.Json :=
  let pending := arrange { session := finishWaitSeed priorFinished }
  let settled := settleFinishWait pending .h0 .r0 complete commit
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str s!"{waiter}_prior_{priorFinished}_complete_{complete}_commit_{commit}"),
    ("waiter", .str waiter), ("prior_finished", Lean.toJson priorFinished),
    ("complete", Lean.toJson complete), ("commit", Lean.toJson commit),
    ("pending", finishWaitSnapshot pending.session), ("settled", finishWaitSnapshot settled.session),
    ("reply", optionalJson Lean.toJson settled.reply)]

def renderFinishWaitCorpus : String :=
  let switches := [false, true]
  let waiters : List (String × (PendingFinishWait → PendingFinishWait)) :=
    [("awaited", id), ("cancelled", cancelFinishWait), ("driver_dropped", dropFinishDriver)]
  let cases := waiters.flatMap fun (waiter, arrange) =>
    switches.flatMap fun priorFinished => switches.flatMap fun complete =>
      switches.map fun commit => finishWaitCase waiter arrange priorFinished complete commit
  String.join (cases.map fun item => item.compress ++ "\n")

def finishWaitSensitivity : Bool :=
  let seed := finishWaitSeed false
  let detached := dropFinishDriver { session := seed }
  let committed := settleFinishWait detached .h0 .r0 true true
  let rolledBack := settleFinishWait detached .h0 .r0 true false
  (loadRun detached.session .h1 .f0).rejection == some .active &&
    committed.reply.isNone && runComplete committed.session .r0 &&
    rolledBack.session.runs == seed.runs &&
    (loadRun rolledBack.session .h1 .f0).selectedRun == some .r0

private def finishReleaseName : FinishRelease → String
  | .beforeDeadline => "before_deadline"
  | .withinGrace => "within_grace"
  | .afterGrace => "after_grace"

private def finishFlags (state : State) : Lean.Json :=
  let row := findRun state .r0
  Lean.Json.mkObj [
    ("finished", Lean.toJson (row.any RunRow.finished)),
    ("complete", Lean.toJson (row.any RunRow.complete))]

private def finishDeadlineCase (release : FinishRelease) (complete commit : Bool) : Lean.Json :=
  let result := finishAcrossShutdown ⟨finishWaitSeed false, true, true⟩ .h0 .r0 release complete commit
  let name := finishReleaseName release
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str s!"{name}_complete_{complete}_commit_{commit}"),
    ("release", .str name), ("complete", Lean.toJson complete), ("commit", Lean.toJson commit),
    ("exit_code", optionalJson Lean.toJson result.exitCode),
    ("session_acknowledged", Lean.toJson result.sessionAcknowledged),
    ("commit_failure_observed", Lean.toJson result.commitFailureObserved),
    ("at_return", finishFlags result.atReturn), ("settled", finishFlags result.settled)]

def renderFinishDeadlineCorpus : String :=
  let releases : List FinishRelease := [.beforeDeadline, .withinGrace, .afterGrace]
  let cases := releases.flatMap fun release => [false, true].flatMap fun complete =>
    [false, true].map fun commit => finishDeadlineCase release complete commit
  String.join (cases.map fun item => item.compress ++ "\n")

def finishDeadlineSensitivity : Bool :=
  let pending : PendingFinishWait := { session := finishWaitSeed false }
  let late := finishAcrossShutdown pending .h0 .r0 .afterGrace true true
  let timely := finishAcrossShutdown pending .h0 .r0 .withinGrace true true
  late.exitCode.isNone && !late.sessionAcknowledged && !runComplete late.atReturn .r0 &&
    runComplete late.settled .r0 && timely.sessionAcknowledged && runComplete timely.atReturn .r0

private def persistReadbackJson (status : Option Status) : Lean.Json :=
  Lean.Json.mkObj [
    ("status", optionalJson (fun status => .str (statusName status)) status),
    ("diagnostics", stringsJson (if status == some .error then ["process.output.close.timeout"] else []))]

private def persistSnapshotJson (row : Option ResultRow) : Lean.Json :=
  let origin := row.map fun result => if result.payload == .p0 then "prior" else "current"
  Lean.Json.mkObj [
    ("result", persistReadbackJson (row.map ResultRow.status)),
    ("origin", optionalJson Lean.Json.str origin)]

private def persistDeadlineSeed (replaces : Bool) : State :=
  let fresh := run State.initial [.open .h0, .begin .h0 .r0 .f0]
  if replaces then run fresh [
    .persist .h0 .r0 .m0 .error .p0 .valid, .finish .h0 .r0 false, .load .h0 .f0]
  else fresh

private def persistDeadlineCase (release : FinishRelease)
    (replaces diagnostic commit : Bool) : Lean.Json :=
  let state := persistDeadlineSeed replaces
  let result := persistAcrossShutdown state .h0 .r0 .m0
    (if diagnostic then .error else .survived) release commit
  let settled := findResult result.settled .r0 .m0
  let stage := finishReleaseName release
  let operation := if replaces then "replace" else "insert"
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str s!"{operation}_{stage}_diagnostic_{diagnostic}_commit_{commit}"),
    ("replaces", Lean.toJson replaces),
    ("release", .str stage), ("diagnostic", Lean.toJson diagnostic), ("commit", Lean.toJson commit),
    ("exit_code", optionalJson Lean.toJson result.exitCode),
    ("pending", persistSnapshotJson (findResult state .r0 .m0)),
    ("at_return", persistSnapshotJson (findResult result.atReturn .r0 .m0)),
    ("settled", persistSnapshotJson settled),
    ("reported", persistReadbackJson result.reported),
    ("resume_reuses", Lean.toJson (settled.any (fun row => row.status.isDeterminate)))]

def renderPersistDeadlineCorpus : String :=
  let releases : List FinishRelease := [.beforeDeadline, .withinGrace, .afterGrace]
  let cases := [false, true].flatMap fun replaces =>
    releases.flatMap fun release => [false, true].flatMap fun diagnostic =>
      [false, true].map fun commit => persistDeadlineCase release replaces diagnostic commit
  String.join (cases.map fun item => item.compress ++ "\n")

def persistDeadlineSensitivity : Bool :=
  let seed := persistDeadlineSeed false
  let late := persistAcrossShutdown seed .h0 .r0 .m0 .survived .afterGrace true
  let failed := persistAcrossShutdown seed .h0 .r0 .m0 .survived .withinGrace false
  let previous := persistDeadlineSeed true
  let replaced := persistAcrossShutdown previous .h0 .r0 .m0 .error .afterGrace true
  let rolledBack := persistAcrossShutdown previous .h0 .r0 .m0 .error .afterGrace false
  (findResult late.atReturn .r0 .m0).isNone && late.reported.isNone &&
    (findResult late.settled .r0 .m0).any (fun result => result.status.isDeterminate) &&
    (findResult failed.settled .r0 .m0).isNone && failed.reported == some .survived &&
    (findResult replaced.atReturn .r0 .m0).map ResultRow.payload == some .p0 &&
    (findResult replaced.settled .r0 .m0).map ResultRow.payload == some .p1 &&
    (findResult rolledBack.settled .r0 .m0).map ResultRow.payload == some .p0

end HoiminOracle.SessionAudit
