// Two workers, a spool of one or two candidates, and a run limit of one.
module MutantAdmissionModel {
  const COUNT_AT_START: bool
  type Stage = Idle | Read | Apply | StartReal | RunReal | FinishReal | Reset
    | StartSynthetic | FinishSynthetic
  type Worker = {stage:Stage, candidate:int}
  type Command = Init | Load(int) | EndOfSpool(int) | Advance(int) | Wait
  type Snapshot = {workers:List[Worker], supplied:int, eof:bool, verifying:bool,
    admitted:int, started:int, killed:int, notRun:int, incomplete:bool}
  var s:Snapshot
  var dispatch:Command

  pure val idle = {stage:Idle, candidate:0}
  pure val read = {stage:Read, candidate:0}

  action init = all {
    s' = {workers:[read,idle], supplied:0, eof:false, verifying:false,
      admitted:0, started:0, killed:0, notRun:0, incomplete:false},
    dispatch' = Init,
  }

  pure def readingCount(workers:List[Worker]):int =
    workers.foldl(0, (count, worker) => count + (if (worker.stage == Read) 1 else 0))

  pure def schedule(snapshot:Snapshot):Snapshot =
    if (snapshot.eof) snapshot.with("verifying", snapshot.workers == [idle,idle])
    else if (readingCount(snapshot.workers) > 0) snapshot
    else if (snapshot.workers.nth(0) == idle)
      snapshot.with("workers", snapshot.workers.replaceAt(0, read))
    else if (snapshot.workers.nth(1) == idle)
      snapshot.with("workers", snapshot.workers.replaceAt(1, read))
    else snapshot

  action publish(following:Snapshot, command:Command):bool = all {
    s' = schedule(following),
    dispatch' = command,
  }

  action load(worker:int):bool = all {
    s.workers.nth(worker).stage == Read and s.supplied < 2,
    val charged = if (COUNT_AT_START) s.started else s.admitted
    val real = charged < 1
    val stage = if (real) Apply else StartSynthetic
    val candidate = s.supplied + 1
    publish({ ...s, workers:s.workers.replaceAt(worker, {stage:stage, candidate:candidate}),
      supplied:candidate, admitted:s.admitted + (if (real) 1 else 0),
      incomplete:s.incomplete or not(real) }, Load(worker)),
  }

  action eof(worker:int):bool = all {
    s.workers.nth(worker).stage == Read and s.supplied > 0,
    publish({ ...s, workers:s.workers.replaceAt(worker, idle), eof:true }, EndOfSpool(worker)),
  }

  pure def followingStage(stage:Stage):Stage = match stage {
    | Apply => StartReal
    | StartReal => RunReal
    | RunReal => FinishReal
    | FinishReal => Reset
    | StartSynthetic => FinishSynthetic
    | _ => Idle
  }

  action advance(worker:int):bool = {
    val current = s.workers.nth(worker)
    all {
      current.stage != Idle and current.stage != Read,
      val stage = followingStage(current.stage)
      val following = if (stage == Idle) idle else current.with("stage", stage)
      publish({ ...s, workers:s.workers.replaceAt(worker, following),
        started:s.started + (if (current.stage == StartReal) 1 else 0),
        killed:s.killed + (if (current.stage == RunReal) 1 else 0),
        notRun:s.notRun + (if (current.stage == FinishSynthetic) 1 else 0) }, Advance(worker)),
    }
  }

  action wait = all { s' = s, dispatch' = Wait }
  action step = any {
    nondet worker = Set(0, 1).oneOf()
    load(worker),
    nondet worker = Set(0, 1).oneOf()
    eof(worker),
    nondet worker = Set(0, 1).oneOf()
    advance(worker),
    all { s.verifying, wait },
  }

  val reader = if (s.workers.nth(0).stage == Read) 0 else 1
  action earlyOverflowStep =
    if (s.supplied < 2) load(reader)
    else if (s.verifying) wait
    else if (s.workers.nth(1).stage == StartSynthetic or s.workers.nth(1).stage == FinishSynthetic)
      advance(1)
    else if (not(s.eof)) eof(reader)
    else advance(0)
  action exactStep =
    if (s.supplied == 0) load(reader)
    else if (not(s.eof)) eof(reader)
    else if (s.verifying) wait
    else advance(0)
  action realFirstStep =
    if (s.supplied == 0) load(reader)
    else if (s.supplied == 1 and s.workers.nth(0) != idle) advance(0)
    else if (s.supplied == 1) load(reader)
    else if (not(s.eof)) eof(reader)
    else if (s.verifying) wait
    else advance(1)

  val Safety = all {
    s.killed <= s.started and s.started <= s.admitted and s.admitted <= 1,
    readingCount(s.workers) <= 1,
    s.notRun + s.admitted <= s.supplied,
    s.incomplete == (s.supplied > s.admitted),
    s.verifying implies s.eof and s.workers == [idle,idle] and
      s.killed + s.notRun == s.supplied,
  }
}

module MutantAdmission { import MutantAdmissionModel(COUNT_AT_START=false).* }
module MutantAdmissionChargesAtStart { import MutantAdmissionModel(COUNT_AT_START=true).* }
