import Std

namespace HoiminOracle.SessionAudit

inductive Handler
  | h0
  | h1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Run
  | r0
  | r1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Fingerprint
  | f0
  | f1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Mutant
  | m0
  | m1
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Payload
  | p0
  | p1
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

namespace Status

def isDeterminate : Status → Bool
  | .killed | .survived => true
  | .timeout | .outOfMemory | .processLimit | .error | .notRun => false

end Status

inductive HandlerState
  | closed
  | live
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure RunRow where
  run : Run
  fingerprint : Fingerprint
  ordinal : Nat
  finished : Bool
  complete : Bool
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

structure ResultRow where
  run : Run
  mutant : Mutant
  status : Status
  payload : Payload
  deriving Repr, DecidableEq, BEq

inductive Pending
  | loadCandidate (handler : Handler) (fingerprint : Fingerprint) (run : Run)
  | loadLocked (handler : Handler) (fingerprint : Fingerprint) (run : Run)
  | replacing (handler : Handler) (old : ResultRow) (replacement : ResultRow)
  deriving Repr, DecidableEq, BEq

structure State where
  handlers : List (Handler × HandlerState)
  runs : List RunRow
  results : List ResultRow
  owners : List (Run × Handler)
  nextOrdinal : Nat
  pending : Option Pending
  deriving Repr, DecidableEq, BEq

namespace State

def initial : State where
  handlers := [(.h0, .closed), (.h1, .closed)]
  runs := []
  results := []
  owners := []
  nextOrdinal := 0
  pending := none

end State

inductive PersistValidity
  | valid
  | invalidDiagnostic
  deriving Repr, DecidableEq, BEq

inductive Event
  | open (handler : Handler)
  | begin (handler : Handler) (run : Run) (fingerprint : Fingerprint)
  | load (handler : Handler) (fingerprint : Fingerprint)
  | lookup (handler : Handler) (run : Run) (mutant : Mutant)
  | persist (handler : Handler) (run : Run) (mutant : Mutant)
      (status : Status) (payload : Payload) (validity : PersistValidity)
  | finish (handler : Handler) (run : Run) (complete : Bool)
  | drop (handler : Handler)
  | crash (handler : Handler)
  | loadReadCandidate (handler : Handler) (fingerprint : Fingerprint)
  | loadAcquire (handler : Handler)
  | loadRecheck (handler : Handler)
  | replacementDelete (handler : Handler) (run : Run) (mutant : Mutant)
      (status : Status) (payload : Payload)
  | replacementCommit (handler : Handler)
  | replacementRollback (handler : Handler)
  deriving Repr, DecidableEq, BEq

namespace Event

def isPublic : Event → Bool
  | .open _ | .begin _ _ _ | .load _ _ | .lookup _ _ _
  | .persist _ _ _ _ _ _ | .finish _ _ _ | .drop _ | .crash _ => true
  | .loadReadCandidate _ _ | .loadAcquire _ | .loadRecheck _
  | .replacementDelete _ _ _ _ _ | .replacementCommit _
  | .replacementRollback _ => false

end Event

inductive Rejection
  | handlerClosed
  | duplicateRun
  | active
  | missingRun
  | completeRun
  | duplicateResult
  | invalidDiagnostic
  | noCandidate
  | internalState
  deriving Repr, DecidableEq, BEq

structure Verdict where
  state : State
  rejection : Option Rejection
  selectedRun : Option Run
  storedResult : Option ResultRow
  deriving Repr, DecidableEq, BEq

structure Observation where
  event : Event
  verdict : String
  errorCode : Option String
  selectedRun : Option Run
  storedResult : Option ResultRow
  runs : List RunRow
  results : List ResultRow
  owners : List (Run × Handler)
  deriving Repr, DecidableEq, BEq

def allHandlers : List Handler := [.h0, .h1]

def allRuns : List Run := [.r0, .r1]

def handlerState (state : State) (handler : Handler) : Option HandlerState :=
  (state.handlers.find? fun entry => entry.1 == handler).map Prod.snd

def handlerLive (state : State) (handler : Handler) : Bool :=
  handlerState state handler == some .live

def setHandler (state : State) (handler : Handler) (value : HandlerState) : State :=
  { state with handlers := state.handlers.map fun entry =>
      if entry.1 == handler then (handler, value) else entry }

def findRun (state : State) (run : Run) : Option RunRow :=
  state.runs.find? fun row => row.run == run

def runExists (state : State) (run : Run) : Bool :=
  (findRun state run).isSome

def runComplete (state : State) (run : Run) : Bool :=
  (findRun state run).any RunRow.complete

def replaceRun (state : State) (replacement : RunRow) : State :=
  { state with runs := state.runs.map fun row =>
      if row.run == replacement.run then replacement else row }

