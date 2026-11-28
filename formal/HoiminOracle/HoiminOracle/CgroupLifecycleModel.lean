import Std

namespace HoiminOracle.CgroupLifecycle

inductive Root where
  | first | second
  deriving DecidableEq, BEq

inductive Status where
  | missing | inactive | active
  deriving DecidableEq, BEq

structure State where
  first : Status
  second : Status
  closed : Bool := false
  cleaned : Bool := false
  deriving DecidableEq

inductive Target where
  | root (root : Root)
  | run
  deriving DecidableEq, BEq

structure Call where
  target : Target
  state : State
  heldRoots : List Root
  closeHeld : Bool
  deriving DecidableEq

structure Outcome where
  after : State
  calls : List Call
  cleanupError : Option Target
  accountingError : Bool
  deriving DecidableEq

structure CloseFaults where
  first : Bool := false
  second : Bool := false
  run : Bool := false

def status (state : State) : Root → Status
  | .first => state.first
  | .second => state.second

def setStatus (state : State) (root : Root) (value : Status) : State :=
  match root with
  | .first => { state with first := value }
  | .second => { state with second := value }

def present (state : State) : List Root := [.first, .second].filter fun root =>
  status state root != .missing

def accountFails (state : State) (fault : Option Root) : Bool :=
  !state.cleaned && fault.any fun root => status state root == .active

def terminate (state : State) (root : Root) (failure : Bool) (accountFault : Option Root) : Outcome :=
  let accounting := accountFails state accountFault
  if status state root == .missing then ⟨state, [], none, accounting⟩
  else
    let call : Call := ⟨.root root, setStatus state root .inactive, [root], false⟩
    let after := if failure then state else setStatus state root .missing
    ⟨after, [call], if failure then some (.root root) else none, accounting⟩

private def makeInactive : Status → Status
  | .missing => .missing
  | _ => .inactive

private def pauseAccounting (state : State) : State :=
  { state with first := makeInactive state.first, second := makeInactive state.second }

private def failedRoot (faults : CloseFaults) : Root → Bool
  | .first => faults.first
  | .second => faults.second

private def closeRoots (held : List Root) (faults : CloseFaults) (state : State) :
    List Root → State × List Call × Option Target
  | [] => (state, [], none)
  | root :: rest =>
    let call : Call := ⟨.root root, state, held, true⟩
    let failed := status state root != .missing && failedRoot faults root
    let after := if failed then state else setStatus state root .missing
    let (final, calls, error) := closeRoots held faults after rest
    (final, call :: calls, if failed then some (.root root) else error)

private def restore (before after : Status) : Status :=
  if after == .missing then .missing else before

def finishClose (state : State) (held : List Root) (faults : CloseFaults)
    (accountFault : Option Root) : Outcome :=
  let published := { state with closed := true }
  if state.cleaned then ⟨published, [], none, false⟩
  else
    let (afterRoots, calls, rootError) := closeRoots held faults (pauseAccounting published) held
    let after := if faults.run then
      { afterRoots with closed := true, first := restore state.first afterRoots.first,
                        second := restore state.second afterRoots.second }
      else ⟨.missing, .missing, true, true⟩
    let runCall : Call := ⟨.run, afterRoots, held, true⟩
    let error := rootError.or (if faults.run then some .run else none)
    ⟨after, calls ++ [runCall], error, accountFails state accountFault⟩

def close (state : State) (faults : CloseFaults) (accountFault : Option Root) : Outcome :=
  finishClose state (present state) faults accountFault

def laterClose (initial : State) (terminated : Outcome) (faults : CloseFaults) : Outcome :=
  -- The close caller publishes closed and captures roots before waiting on the gates.
  finishClose terminated.after (present initial) faults none

theorem root_failure_restores_prior_accounting (state : State) (root : Root) (accountFault : Option Root) :
    (terminate state root true accountFault).after = state := by
  unfold terminate
  split <;> rfl

theorem successful_recursive_cleanup_publishes_empty_closed_state (state : State) (held : List Root)
    (faults : CloseFaults) (accountFault : Option Root) (notCleaned : state.cleaned = false)
    (success : faults.run = false) :
    (finishClose state held faults accountFault).after = ⟨.missing, .missing, true, true⟩ := by
  simp [finishClose, notCleaned, success]

theorem cleaned_run_never_reenters_cleanup (state : State) (held : List Root) (faults : CloseFaults)
    (accountFault : Option Root) (cleaned : state.cleaned = true) :
    (finishClose state held faults accountFault).calls = [] := by
  simp [finishClose, cleaned]

theorem closed_is_published_after_every_close (state : State) (held : List Root) (faults : CloseFaults)
    (accountFault : Option Root) : (finishClose state held faults accountFault).after.closed = true := by
  by_cases cleaned : state.cleaned
  · simp [finishClose, cleaned]
  · simp only [finishClose, if_neg cleaned]
    cases closeRoots held faults (pauseAccounting { state with closed := true }) held with
    | mk intermediate remaining => cases remaining; cases faults.run <;> rfl

theorem completed_root_is_not_called_by_later_terminate (state : State) (root : Root)
    (accountFault : Option Root) :
    (terminate (terminate state root false accountFault).after root false none).calls = [] := by
  cases state with
  | mk first second closed cleaned =>
    cases root <;> cases first <;> cases second <;> rfl

end HoiminOracle.CgroupLifecycle
