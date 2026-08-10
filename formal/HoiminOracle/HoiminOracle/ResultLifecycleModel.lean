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
  | reportOk (mutant : Mutant)
  | reportFailed (mutant : Mutant)
  | stop
  | markNotRun (mutant : Mutant)
  | finishSession (complete : Bool)
  | finishMetrics
  | metricsFailed
  | returnRun
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Rejection
  | unknownMutant
  | duplicate
  | stopped
  | invalidStatus
  | resultMissing
  | persistenceRequired
  | sessionDisabled
  | alreadyFinished
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

def safe (state : State) : Bool :=
  identitiesUnique state.accepted &&
    identitiesUnique state.durable &&
    identitiesUnique state.reported &&
    (state.accepted.all fun result => result.executed && result.status != .notRun) &&
    (state.durable.all fun result => resultBacked state result) &&
    (state.reported.all fun result => reportBacked state result) &&
    state.summary == state.reported.map Result.status &&
    state.metricsExecuted == state.accepted.length &&
    (!state.complete || completeCoverage state) &&
    (!state.sessionComplete || state.sessionFinished) &&
    (!state.returned || state.complete ==
      (!state.stopped && state.diagnostics.isEmpty && completeCoverage state))

def Invariant (state : State) : Prop :=
  state.summary = state.reported.map Result.status ∧
    state.metricsExecuted = state.accepted.length

def normalize (state : State) : State := {
  state with
    summary := state.reported.map Result.status
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
      else if containsMutant state.accepted mutant then
        .error .duplicate
      else
        .ok { state with
          accepted := state.accepted ++ [{ mutant, status, executed := true }]
        }
  | .persistOk mutant =>
      if !state.setup.session then
        .error .sessionDisabled
      else
        match findResult state.accepted mutant with
        | none => .error .resultMissing
        | some result => .ok { state with
            durable := upsertResult state.durable result
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
  | .reportOk mutant =>
      if containsMutant state.reported mutant then
        .error .duplicate
      else
        match currentOrDurable? state mutant with
        | none => .error .resultMissing
        | some result =>
            if state.setup.session && containsMutant state.accepted mutant &&
                !(result ∈ state.durable) && !(mutant ∈ state.persistenceFailures) then
              .error .persistenceRequired
            else
              .ok { state with reported := state.reported ++ [result] }
  | .reportFailed mutant =>
      if (currentOrDurable? state mutant).isNone then
        .error .resultMissing
      else
        .ok { state with diagnostics := state.diagnostics ++ [.reportFailed] }
  | .stop =>
      if state.stopped then .error .duplicate else .ok { state with stopped := true }
  | .markNotRun mutant =>
      if !(mutant ∈ state.setup.discovered) then
        .error .unknownMutant
      else if containsMutant state.accepted mutant || containsMutant state.reported mutant then
        .error .duplicate
      else
        .ok { state with
          reported := state.reported ++ [{ mutant, status := .notRun, executed := false }]
        }
  | .finishSession complete =>
      if !state.setup.session then
        .error .sessionDisabled
      else if state.sessionFinished then
        .error .alreadyFinished
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
      else
        .ok { state with diagnostics := state.diagnostics ++ [.metricsFailed] }
  | .returnRun =>
      if state.returned then
        .error .alreadyFinished
      else
        let complete := !state.stopped && state.diagnostics.isEmpty && completeCoverage state
        .ok { state with returned := true, complete }

def step (state : State) (event : Event) : Verdict :=
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
      match currentOrDurable? state mutant with
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
      { verdict with state := { verdict.state with
          metricsExecuted := verdict.state.metricsExecuted + 1
        } }
  | _ => step state event

def allMutants : List Mutant := [.m0, .m1]
def representativeStatuses : List Status := [.killed, .survived, .timeout, .error]

def eventAlphabet : List Event :=
  [.stop, .finishSession true, .finishSession false, .finishMetrics,
    .metricsFailed, .returnRun] ++
  allMutants.flatMap fun mutant =>
    [.discover mutant, .persistOk mutant, .persistFailed mutant,
      .reportOk mutant, .reportFailed mutant, .markNotRun mutant] ++
    representativeStatuses.map fun status => .accept mutant status

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq, BEq

def containsState (items : List Reachable) (state : State) : Bool :=
  items.any fun item => item.state == state

def successors (next : State → Event → Verdict) (item : Reachable) : List Reachable :=
  eventAlphabet.map fun event => {
    trace := item.trace ++ [event]
    state := (next item.state event).state
  }

def uniqueNewStates (seen candidates : List Reachable) : List Reachable :=
  candidates.foldl (fun retained candidate =>
    if containsState seen candidate.state || containsState retained candidate.state then
      retained
    else
      retained ++ [candidate]) []

def explorationLayersWith
    (next : State → Event → Verdict) :
    Nat → List Reachable → List Reachable → List (List Reachable)
  | 0, _, frontier => [frontier]
  | depth + 1, seen, frontier =>
      let following := uniqueNewStates seen (frontier.flatMap (successors next))
      frontier :: explorationLayersWith next depth (seen ++ following) following

def reachableUpTo
    (next : State → Event → Verdict)
    (setup : Setup)
    (depth : Nat) : List Reachable :=
  let start : Reachable := { trace := [], state := State.initial setup }
  (explorationLayersWith next depth [start] [start]).flatten

def firstCounterexample?
    (next : State → Event → Verdict)
    (setup : Setup)
    (depth : Nat) : Option Reachable :=
  (reachableUpTo next setup depth).find? fun item => !(safe item.state)

end HoiminOracle.ResultLifecycle