def findResult (state : State) (run : Run) (mutant : Mutant) : Option ResultRow :=
  state.results.find? fun row => row.run == run && row.mutant == mutant

def eraseResult (results : List ResultRow) (run : Run) (mutant : Mutant) : List ResultRow :=
  results.filter fun row => !(row.run == run && row.mutant == mutant)

def replaceResult (state : State) (replacement : ResultRow) : State :=
  { state with results :=
      eraseResult state.results replacement.run replacement.mutant ++ [replacement] }

def ownerCount (state : State) (run : Run) : Nat :=
  (state.owners.filter fun owner => owner.1 == run).length

def ownerRuns (state : State) (handler : Handler) : List Run :=
  (state.owners.filter fun owner => owner.2 == handler).map Prod.fst

def owns (state : State) (run : Run) (handler : Handler) : Bool :=
  state.owners.any fun owner => owner.1 == run && owner.2 == handler

def releaseHandler (state : State) (handler : Handler) : State :=
  { state with owners := state.owners.filter fun owner => owner.2 != handler }

def releaseRun (state : State) (run : Run) : State :=
  { state with owners := state.owners.filter fun owner => owner.1 != run }

def addOwner (state : State) (run : Run) (handler : Handler) : State :=
  if owns state run handler then state
  else { state with owners := state.owners ++ [(run, handler)] }

def resumeEligible (row : RunRow) (fingerprint : Fingerprint) : Bool :=
  row.fingerprint == fingerprint && !row.complete

def newer (left right : RunRow) : RunRow :=
  if left.ordinal < right.ordinal then right else left

def latestCompatible (state : State) (fingerprint : Fingerprint) : Option RunRow :=
  (state.runs.filter fun row => resumeEligible row fingerprint).foldl
    (fun selected row => some (selected.map (newer · row) |>.getD row)) none

def pendingHandler : Pending → Handler
  | .loadCandidate handler _ _ => handler
  | .loadLocked handler _ _ => handler
  | .replacing handler _ _ => handler

def clearPendingFor (state : State) (handler : Handler) : State :=
  match state.pending with
  | some pending =>
      if pendingHandler pending == handler then { state with pending := none } else state
  | none => state

def durable (state : State) : List RunRow × List ResultRow :=
  (state.runs, state.results)

def SameExceptResult (before after : State) (replacement : ResultRow) : Prop :=
  after = replaceResult before replacement

def pendingValid (state : State) : Bool :=
  match state.pending with
  | none => true
  | some (.loadCandidate handler fingerprint run)
  | some (.loadLocked handler fingerprint run) =>
      handlerLive state handler &&
        (findRun state run).any fun row => row.fingerprint == fingerprint
  | some (.replacing handler old replacement) =>
      handlerLive state handler && old.run == replacement.run &&
        old.mutant == replacement.mutant &&
        findResult state old.run old.mutant == some old &&
        !old.status.isDeterminate

def safe (state : State) : Bool :=
  decide (state.handlers.map Prod.fst).Nodup &&
    allHandlers.all fun handler => (handlerState state handler).isSome &&
  decide (state.runs.map RunRow.run).Nodup &&
  decide (state.runs.map RunRow.ordinal).Nodup &&
  state.runs.all fun row =>
    row.ordinal < state.nextOrdinal && (!row.complete || row.finished) &&
  decide (state.results.map fun row => (row.run, row.mutant)).Nodup &&
  state.results.all fun row => runExists state row.run &&
  decide state.owners.Nodup &&
  allRuns.all fun run => ownerCount state run ≤ 1 &&
  state.owners.all fun owner =>
    runExists state owner.1 && handlerLive state owner.2 && !runComplete state owner.1 &&
  pendingValid state

def Invariant (state : State) : Prop :=
  state.nextOrdinal = state.runs.length

def rejectionCode (event : Event) : Rejection → String
  | .handlerClosed => "session.handler.closed"
  | .duplicateRun => "session.begin"
  | .active => "session.resume.active"
  | .missingRun =>
      match event with
      | .persist _ _ _ _ _ _ => "session.commit"
      | .finish _ _ _ => "session.finish.state"
      | _ => "session.lookup.state"
  | .completeRun =>
      match event with
      | .lookup _ _ _ => "session.lookup.complete"
      | .persist _ _ _ _ _ _ => "session.persist.complete"
      | .finish _ _ _ => "session.finish.state"
      | _ => "session.complete"
  | .duplicateResult => "session.duplicate_result"
  | .invalidDiagnostic => "session.persist.diagnostic"
  | .noCandidate => "session.model.no_candidate"
  | .internalState => "session.model.internal_state"

