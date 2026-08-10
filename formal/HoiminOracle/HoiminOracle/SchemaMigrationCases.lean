import HoiminOracle.SchemaMigrationProofs

namespace HoiminOracle.SchemaMigration

def actors : List Actor := [.left, .right]

def events : List Event := actors.flatMap fun actor => [
  .observe actor, .begin actor, .migrate actor, .commit actor, .fail actor
]

def phaseIsSuccessful (item : Phase) : Bool := item == .succeeded

def stateSafe (state : State) : Bool :=
  (!phaseIsSuccessful state.leftPhase || state.version == .current) &&
  (!phaseIsSuccessful state.rightPhase || state.version == .current) &&
  (state.lock.isNone == state.txVersion.isNone) &&
  match state.lock with
  | none => true
  | some actor => phase state actor == .locked

def expand (states : List State) : List State :=
  (states.flatMap fun state => events.map fun event => step state event).eraseDups

def reachableStates : Nat → List State → List State
  | 0, states => states.eraseDups
  | depth + 1, states => reachableStates depth (expand states)

def auditDepth : Nat := 9
def stateCeiling : Nat := 128

def auditInitials : List State := [
  initial .fresh false,
  initial .v1 true,
  initial .future true
]

def auditedStates : List State := reachableStates auditDepth auditInitials

def finiteAuditPasses : Bool :=
  auditedStates.length ≤ stateCeiling && auditedStates.all stateSafe

def brokenFailPublishes (state : State) (actor : Actor) : State :=
  if state.lock == some actor && phase state actor == .locked then
    match state.txVersion with
    | some staged =>
        setPhase {
          state with version := staged, lock := none, txVersion := none
        } actor .migrationFailed
    | none => state
  else state

def brokenBeginUsesObserved (state : State) (actor : Actor) : State :=
  match phase state actor, state.lock with
  | .waiting observed, none =>
      match observed with
      | .future => setPhase state actor .futureRejected
      | version =>
          setPhase { state with lock := some actor, txVersion := some version } actor .locked
  | _, _ => state

def brokenBeginMigratesFuture (state : State) (actor : Actor) : State :=
  match phase state actor, state.lock with
  | .waiting .future, none =>
      setPhase { state with lock := some actor, txVersion := some .v1 } actor .locked
  | _, _ => begin state actor

def brokenStepStale (state : State) : Event → State
  | .begin actor => brokenBeginUsesObserved state actor
  | event => step state event

def brokenStepFuture (state : State) : Event → State
  | .begin actor => brokenBeginMigratesFuture state actor
  | event => step state event

def runWith (next : State → Event → State) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event) rest

def freshConcurrentTrace : List Event := [
  .observe .left, .observe .right,
  .begin .left, .migrate .left, .migrate .left, .migrate .left, .commit .left,
  .begin .right, .commit .right
]

def v1ConcurrentTrace : List Event := [
  .observe .left, .observe .right,
  .begin .left, .migrate .left, .migrate .left, .commit .left,
  .begin .right, .commit .right
]

def staleReadTrace : List Event := [
  .observe .left, .observe .right,
  .begin .left, .migrate .left, .migrate .left, .commit .left,
  .begin .right, .migrate .right, .commit .right
]

def rollbackTrace : List Event := [
  .observe .left, .begin .left, .migrate .left, .fail .left
]

def futureTrace : List Event := [
  .observe .left, .begin .left
]

def futureBrokenTrace : List Event := [
  .observe .left, .begin .left,
  .migrate .left, .migrate .left, .commit .left
]

def atomicitySensitivity : Bool :=
  let before := initial .v1 true
  let staged := run before [.observe .left, .begin .left, .migrate .left]
  (fail staged .left).version == .v1 &&
    (brokenFailPublishes staged .left).version == .v2

def staleReadSensitivity : Bool :=
  let before := initial .v1 true
  let correct := run before staleReadTrace
  let broken := runWith brokenStepStale before staleReadTrace
  correct.leftPhase == .succeeded && correct.rightPhase == .succeeded &&
    broken.leftPhase == .succeeded && broken.rightPhase == .migrationFailed

def futurePrecedenceSensitivity : Bool :=
  let before := initial .future true
  let correct := run before futureBrokenTrace
  let broken := runWith brokenStepFuture before futureBrokenTrace
  correct.version == .future && correct.leftPhase == .futureRejected &&
    broken.version == .future && broken.leftPhase == .migrationFailed

def sensitivityPasses : Bool :=
  atomicitySensitivity && staleReadSensitivity && futurePrecedenceSensitivity

structure OracleCase where
  schema : Nat
  id : String
  mode : String
  scenario : String
  initialVersion : Version
  legacyRow : Bool
  trace : List Event
  deriving Repr, DecidableEq, BEq

def cases : List OracleCase := [
  { schema := 1, id := "concurrent_fresh_open", mode := "strict",
    scenario := "concurrent_fresh", initialVersion := .fresh,
    legacyRow := false, trace := freshConcurrentTrace },
  { schema := 1, id := "concurrent_v1_upgrade", mode := "strict",
    scenario := "concurrent_v1", initialVersion := .v1,
    legacyRow := true, trace := v1ConcurrentTrace },
  { schema := 1, id := "forced_stale_reread", mode := "model-only",
    scenario := "stale_reread", initialVersion := .v1,
    legacyRow := true, trace := staleReadTrace },
  { schema := 1, id := "failed_v1_upgrade_rollback", mode := "strict",
    scenario := "rollback", initialVersion := .v1,
    legacyRow := true, trace := rollbackTrace },
  { schema := 1, id := "future_version_precedence", mode := "strict",
    scenario := "future", initialVersion := .future,
    legacyRow := true, trace := futureTrace }
]

def caseResult (item : OracleCase) : State :=
  run (initial item.initialVersion item.legacyRow) item.trace

def caseSafe (item : OracleCase) : Bool :=
  let result := caseResult item
  stateSafe result && result.legacyRow == item.legacyRow

def fixedCasesPass : Bool := cases.all caseSafe

end HoiminOracle.SchemaMigration
