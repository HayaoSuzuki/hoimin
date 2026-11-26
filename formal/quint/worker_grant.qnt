// Two workers share one copy reservation; ACKs can arrive in either order.
module WorkerGrantModel {
  const IGNORE_RESERVATION: bool
  type Phase = Copying | Materialized | VerifyingStop | Diagnostic
  type Slot = Pending | Consumed | Retired
  type Reply = {request:int, worker:int, reservationMatches:bool}
  type Command = Init | Created(Reply) | Failed(int) | Deadline | Idle
  type Result = Accepted | Duplicate | Late | WrongWorker | WrongReservation
  type Snapshot = {phase:Phase, slots:List[Slot], confirmed:Set[int], exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Copying, slots:[Pending, Pending], confirmed:Set(), exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action created(reply:Reply):bool = {
    val command = Created(reply)
    val verdict = match s.slots.nth(reply.request) {
      | Consumed => Duplicate
      | Retired => Late
      | Pending =>
        if (reply.worker != reply.request) WrongWorker
        else if (not(reply.reservationMatches) and not(IGNORE_RESERVATION)) WrongReservation
        else Accepted
    }
    if (verdict != Accepted) publish(s.with("result", verdict), command)
    else {
      val slots = s.slots.replaceAt(reply.request, Consumed)
      val phase = if (slots == [Consumed, Consumed]) Materialized else Copying
      val confirmed = if (reply.reservationMatches) s.confirmed.union(Set(reply.request)) else s.confirmed
      publish({ ...s, phase:phase, slots:slots, confirmed:confirmed, result:Accepted }, command)
    }
  }

  action answer(request:int, worker:int, reservationMatches:bool):bool =
    created({request:request, worker:worker, reservationMatches:reservationMatches})

  pure def retire(slots:List[Slot]):List[Slot] =
    slots.foldl([], (closed, slot) => closed.append(if (slot == Pending) Retired else slot))

  action failure(request:int):bool = all {
    s.slots.nth(request) == Pending,
    publish({ ...s, phase:Diagnostic, slots:retire(s.slots.replaceAt(request, Consumed)),
      exit:2, result:Accepted }, Failed(request)),
  }

  action deadline = all {
    s.phase == Copying,
    publish({ ...s, phase:VerifyingStop, slots:retire(s.slots), exit:4, result:Accepted }, Deadline),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet request = Set(0,1).oneOf()
    nondet worker = Set(0,1).oneOf()
    nondet reservationMatches = Bool.oneOf()
    answer(request, worker, reservationMatches),
    nondet request = Set(0,1).oneOf()
    failure(request),
    deadline,
    all { s.phase != Copying, idle },
  }

  action reverseStep =
    if (s.slots.nth(1) == Pending) answer(1,1,true) else answer(0,0,true)
  action wrongReservationStep =
    if (s.slots.nth(1) == Pending) answer(1,1,true)
    else if (s.slots.nth(0) == Pending and s.result == Accepted) answer(0,0,false)
    else answer(0,0,true)
  action wrongWorkerStep =
    if (s.slots.nth(0) == Pending and s.result == Accepted) answer(0,1,false)
    else if (s.slots.nth(0) == Pending) answer(0,0,true)
    else answer(1,1,true)
  action failureStep =
    if (s.phase == Copying) failure(1)
    else if (dispatch == Failed(1)) answer(1,1,true)
    else answer(0,0,true)
  action deadlineStep =
    if (s.slots.nth(1) == Pending) answer(1,1,true)
    else if (s.phase == Copying) deadline
    else answer(0,0,true)

  val Safety = all {
    s.phase == Materialized implies s.confirmed == Set(0,1) and s.exit == 0,
    s.confirmed.forall(worker => s.slots.nth(worker) == Consumed),
    s.phase != Copying implies not(s.slots.indices().exists(worker => s.slots.nth(worker) == Pending)),
  }
}

module WorkerGrant { import WorkerGrantModel(IGNORE_RESERVATION=false).* }
module WorkerGrantIgnoresReservation { import WorkerGrantModel(IGNORE_RESERVATION=true).* }