def reject (state : State) (reason : Rejection) : Verdict where
  state := state
  rejection := some reason
  selectedRun := none
  storedResult := none

def accept (state : State) (selectedRun : Option Run := none)
    (storedResult : Option ResultRow := none) : Verdict where
  state := state
  rejection := none
  selectedRun := selectedRun
  storedResult := storedResult

def observe (event : Event) (verdict : Verdict) : Observation where
  event := event
  verdict := if verdict.rejection.isSome then "rejected" else "accepted"
  errorCode := verdict.rejection.map (rejectionCode event)
  selectedRun := verdict.selectedRun
  storedResult := verdict.storedResult
  runs := verdict.state.runs
  results := verdict.state.results
  owners := verdict.state.owners

def openHandler (state : State) (handler : Handler) : Verdict :=
  accept (setHandler state handler .live)

def beginRun (state : State) (handler : Handler) (run : Run)
    (fingerprint : Fingerprint) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else if runExists state run then
    reject state .duplicateRun
  else
    let row : RunRow := {
      run := run
      fingerprint := fingerprint
      ordinal := state.nextOrdinal
      finished := false
      complete := false
    }
    accept {
      state with
        runs := state.runs ++ [row]
        owners := state.owners ++ [(run, handler)]
        nextOrdinal := state.nextOrdinal + 1
    } (some run)

def loadRun (state : State) (handler : Handler)
    (fingerprint : Fingerprint) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else
    match latestCompatible state fingerprint with
    | none => accept state
    | some row =>
        if owns state row.run handler then
          accept state (some row.run)
        else if ownerCount state row.run > 0 then
          reject state .active
        else
          accept (addOwner state row.run handler) (some row.run)

def lookupResult (state : State) (handler : Handler) (run : Run)
    (mutant : Mutant) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else
    match findRun state run with
    | none => reject state .missingRun
    | some row =>
        if row.complete then reject state .completeRun
        else accept state none (findResult state run mutant)

def persistResult (state : State) (handler : Handler) (run : Run)
    (mutant : Mutant) (status : Status) (payload : Payload)
    (validity : PersistValidity) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else
    match findRun state run with
    | none => reject state .missingRun
    | some runRow =>
        if runRow.complete then
          reject state .completeRun
        else
          let replacement : ResultRow := { run, mutant, status, payload }
          match findResult state run mutant with
          | some old =>
              if old.status.isDeterminate then
                reject state .duplicateResult
              else
                match validity with
                | .invalidDiagnostic => reject state .invalidDiagnostic
                | .valid => accept (replaceResult state replacement) none (some replacement)
          | none =>
              match validity with
              | .invalidDiagnostic => reject state .invalidDiagnostic
              | .valid => accept (replaceResult state replacement) none (some replacement)

def finishRun (state : State) (handler : Handler) (run : Run)
    (complete : Bool) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else
    match findRun state run with
    | none => reject state .missingRun
    | some row =>
        if row.complete then
          reject state .completeRun
        else
          let next := replaceRun (releaseRun state run) {
            row with finished := true, complete := complete
          }
          accept next (some run)

def stopHandler (state : State) (handler : Handler) : Verdict :=
  let next := clearPendingFor (releaseHandler state handler) handler
  accept (setHandler next handler .closed)

def readLoadCandidate (state : State) (handler : Handler)
    (fingerprint : Fingerprint) : Verdict :=
  if !handlerLive state handler then
    reject state .handlerClosed
  else if state.pending.isSome then
    reject state .internalState
  else
    match latestCompatible state fingerprint with
    | none => reject state .noCandidate
    | some row => accept { state with pending := some (.loadCandidate handler fingerprint row.run) }

def acquireLoad (state : State) (handler : Handler) : Verdict :=
  match state.pending with
  | some (.loadCandidate pendingHandler fingerprint run) =>
      if pendingHandler != handler then
        reject state .internalState
      else if owns state run handler then
        accept { state with pending := some (.loadLocked handler fingerprint run) }
      else if ownerCount state run > 0 then
        reject { state with pending := none } .active
      else
        accept { state with pending := some (.loadLocked handler fingerprint run) }
  | _ => reject state .internalState

def recheckLoad (state : State) (handler : Handler) : Verdict :=
  match state.pending with
  | some (.loadLocked pendingHandler fingerprint run) =>
      if pendingHandler != handler then
        reject state .internalState
      else
        match findRun state run with
        | some row =>
            if resumeEligible row fingerprint then
              if owns state run handler then
                accept { state with pending := none } (some run)
              else if ownerCount state run > 0 then
                reject { state with pending := none } .active
              else
                accept (addOwner { state with pending := none } run handler) (some run)
            else
              accept { state with pending := none }
        | none => accept { state with pending := none }
  | _ => reject state .internalState

