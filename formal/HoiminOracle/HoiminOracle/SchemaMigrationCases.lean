import HoiminOracle.SchemaMigrationProofs

namespace HoiminOracle.SchemaMigration

def actors : List Actor := [.left, .right]

def events : List Event := actors.flatMap fun actor => [
  .observe actor, .begin actor, .initializeSchema actor, .commit actor, .fail actor
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

def auditDepth : Nat := 7
def stateCeiling : Nat := 256

def auditInitials : List State := [
  initial .fresh false, initial .fresh true,
  initial .v1 true, initial .v2 true, initial .v3 true, initial .v4 true,
  initial .current true, initial .future true
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
        } actor .initializationFailed
    | none => state
  else state

def brokenBeginUsesObserved (state : State) (actor : Actor) : State :=
  match phase state actor, state.lock with
  | .waiting observed, none =>
      match observed with
      | .fresh | .current =>
          setPhase { state with lock := some actor, txVersion := some observed } actor .locked
      | .v1 | .v2 | .v3 | .v4 | .future => setPhase state actor .unsupportedRejected
  | _, _ => state

def brokenBeginAcceptsUnsupported (state : State) (actor : Actor) : State :=
  match phase state actor, state.lock with
  | .waiting _, none =>
      setPhase { state with lock := some actor, txVersion := some .fresh } actor .locked
  | _, _ => begin state actor

def brokenStepStale (state : State) : Event → State
  | .begin actor => brokenBeginUsesObserved state actor
  | event => step state event

def brokenStepUnsupported (state : State) : Event → State
  | .begin actor => brokenBeginAcceptsUnsupported state actor
  | event => step state event

def runWith (next : State → Event → State) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event) rest

def freshConcurrentTrace : List Event := [
  .observe .left, .observe .right,
  .begin .left, .initializeSchema .left, .commit .left,
  .begin .right, .commit .right
]

def staleReadTrace : List Event := [
  .observe .left, .observe .right,
  .begin .left, .initializeSchema .left, .commit .left,
  .begin .right, .initializeSchema .right, .commit .right
]

def rollbackTrace : List Event := [
  .observe .left, .begin .left, .initializeSchema .left, .fail .left
]

def unsupportedTrace : List Event := [.observe .left, .begin .left]

def unsupportedBrokenTrace : List Event := [
  .observe .left, .begin .left, .initializeSchema .left, .commit .left
]

def atomicitySensitivity : Bool :=
  let before := initial .fresh true
  let staged := run before [.observe .left, .begin .left, .initializeSchema .left]
  (fail staged .left).version == .fresh &&
    (brokenFailPublishes staged .left).version == .current

def staleReadSensitivity : Bool :=
  let before := initial .fresh false
  let correct := run before staleReadTrace
  let broken := runWith brokenStepStale before staleReadTrace
  correct.leftPhase == .succeeded && correct.rightPhase == .succeeded &&
    broken.leftPhase == .succeeded && broken.rightPhase == .initializationFailed

def unsupportedPrecedenceSensitivity : Bool :=
  ([Version.v1, .v2, .v3, .v4, .future]).all fun version =>
    let before := initial version true
    let correct := run before unsupportedBrokenTrace
    let broken := runWith brokenStepUnsupported before unsupportedBrokenTrace
    correct.version == version && correct.leftPhase == .unsupportedRejected &&
      broken.version == version && broken.leftPhase == .initializationFailed

def sensitivityPasses : Bool :=
  atomicitySensitivity && staleReadSensitivity && unsupportedPrecedenceSensitivity

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
  { schema := 1, id := "forced_stale_fresh_reread", mode := "model-only",
    scenario := "stale_reread", initialVersion := .fresh,
    legacyRow := false, trace := staleReadTrace },
  -- The transaction model retains rollback, but the old public fixture contains
  -- foreign schema objects and is now rejected before initialization. Actual
  -- SQLITE_FULL rollback is checked separately at the production schema owner.
  { schema := 1, id := "failed_fresh_initialization_rollback", mode := "model-only",
    scenario := "rollback", initialVersion := .fresh,
    legacyRow := true, trace := rollbackTrace }
] ++ ([Version.v1, .v2, .v3, .v4, .future]).map fun version =>
  { schema := 1, id := match version with
      | .v1 => "reject_schema_v1"
      | .v2 => "reject_schema_v2"
      | .v3 => "reject_schema_v3"
      | .v4 => "reject_schema_v4"
      | _ => "reject_future_schema",
    mode := "strict", scenario := "unsupported", initialVersion := version,
    legacyRow := true, trace := unsupportedTrace }

def caseResult (item : OracleCase) : State :=
  run (initial item.initialVersion item.legacyRow) item.trace

def caseSafe (item : OracleCase) : Bool :=
  let result := caseResult item
  stateSafe result && result.legacyRow == item.legacyRow

def fixedCasesPass : Bool := cases.all caseSafe

end HoiminOracle.SchemaMigration
