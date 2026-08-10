import Std

namespace HoiminOracle.ResultLifecycle

inductive Mutant
  | m0
  | m1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Status
  | killed
  | survived
  | timeout
  | outOfMemory
  | processLimit
  | error
  | notRun
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Result where
  mutant : Mutant
  status : Status
  executed : Bool
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Setup where
  session : Bool
  metrics : Bool
  discovered : List Mutant
  seededDurable : List Result
  deriving Repr, DecidableEq, BEq

inductive Diagnostic
  | persistenceFailed
  | reportFailed
  | metricsFailed
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure State where
  setup : Setup
  accepted : List Result
  durable : List Result
  persistenceFailures : List Mutant
  summarized : List Result
  reported : List Result
  summary : List Status
  metricsExecuted : Nat
  stopped : Bool
  sessionFinished : Bool
  sessionComplete : Bool
  metricsFinished : Bool
  returned : Bool
  complete : Bool
  diagnostics : List Diagnostic
  deriving Repr, DecidableEq, BEq

namespace State

def initial (setup : Setup) : State where
  setup := setup
  accepted := []
  durable := setup.seededDurable
  persistenceFailures := []
  summarized := []
  reported := []
  summary := []
  metricsExecuted := 0
  stopped := false
  sessionFinished := false
  sessionComplete := false
  metricsFinished := false
  returned := false
  complete := false
  diagnostics := []

end State

inductive Event
  | discover (mutant : Mutant)
  | accept (mutant : Mutant) (status : Status)
  | persistOk (mutant : Mutant)
  | persistFailed (mutant : Mutant)
  | recordResult (mutant : Mutant)
  | reportOk (mutant : Mutant)
  | reportFailed (mutant : Mutant)
  | stop
  | markNotRun (mutant : Mutant)
  | finishSession (complete : Bool)
  | finishMetrics
  | metricsFailed
  | returnRun
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

namespace Event

def isLifecycleMutation : Event → Bool
  | .finishSession _ | .finishMetrics | .metricsFailed | .returnRun => false
  | _ => true

end Event

inductive Rejection
  | unknownMutant
  | duplicate
  | stopped
  | invalidStatus
  | resultMissing
  | persistenceRequired
  | sessionDisabled
  | alreadyFinished
  | finalizationRequired
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure Verdict where
  state : State
  rejection : Option Rejection
  deriving Repr, DecidableEq, BEq

def findResult (results : List Result) (mutant : Mutant) : Option Result :=
  results.find? fun result => result.mutant == mutant

def containsMutant (results : List Result) (mutant : Mutant) : Bool :=
  (findResult results mutant).isSome

def eraseMutant (results : List Result) (mutant : Mutant) : List Result :=
  results.filter fun result => result.mutant != mutant

def upsertResult (results : List Result) (result : Result) : List Result :=
  eraseMutant results result.mutant ++ [result]

def currentOrDurable? (state : State) (mutant : Mutant) : Option Result :=
  (findResult state.accepted mutant).orElse fun _ => findResult state.durable mutant

def resultBacked (state : State) (result : Result) : Bool :=
  result ∈ state.accepted || result ∈ state.setup.seededDurable

def reportBacked (state : State) (result : Result) : Bool :=
  result ∈ state.accepted || result ∈ state.durable ||
    (!result.executed && result.status == .notRun)

def identitiesUnique (results : List Result) : Bool :=
  decide (results.map Result.mutant).Nodup

def completeCoverage (state : State) : Bool :=
  state.setup.discovered.all fun mutant => containsMutant state.reported mutant

def runFailureFree (state : State) : Bool :=
  state.diagnostics.all fun diagnostic => diagnostic == .metricsFailed

def resultsConclusive (state : State) : Bool :=
  state.summary.all fun status => status == .killed || status == .survived

def exitCode (state : State) : Nat :=
  if state.diagnostics.any fun diagnostic => diagnostic != .metricsFailed then 2
  else if .error ∈ state.summary then 2
  else if state.stopped || state.summary.any fun status =>
      status == .timeout || status == .outOfMemory || status == .processLimit ||
        status == .notRun then 4
  else if .survived ∈ state.summary then 1
  else 0

