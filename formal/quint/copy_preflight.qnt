// Two configured workers and a 20-byte copy budget. Overflow is symbolic.
module CopyPreflightModel {
  const ACCEPT_WRONG_COUNT: bool
  type Mode = Disabled | Fresh | Resume
  type Size = Bytes(int) | Overflow
  type Measurement = {perWorker:Size, workers:int, total:int}
  type Stage = Ready | Awaiting | Report | BeginSession | LoadSession | Workers
  type Command = Init | Configure(Mode) | Measure(Measurement) | AckReport | Idle
  type Result = Accepted | AggregateOverflow | AggregateMismatch | LimitExceeded | CountMismatch | Duplicate
  type Snapshot = {mode:Mode, stage:Stage, evidence:Measurement, result:Result}
  var s:Snapshot
  var dispatch:Command

  pure val empty = {perWorker:Bytes(0), workers:2, total:0}
  pure val exact = {perWorker:Bytes(10), workers:2, total:20}
  pure val excessive = {perWorker:Bytes(11), workers:2, total:22}
  pure val wrongTotal = {perWorker:Bytes(10), workers:2, total:19}
  pure val wrongCount = {perWorker:Bytes(10), workers:1, total:10}
  pure val overflow = {perWorker:Overflow, workers:2, total:0}

  action init = all {
    s' = {mode:Disabled, stage:Ready, evidence:empty, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action configure(mode:Mode):bool = all {
    s.stage == Ready,
    publish({ ...s, mode:mode, stage:Awaiting }, Configure(mode)),
  }

  pure def validate(evidence:Measurement):Result = match evidence.perWorker {
    | Overflow => AggregateOverflow
    | Bytes(amount) =>
      if (amount * evidence.workers != evidence.total) AggregateMismatch
      else if (evidence.total > 20) LimitExceeded
      else if (evidence.workers != 2 and not(ACCEPT_WRONG_COUNT)) CountMismatch
      else Accepted
  }

  action measure(evidence:Measurement):bool = {
    val result = if (s.stage == Awaiting) validate(evidence) else Duplicate
    val stage = match s.mode {
      | Disabled => Report
      | Fresh => BeginSession
      | Resume => LoadSession
    }
    publish(if (result == Accepted) { ...s, stage:stage, evidence:evidence, result:result }
      else s.with("result", result), Measure(evidence))
  }

  action acknowledge = all {
    s.stage == Report,
    publish({ ...s, stage:Workers, result:Accepted }, AckReport),
  }

  action idle = publish(s, Idle)
  action step = if (s.stage == Ready) {
    nondet mode = Set(Disabled, Fresh, Resume).oneOf()
    configure(mode)
  } else any {
    nondet evidence = Set(empty, exact, excessive, wrongTotal, wrongCount, overflow,
      {perWorker:Bytes(0), workers:0, total:0}, {perWorker:Bytes(6), workers:3, total:18}).oneOf()
    measure(evidence),
    acknowledge,
    all { s.stage != Awaiting, idle },
  }

  action ordinaryStep =
    if (s.stage == Ready) configure(Disabled)
    else if (s.stage == Awaiting) measure(exact)
    else if (s.stage == Report) acknowledge else measure(exact)
  action freshRetryStep =
    if (s.stage == Ready) configure(Fresh)
    else if (s.stage == Awaiting and s.result == Accepted) measure(wrongCount)
    else measure(exact)
  action resumeRetryStep =
    if (s.stage == Ready) configure(Resume)
    else if (s.stage == Awaiting and s.result == Accepted) measure(wrongCount)
    else if (s.stage == Awaiting and s.result == CountMismatch) measure(wrongTotal)
    else if (s.stage == Awaiting and s.result == AggregateMismatch) measure(overflow)
    else if (s.stage == Awaiting and s.result == AggregateOverflow) measure(excessive)
    else measure(exact)
  action emptyStep =
    if (s.stage == Ready) configure(Disabled)
    else if (s.stage == Awaiting) measure(empty)
    else if (s.stage == Report) acknowledge else idle

  val consistentTotal = match s.evidence.perWorker {
    | Overflow => false
    | Bytes(amount) => s.evidence.total == amount * s.evidence.workers
  }
  val Safety = all {
    Set(Report, BeginSession, LoadSession, Workers).contains(s.stage) implies
      s.evidence.workers == 2 and s.evidence.total <= 20 and consistentTotal,
    s.stage == BeginSession implies s.mode == Fresh,
    s.stage == LoadSession implies s.mode == Resume,
  }
}

module CopyPreflight { import CopyPreflightModel(ACCEPT_WRONG_COUNT=false).* }
module CopyPreflightAcceptsWrongCount { import CopyPreflightModel(ACCEPT_WRONG_COUNT=true).* }