def startReplacement (state : State) (handler : Handler) (run : Run)
    (mutant : Mutant) (status : Status) (payload : Payload) : Verdict :=
  if !handlerLive state handler || state.pending.isSome then
    reject state .internalState
  else
    match findResult state run mutant with
    | some old =>
        if old.status.isDeterminate then
          reject state .duplicateResult
        else
          let replacement : ResultRow := { run, mutant, status, payload }
          accept { state with pending := some (.replacing handler old replacement) }
    | none => reject state .missingRun

def commitReplacement (state : State) (handler : Handler) : Verdict :=
  match state.pending with
  | some (.replacing pendingHandler _ replacement) =>
      if pendingHandler != handler then
        reject state .internalState
      else
        accept { replaceResult state replacement with pending := none } none (some replacement)
  | _ => reject state .internalState

def rollbackReplacement (state : State) (handler : Handler) : Verdict :=
  match state.pending with
  | some (.replacing pendingHandler _ _) =>
      if pendingHandler != handler then reject state .internalState
      else accept { state with pending := none }
  | _ => reject state .internalState

def step (state : State) : Event → Verdict
  | .open handler => openHandler state handler
  | .begin handler run fingerprint => beginRun state handler run fingerprint
  | .load handler fingerprint => loadRun state handler fingerprint
  | .lookup handler run mutant => lookupResult state handler run mutant
  | .persist handler run mutant status payload validity =>
      persistResult state handler run mutant status payload validity
  | .finish handler run complete => finishRun state handler run complete
  | .drop handler => stopHandler state handler
  | .crash handler => stopHandler state handler
  | .loadReadCandidate handler fingerprint => readLoadCandidate state handler fingerprint
  | .loadAcquire handler => acquireLoad state handler
  | .loadRecheck handler => recheckLoad state handler
  | .replacementDelete handler run mutant status payload =>
      startReplacement state handler run mutant status payload
  | .replacementCommit handler => commitReplacement state handler
  | .replacementRollback handler => rollbackReplacement state handler

def run : State → List Event → State
  | state, [] => state
  | state, event :: rest => run (step state event).state rest

def runVerdicts : State → List Event → List Verdict
  | _, [] => []
  | state, event :: rest =>
      let verdict := step state event
      verdict :: runVerdicts verdict.state rest

def publicEvents : List Event := [
  .open .h0,
  .open .h1,
  .begin .h0 .r0 .f0,
  .begin .h1 .r1 .f1,
  .load .h0 .f0,
  .load .h1 .f1,
  .lookup .h0 .r0 .m0,
  .persist .h0 .r0 .m0 .killed .p0 .valid,
  .finish .h0 .r0 false,
  .finish .h0 .r0 true,
  .drop .h0,
  .crash .h1
]

def brokenAtomicityStep (state : State) (event : Event) : Verdict :=
  match event with
  | .persist handler run mutant _ _ .invalidDiagnostic =>
      match findResult state run mutant with
      | some old =>
          if !handlerLive state handler then reject state .handlerClosed
          else if old.status.isDeterminate then reject state .duplicateResult
          else reject { state with results := eraseResult state.results run mutant }
            .invalidDiagnostic
      | none => step state event
  | _ => step state event

def brokenUniquenessStep (state : State) (event : Event) : Verdict :=
  match event with
  | .load handler fingerprint =>
      if !handlerLive state handler then reject state .handlerClosed
      else
        match latestCompatible state fingerprint with
        | none => accept state
        | some row => accept {
            state with owners := state.owners ++ [(row.run, handler)]
          } (some row.run)
  | _ => step state event

def brokenBoundaryStep (state : State) (event : Event) : Verdict :=
  match event with
  | .loadRecheck handler =>
      match state.pending with
      | some (.loadLocked pendingHandler _ run) =>
          if pendingHandler != handler then reject state .internalState
          else accept (addOwner { state with pending := none } run handler) (some run)
      | _ => reject state .internalState
  | _ => step state event

def atomicityWitness : List Event := [
  .open .h0,
  .begin .h0 .r0 .f0,
  .persist .h0 .r0 .m0 .timeout .p0 .valid,
  .persist .h0 .r0 .m0 .killed .p1 .invalidDiagnostic
]

def uniquenessWitness : List Event := [
  .open .h0,
  .open .h1,
  .begin .h0 .r0 .f0,
  .load .h1 .f0
]

def boundaryWitness : List Event := [
  .open .h0,
  .open .h1,
  .begin .h0 .r0 .f0,
  .loadReadCandidate .h1 .f0,
  .finish .h0 .r0 true,
  .loadAcquire .h1,
  .loadRecheck .h1
]

end HoiminOracle.SessionAudit
