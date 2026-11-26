// Ordered selection [3, 1] with two idle workers. The model ends at verification
// or the failure diagnostic after draining candidates as NotRun.
module OrderedDrainModel {
  const DRAIN_UNDISCOVERED: bool

  type Phase = Scanning | Indexed | Loading | Starting | Finishing | VerifyPending | FailureDiagnostic
  type Command = Init | Discover(int) | EndOfSpool | Stop(bool) | FailRead | LoadBody | Ack | Late(int)
  type State = {
    phase: Phase, discovered: Set[int], scanned: int, pending: List[int],
    reported: List[int], stops: int, interrupted: bool, failed: bool,
    failedId: int, lateChecks: int,
  }
  var s: State
  var dispatch: Command

  pure val requested = List(3, 1)
  pure val selected = Set(1, 3)

  pure def known(discovered: Set[int]): List[int] = requested.select(id => discovered.contains(id))

  pure def ready(state: State, pending: List[int]): State = {
    val phase = if (pending.length() > 0) Loading
      else if (state.failed) FailureDiagnostic else VerifyPending
    { ...state, pending: pending, phase: phase }
  }

  action init = all {
    s' = { phase: Scanning, discovered: Set(), scanned: 0, pending: List(),
      reported: List(), stops: 0, interrupted: false, failed: false, failedId: 0, lateChecks: 0 },
    dispatch' = Init,
  }

  action discover(id: int): bool = all {
    s.phase == Scanning, s.scanned < 4,
    s' = { ...s, discovered: s.discovered.union(Set(id).intersect(selected)), scanned: s.scanned + 1 },
    dispatch' = Discover(id),
  }

  action endOfSpool = all {
    s.phase == Scanning, s.discovered == selected,
    s' = { ...s, phase: Indexed, pending: requested }, dispatch' = EndOfSpool,
  }

  val reading = Set(Scanning, Indexed, Loading).contains(s.phase)
  val currentId = if (s.pending.length() == 0) 0 else s.pending.head()

  action stop(cancel: bool): bool = all {
    reading, s.stops < 2,
    s' = ready({ ...s, stops: s.stops + 1, interrupted: s.interrupted or cancel },
      if (s.phase != Scanning) s.pending
      else if (DRAIN_UNDISCOVERED) requested else known(s.discovered)),
    dispatch' = Stop(cancel),
  }

  action failRead = all {
    reading, not(s.failed),
    s' = ready({ ...s, failed: true, failedId: currentId },
      if (s.phase == Scanning) known(s.discovered) else s.pending.tail()),
    dispatch' = FailRead,
  }

  action loadBody = all {
    s.phase == Loading,
    s' = { ...s, phase: Starting }, dispatch' = LoadBody,
  }

  action ack = all {
    Set(Starting, Finishing).contains(s.phase),
    s' = if (s.phase == Starting) { ...s, phase: Finishing }
      else ready({ ...s, reported: s.reported.append(currentId) }, s.pending.tail()),
    dispatch' = Ack,
  }

  action late = all {
    s.lateChecks < s.stops,
    s' = { ...s, lateChecks: s.lateChecks + 1 }, dispatch' = Late(s.lateChecks),
  }

  action drainStep = any { loadBody, ack, late }
  action step = {
    nondet id = Set(1, 2, 3).oneOf()
    nondet cancel = Set(false, true).oneOf()
    any { discover(id), endOfSpool, stop(cancel), failRead, drainStep }
  }

  action partialStopStep =
    if (s.phase == Scanning and s.scanned < 2) discover(s.scanned + 1)
    else if (s.phase == Scanning) stop(true)
    else if (s.lateChecks == 0) late else drainStep

  action scanBoth = if (s.scanned == 0) discover(1) else discover(3)
  action repeatedIndexedStopStep =
    if (s.phase == Scanning and s.scanned < 2) scanBoth
    else if (s.phase == Scanning) endOfSpool
    else if (s.phase == Indexed) stop(false)
    else if (s.stops == 1) stop(true)
    else if (s.lateChecks < 2) late else drainStep

  action indexedFailureStep =
    if (s.phase == Scanning and s.scanned < 2) scanBoth
    else if (s.phase == Scanning) endOfSpool
    else if (s.phase == Indexed) failRead else drainStep

  action trailingFailureStep =
    if (s.phase == Scanning and s.scanned < 2) scanBoth
    else if (s.phase == Scanning) failRead else drainStep

  val partialDrain = s.phase == VerifyPending and s.reported == List(1) and s.lateChecks == 1
  val repeatedDrain = s.phase == VerifyPending and s.reported == requested and s.lateChecks == 2 and s.interrupted
  val failedBodySkipped = s.phase == FailureDiagnostic and s.failedId == 3 and s.reported == List(1)
  val trailingErrorDrained = s.phase == FailureDiagnostic and s.failedId == 0 and s.reported == requested
  val draining = s.stops > 0 or s.failed

  val Safety = all {
    s.discovered.subseteq(selected), s.lateChecks <= s.stops,
    draining implies s.reported.concat(s.pending) == known(s.discovered).select(id => id != s.failedId),
    s.phase == Scanning implies (not(draining) and s.pending == List()),
    s.phase == Indexed implies (not(draining) and s.discovered == selected and s.pending == requested),
    Set(VerifyPending, FailureDiagnostic).contains(s.phase) implies s.pending == List(),
    s.phase == VerifyPending implies not(s.failed),
    s.phase == FailureDiagnostic implies s.failed,
  }
}
module OrderedDrain { import OrderedDrainModel(DRAIN_UNDISCOVERED = false).* }
module OrderedDrainInventsCandidate { import OrderedDrainModel(DRAIN_UNDISCOVERED = true).* }