def safe (state : State) : Bool :=
  identitiesUnique state.accepted &&
    identitiesUnique state.durable &&
    identitiesUnique state.summarized &&
    identitiesUnique state.reported &&
    (state.accepted.all fun result => result.executed && result.status != .notRun) &&
    (state.durable.all fun result => resultBacked state result) &&
    (state.summarized.all fun result => reportBacked state result) &&
    (state.reported.all fun result => result ∈ state.summarized) &&
    state.summary == state.summarized.map Result.status &&
    state.metricsExecuted == state.accepted.length &&
    (!state.complete || completeCoverage state) &&
    (!state.sessionComplete || (state.sessionFinished && !state.stopped &&
      runFailureFree state && completeCoverage state && resultsConclusive state)) &&
    (!state.returned || ((!state.setup.session || state.sessionFinished) &&
      (!state.setup.metrics || state.metricsFinished))) &&
    (!state.returned || state.complete ==
      (!state.stopped && runFailureFree state && completeCoverage state && resultsConclusive state &&
        (!state.setup.session || state.sessionComplete)))

def SetupInvariant (setup : Setup) : Prop :=
  (setup.seededDurable.map Result.mutant).Nodup ∧
    ∀ result ∈ setup.seededDurable, result.executed = false ∧ result.status ≠ .notRun

def StructuralInvariant (state : State) : Prop :=
  (state.accepted.map Result.mutant).Nodup ∧
    (state.durable.map Result.mutant).Nodup ∧
    (state.summarized.map Result.mutant).Nodup ∧
    (state.reported.map Result.mutant).Nodup ∧
    (∀ result ∈ state.accepted, result.executed = true ∧ result.status ≠ .notRun) ∧
    (∀ result ∈ state.durable, resultBacked state result) ∧
    (∀ result ∈ state.summarized, reportBacked state result) ∧
    (∀ result ∈ state.reported, result ∈ state.summarized) ∧
    (∀ result ∈ state.accepted,
      ¬containsMutant state.setup.seededDurable result.mutant)

def Invariant (state : State) : Prop :=
  StructuralInvariant state ∧
    state.summary = state.summarized.map Result.status ∧
    state.metricsExecuted = state.accepted.length

def normalize (state : State) : State := {
  state with
    summary := state.summarized.map Result.status
    metricsExecuted := state.accepted.length
}

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  rejection := some reason

def acceptState (state : State) : Verdict where
  state := normalize state
  rejection := none

