// The retry phase starts with two roots deferred by their initial cleanup attempts.
// Each visit receives a cleanup result, and the deadline is observed between visits.
module JanitorRetry {
  type SliceResult = Complete | Failed | Deferred(int)
  type Stop = Drained | Stalled | Deadline
  type Phase = Running | Halted(Stop)
  type Visit = { root: int, round: int, result: SliceResult }
  type QueueState = { pending: List[int], unvisited: int, progressed: bool,
    phase: Phase, round: int, visits: List[Visit] }

  pure val maxVisits = 6
  var s: QueueState

  action init = s' = { pending: [0, 1], unvisited: 2, progressed: false,
    phase: Running, round: 0, visits: [] }

  pure def isDeferred(result: SliceResult): bool = match result {
    | Deferred(_) => true
    | _ => false
  }

  pure def madeProgress(result: SliceResult): bool = match result {
    | Deferred(removed) => removed > 0
    | _ => false
  }

  pure def afterSlice(state: QueueState, result: SliceResult): QueueState = {
    val root = state.pending.head()
    val rest = state.pending.tail()
    { ...state,
      pending: if (isDeferred(result)) rest.append(root) else rest,
      unvisited: state.unvisited - 1,
      progressed: state.progressed or madeProgress(result),
      visits: state.visits.append({ root: root, round: state.round, result: result }) }
  }

  action visit(result: SliceResult): bool = all {
    s.phase == Running and s.unvisited > 0 and s.visits.length() < maxVisits,
    s' = afterSlice(s, result),
  }

  pure def afterRound(state: QueueState, repeatStalled: bool): QueueState =
    if (state.pending.length() == 0) { ...state, phase: Halted(Drained) }
    else if (state.progressed or repeatStalled) {
      ...state, round: state.round + 1, unvisited: state.pending.length(), progressed: false
    } else { ...state, phase: Halted(Stalled) }

  action endRound(repeatStalled: bool): bool = all {
    s.phase == Running and s.unvisited == 0,
    s' = afterRound(s, repeatStalled),
  }

  action expire: bool = all {
    s.phase == Running and s.unvisited > 0,
    s' = { ...s, phase: Halted(Deadline) },
  }

  action chooseVisit: bool = {
    nondet result = Set(Complete, Failed, Deferred(0), Deferred(1)).oneOf()
    visit(result)
  }

  action step = any { chooseVisit, endRound(false), expire }

  action stalledScenario(repeatStalled: bool): bool =
    if (s.unvisited == 0) endRound(repeatStalled)
    else visit(if (s.visits.length() == 0) Complete else Deferred(0))

  action stalledStep = stalledScenario(false)
  action stalledRetryStep = stalledScenario(true)

  action progressStep =
    if (s.unvisited == 0) endRound(false)
    else if (s.round > 0) visit(Complete)
    else visit(if (s.visits.length() == 0) Deferred(1) else Deferred(0))

  action deadlineStep =
    if (s.visits.length() == 0) visit(Deferred(1)) else expire

  pure def visitsIn(visits: List[Visit], round: int): List[Visit] =
    visits.select(entry => entry.round == round)

  pure def progressedIn(visits: List[Visit], round: int): bool =
    visitsIn(visits, round).select(entry => madeProgress(entry.result)).length() > 0

  // A completed round preserves the original relative order of roots still pending.
  pure def fifo(state: QueueState): bool = {
    val finishedEarlier = state.visits.select(entry =>
      entry.round < state.round and not(isDeferred(entry.result)))
    val atStart = [0, 1].select(root =>
      finishedEarlier.select(entry => entry.root == root).length() == 0)
    val current = visitsIn(state.visits, state.round)
    val visited = current.foldl([], (ids, entry) => ids.append(entry.root))
    val deferred = current.select(entry => isDeferred(entry.result))
      .foldl([], (ids, entry) => ids.append(entry.root))
    if (visited.length() > atStart.length()) false
    else all {
      visited == atStart.slice(0, visited.length()),
      state.unvisited == atStart.length() - visited.length(),
      state.pending == atStart.slice(visited.length(), atStart.length()).concat(deferred),
    }
  }

  val continuedAfterProgress = s.round > 0
  val drained = s.phase == Halted(Drained)
  val stalledAfterCompletion = s.phase == Halted(Stalled) and
    visitsIn(s.visits, s.round).select(entry => entry.result == Complete).length() > 0
  val deadlineMidRound = s.phase == Halted(Deadline) and s.pending == [1, 0]
  val failedRoot = s.visits.select(entry => entry.result == Failed).length() > 0

  val Safety = all {
    fifo(s),
    s.progressed == progressedIn(s.visits, s.round),
    0.to(s.round - 1).forall(round => progressedIn(s.visits, round)),
    s.phase == Halted(Drained) implies s.pending.length() == 0,
    s.phase == Halted(Stalled) implies
      s.pending.length() > 0 and s.unvisited == 0 and not(s.progressed),
    s.phase == Halted(Deadline) implies s.unvisited > 0,
  }
}
