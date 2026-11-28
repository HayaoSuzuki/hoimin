namespace HoiminOracle.WorkerMetrics

def nanosPerMilli : Nat := 1000000

def maxMillis : Nat := 18446744073709551615

def millis (nanos : Nat) : Nat := min (nanos / nanosPerMilli) maxMillis

def addMillis (accumulated interval : Nat) : Nat :=
  min (accumulated + millis interval) maxMillis

-- Clock readings are nanoseconds. The exported counters are milliseconds.
inductive Phase
  | idle
  | queued (since : Nat)
  | running (since : Nat)
  deriving DecidableEq, BEq

inductive Action
  | queue | start | cancel | finishCounted | finishUncounted
  deriving DecidableEq, BEq

inductive Rejection
  | alreadyQueued | alreadyStarted | notQueued | notStarted
  deriving DecidableEq, BEq

structure Accounting where
  queueWait : Nat := 0
  busy : Nat := 0
  processes : Nat := 0
  deriving DecidableEq, BEq

structure Worker extends Accounting where
  phase : Phase := .idle
  deriving DecidableEq, BEq

def Worker.toIdle (worker : Worker) : Worker := { worker with phase := .idle }

def transition (worker : Worker) (time : Nat) (action : Action) :
    Except Rejection Worker :=
  match action, worker.phase with
  | .queue, .idle => .ok { worker with phase := .queued time }
  | .queue, .queued _ => .error .alreadyQueued
  | .queue, .running _ => .error .alreadyStarted
  | .start, .queued start =>
      .ok { worker with phase := .running time, queueWait := addMillis worker.queueWait (time - start) }
  | .start, .idle => .error .notQueued
  | .start, .running _ => .error .alreadyStarted
  | .cancel, .queued _ => .ok worker.toIdle
  | .cancel, _ => .error .notQueued
  | .finishCounted, .running start =>
      .ok { worker.toIdle with
        busy := addMillis worker.busy (time - start)
        processes := worker.processes + 1 }
  | .finishUncounted, .running start =>
      .ok { worker.toIdle with busy := addMillis worker.busy (time - start) }
  | .finishCounted, _ | .finishUncounted, _ => .error .notStarted

def applyAction (worker : Worker) (time : Nat) (action : Action) :
    Worker × Option Rejection :=
  match transition worker time action with
  | .ok updated => (updated, none)
  | .error reason => (worker, some reason)

inductive WorkerId | first | second
  deriving DecidableEq, BEq

structure Event where
  time : Nat
  id : WorkerId
  action : Action

structure Workers where
  first : Worker := {}
  second : Worker := {}
  deriving DecidableEq, BEq

def recordWith (apply : Worker → Nat → Action → Worker × Option Rejection)
    (workers : Workers) (event : Event) : Workers × Option Rejection :=
  match event.id with
  | .first =>
      let (worker, reason) := apply workers.first event.time event.action
      ({ workers with first := worker }, reason)
  | .second =>
      let (worker, reason) := apply workers.second event.time event.action
      ({ workers with second := worker }, reason)

def recordEvent := recordWith applyAction

def replayWith (record : Workers → Event → Workers × Option Rejection)
    (workers : Workers) : List Event → Workers × List (Option Rejection)
  | [] => (workers, [])
  | event :: rest =>
      let (after, reason) := record workers event
      let (final, reasons) := replayWith record after rest
      (final, reason :: reasons)

def replay := replayWith recordEvent

inductive FinishResult
  | outstanding (id : WorkerId)
  | validation
  | saved (discovered : Nat) (workers : List (WorkerId × Accounting))
  deriving DecidableEq, BEq

def finish (workers : Workers) (time discovered executed : Nat) : FinishResult :=
  if workers.first.phase != .idle then .outstanding .first
  else if workers.second.phase != .idle then .outstanding .second
  else
    let ordered : List (WorkerId × Accounting) :=
      [(.first, workers.first.toAccounting), (.second, workers.second.toAccounting)]
    let seen := ordered.filter fun (_, worker) => worker.processes > 0
    if executed > discovered || executed != workers.first.processes + workers.second.processes ||
        seen.any (fun (_, worker) => worker.queueWait + worker.busy > millis time) then .validation
    else .saved discovered seen

end HoiminOracle.WorkerMetrics