def proposal (state : State) : Event → Except Rejection State
  | .discover mutant =>
      if mutant ∈ state.setup.discovered then
        .error .duplicate
      else
        .ok { state with
          setup := { state.setup with discovered := state.setup.discovered ++ [mutant] }
        }
  | .accept mutant status =>
      if state.stopped then
        .error .stopped
      else if !(mutant ∈ state.setup.discovered) then
        .error .unknownMutant
      else if status == .notRun then
        .error .invalidStatus
      else if containsMutant state.accepted mutant ||
          containsMutant state.setup.seededDurable mutant then
        .error .duplicate
      else
        .ok { state with
          accepted := state.accepted ++ [{ mutant, status, executed := true }]
        }
  | .persistOk mutant =>
      if !state.setup.session then
        .error .sessionDisabled
      else if containsMutant state.durable mutant then
        .error .duplicate
      else
        match findResult state.accepted mutant with
        | none => .error .resultMissing
        | some result => .ok { state with
            durable := state.durable ++ [result]
            persistenceFailures := state.persistenceFailures.erase mutant
          }
  | .persistFailed mutant =>
      if !state.setup.session then
        .error .sessionDisabled
      else if !containsMutant state.accepted mutant then
        .error .resultMissing
      else if mutant ∈ state.persistenceFailures then
        .error .duplicate
      else
        .ok { state with
          persistenceFailures := state.persistenceFailures ++ [mutant]
          diagnostics := state.diagnostics ++ [.persistenceFailed]
        }
  | .recordResult mutant =>
      if containsMutant state.summarized mutant then
        .error .duplicate
      else
        match currentOrDurable? state mutant with
        | none => .error .resultMissing
        | some result =>
            if state.setup.session && !state.stopped && containsMutant state.accepted mutant &&
                !(result ∈ state.durable) && !(mutant ∈ state.persistenceFailures) then
              .error .persistenceRequired
            else
              .ok { state with summarized := state.summarized ++ [result] }
  | .reportOk mutant =>
      if containsMutant state.reported mutant then
        .error .duplicate
      else
        match findResult state.summarized mutant with
        | none => .error .resultMissing
        | some result => .ok { state with reported := state.reported ++ [result] }
  | .reportFailed mutant =>
      if (currentOrDurable? state mutant).isNone then
        .error .resultMissing
      else
        .ok { state with diagnostics := state.diagnostics ++ [.reportFailed] }
  | .stop =>
      if state.stopped then .error .duplicate else .ok { state with stopped := true }
  | .markNotRun mutant =>
      if !state.stopped then
        .error .stopped
      else if !(mutant ∈ state.setup.discovered) then
        .error .unknownMutant
      else if containsMutant state.accepted mutant || containsMutant state.durable mutant ||
          containsMutant state.summarized mutant || containsMutant state.reported mutant then
        .error .duplicate
      else
        let result : Result := { mutant, status := .notRun, executed := false }
        .ok { state with
          summarized := state.summarized ++ [result]
          reported := state.reported ++ [result]
        }
  | .finishSession complete =>
      if !state.setup.session then
        .error .sessionDisabled
      else if state.sessionFinished then
        .error .alreadyFinished
      else if complete && (state.stopped || !runFailureFree state || !completeCoverage state ||
          !resultsConclusive state) then
        .error .finalizationRequired
      else
        .ok { state with sessionFinished := true, sessionComplete := complete }
  | .finishMetrics =>
      if !state.setup.metrics then
        .error .alreadyFinished
      else if state.metricsFinished then
        .error .alreadyFinished
      else
        .ok { state with metricsFinished := true }
  | .metricsFailed =>
      if !state.setup.metrics then
        .error .alreadyFinished
      else if state.metricsFinished then
        .error .alreadyFinished
      else
        .ok { state with
          metricsFinished := true
          diagnostics := state.diagnostics ++ [.metricsFailed]
        }
  | .returnRun =>
      if state.returned then
        .error .alreadyFinished
      else if (state.setup.session && !state.sessionFinished) ||
          (state.setup.metrics && !state.metricsFinished) then
        .error .finalizationRequired
      else
        let complete := !state.stopped && runFailureFree state && completeCoverage state &&
          resultsConclusive state && (!state.setup.session || state.sessionComplete)
        .ok { state with returned := true, complete }

def step (state : State) (event : Event) : Verdict :=
  if state.returned then
    reject state .alreadyFinished
  else if (state.sessionFinished || state.metricsFinished) && event.isLifecycleMutation then
    reject state .alreadyFinished
  else
    match proposal state event with
    | .ok next => acceptState next
    | .error reason => reject state reason

def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

def run : State → List Event → State := runWith step

def alterAcceptedStatus (state : State) (mutant : Mutant) (status : Status) : State :=
  { state with accepted := state.accepted.map fun result =>
      if result.mutant == mutant then { result with status } else result }

def brokenAtomicity (state : State) (event : Event) : Verdict :=
  match event with
  | .persistFailed mutant =>
      let verdict := step state event
      { verdict with state := { verdict.state with
          accepted := eraseMutant verdict.state.accepted mutant
        } }
  | _ => step state event

def brokenUniqueness (state : State) (event : Event) : Verdict :=
  match event with
  | .reportOk mutant =>
      match findResult state.summarized mutant with
      | some result => acceptState { state with reported := state.reported ++ [result] }
      | none => step state event
  | _ => step state event

def brokenBoundary (state : State) (event : Event) : Verdict :=
  match event with
  | .stop =>
      let changed := state.accepted.foldl
        (fun current result => alterAcceptedStatus current result.mutant .notRun) state
      acceptState { changed with stopped := true }
  | _ => step state event

def divergentStatus : Status → Status
  | .killed => .survived
  | _ => .killed

def brokenCrossSurface (state : State) (event : Event) : Verdict :=
  match event with
  | .persistOk mutant =>
      let verdict := step state event
      match findResult verdict.state.durable mutant with
      | none => verdict
      | some result => { verdict with state := { verdict.state with
          durable := upsertResult verdict.state.durable
            { result with status := divergentStatus result.status }
        } }
  | _ => step state event

def brokenMetrics (state : State) (event : Event) : Verdict :=
  match event with
  | .markNotRun _ =>
      let verdict := step state event
      if verdict.rejection.isNone then
        { verdict with state := { verdict.state with
            metricsExecuted := verdict.state.metricsExecuted + 1
          } }
      else verdict
  | _ => step state event

end HoiminOracle.ResultLifecycle
