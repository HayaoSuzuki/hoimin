import Std

namespace HoiminOracle.SessionDispatch

inductive Command where
  | beginRun | resume
  deriving DecidableEq, BEq

inductive Waiter where
  | awaited | waiterCancelled | queuedCancelled | dispatcherDropped | unpolled
  deriving DecidableEq, BEq

inductive Event where
  | normal | databaseError | budgetChanged
  deriving DecidableEq, BEq

inductive Reply where
  | started | resumed | commitError | recheckError | candidateChanged
  deriving DecidableEq, BEq

structure Snapshot where
  budget : Option Nat
  locked : Bool
  deriving DecidableEq, BEq

structure Observation where
  pending : Option Snapshot
  settled : Snapshot
  afterDispatcherDrop : Snapshot
  reply : Option Reply
  deriving DecidableEq

def originalBudget : Nat := 255
def editedBudget : Nat := 256
def requestedBudget : Nat := 257

def initial : Command → Snapshot
  | .beginRun => ⟨none, false⟩
  | .resume => ⟨some originalBudget, false⟩

def atGate (command : Command) : Snapshot :=
  { initial command with locked := command == .beginRun }

def finishWork (command : Command) (event : Event) : Reply × Snapshot :=
  match command, event with
  | .beginRun, .databaseError => (.commitError, initial command)
  | .beginRun, _ => (.started, ⟨some requestedBudget, true⟩)
  | .resume, .databaseError => (.recheckError, initial command)
  | .resume, .budgetChanged => (.candidateChanged, ⟨some editedBudget, false⟩)
  | .resume, .normal => (.resumed, ⟨some requestedBudget, true⟩)

def dispatch (command : Command) (waiter : Waiter) (event : Event) : Observation :=
  if waiter == .unpolled then
    ⟨none, initial command, initial command, none⟩
  else
    let (reply, completed) := finishWork command event
    let released := { completed with locked := false }
    let settled := if waiter == .dispatcherDropped then released else completed
    ⟨some (atGate command), settled, released, if waiter == .awaited then some reply else none⟩

theorem unpolled_future_has_no_worker_effects (command : Command) (event : Event) :
    dispatch command .unpolled event = ⟨none, initial command, initial command, none⟩ := by
  rfl

theorem cancelled_waiter_preserves_worker_effects (command : Command) (event : Event) :
    (dispatch command .waiterCancelled event).settled = (dispatch command .awaited event).settled ∧
      (dispatch command .waiterCancelled event).reply = none := by
  cases command <;> cases event <;> decide

theorem scheduled_work_continues_after_cancellation_before_handler_entry
    (command : Command) (event : Event) :
    dispatch command .queuedCancelled event = dispatch command .waiterCancelled event := by
  cases command <;> cases event <;> decide

theorem dropped_dispatcher_releases_lock_after_finishing_work (command : Command) (event : Event) :
    let outcome := dispatch command .dispatcherDropped event
    outcome.settled.locked = false ∧
      outcome.settled.budget = (dispatch command .awaited event).settled.budget ∧ outcome.reply = none := by
  cases command <;> cases event <;> decide

theorem provisional_begin_lock_survives_dispatcher_drop_at_gate (event : Event) :
    (dispatch .beginRun .dispatcherDropped event).pending = some ⟨none, true⟩ := by
  cases event <;> decide

theorem resume_gate_precedes_ownership_and_budget_update (event : Event) :
    (dispatch .resume .waiterCancelled event).pending = some ⟨some originalBudget, false⟩ := by
  cases event <;> decide

theorem rejected_begin_and_recheck_leave_no_lock (waiter : Waiter) :
    (dispatch .beginRun waiter .databaseError).settled = initial .beginRun ∧
      (dispatch .resume waiter .databaseError).settled = initial .resume := by
  cases waiter <;> decide

end HoiminOracle.SessionDispatch
