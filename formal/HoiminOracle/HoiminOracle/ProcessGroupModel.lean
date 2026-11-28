import Std

namespace HoiminOracle.ProcessGroup

inductive Authority where
  | missing | signalable | probeOnly
  deriving DecidableEq, BEq

structure State where
  authority : Authority
  terminated : Bool := false
  quiescent : Bool := false
  deriving DecidableEq, BEq

inductive OsResult where
  | ok | absent | denied | ioError
  deriving DecidableEq, BEq

inductive Command where
  | reap
  | terminateLive (signal : OsResult) (failBeforeSignal : Bool)
  | terminateReaped (probe : OsResult) (failAfterProbe : Bool)
  | refresh (probe : OsResult)
  deriving DecidableEq, BEq

inductive Reply where
  | noReply | settled (quiescent : Bool) | probeError | signalError | terminationError
  deriving DecidableEq, BEq

inductive Call where
  | probe | signal
  deriving DecidableEq, BEq

structure Observation where
  after : State
  reply : Reply
  call : Option Call := none
  deriving DecidableEq

def initial (attached : Bool) : State :=
  ⟨if attached then .signalable else .missing, false, false⟩

def disarm (state : State) : State :=
  if state.authority == .signalable then { state with authority := .probeOnly } else state

def probeAfterReap (state : State) (result : OsResult) : Observation :=
  let state := disarm state
  if state.authority == .probeOnly then
    match result with
    | .absent => ⟨{ state with authority := .missing }, .settled true, some .probe⟩
    | .ioError => ⟨state, .probeError, some .probe⟩
    | .ok | .denied => ⟨state, .settled false, some .probe⟩
  else
    ⟨state, .settled false, none⟩

def complete (state : State) (quiescent : Bool) (call : Option Call) : Observation :=
  ⟨{ state with terminated := true, quiescent }, .settled quiescent, call⟩

def step (state : State) : Command → Observation
  | .reap => ⟨disarm state, .noReply, none⟩
  | .terminateLive result fail =>
    if state.terminated then ⟨state, .settled state.quiescent, none⟩
    else if fail then ⟨state, .terminationError, none⟩
    else if state.authority == .signalable then
      match result with
      | .ok | .absent => complete state false (some .signal)
      | .denied | .ioError => ⟨state, .signalError, some .signal⟩
    else complete state false none
  | .terminateReaped result fail =>
    if state.terminated then ⟨state, .settled state.quiescent, none⟩
    else
      let observed := probeAfterReap state result
      match observed.reply with
      | .settled quiet =>
        if fail then { observed with reply := .terminationError }
        else complete observed.after quiet observed.call
      | _ => observed
  | .refresh result =>
    if state.quiescent then ⟨state, .settled true, none⟩
    else
      let observed := probeAfterReap state result
      match observed.reply with
      | .settled quiet => { observed with after := { observed.after with quiescent := quiet } }
      | _ => observed

def maySignal (state : State) : Bool :=
  state.authority == .signalable && !state.terminated

def quietImpliesNoIdentifier (state : State) : Prop :=
  state.quiescent = true → state.authority = .missing

def noFutureSignal (state : State) : List Command → Prop
  | [] => True
  | command :: rest =>
    let next := step state command
    next.call ≠ some .signal ∧ noFutureSignal next.after rest

theorem step_without_authority_cannot_signal_or_rearm (state : State) (command : Command)
    (disarmed : maySignal state = false) :
    (step state command).call ≠ some .signal ∧ maySignal (step state command).after = false := by
  obtain ⟨authority, terminated, quiescent⟩ := state
  revert disarmed
  cases authority <;> cases terminated <;> cases quiescent
  all_goals
    cases command with
    | reap => decide
    | terminateLive result fail | terminateReaped result fail =>
      cases result <;> cases fail <;> decide
    | refresh result => cases result <;> decide

theorem all_later_operations_are_signal_free (state : State) (commands : List Command)
    (disarmed : maySignal state = false) : noFutureSignal state commands := by
  induction commands generalizing state with
  | nil => trivial
  | cons command rest inductionHypothesis =>
    have next := step_without_authority_cannot_signal_or_rearm state command disarmed
    exact ⟨next.1, inductionHypothesis (step state command).after next.2⟩

theorem explicit_reap_disarms_before_later_operations (state : State) :
    maySignal (step state .reap).after = false := by
  obtain ⟨authority, terminated, quiescent⟩ := state
  cases authority <;> cases terminated <;> cases quiescent <;> decide

theorem terminate_reaped_disarms_even_on_probe_or_termination_error (state : State)
    (result : OsResult) (fail : Bool) :
    (step state (.terminateReaped result fail)).call ≠ some .signal ∧
      maySignal (step state (.terminateReaped result fail)).after = false := by
  obtain ⟨authority, terminated, quiescent⟩ := state
  cases authority <;> cases terminated <;> cases quiescent <;> cases result <;> cases fail <;> decide

theorem step_preserves_valid_quiescence (state : State) (command : Command)
    (valid : quietImpliesNoIdentifier state) : quietImpliesNoIdentifier (step state command).after := by
  obtain ⟨authority, terminated, quiescent⟩ := state
  revert valid
  unfold quietImpliesNoIdentifier
  cases authority <;> cases terminated <;> cases quiescent
  all_goals
    cases command with
    | reap => decide
    | terminateLive result fail | terminateReaped result fail =>
      cases result <;> cases fail <;> decide
    | refresh result => cases result <;> decide

def reportsAbsence : Command → Bool
  | .refresh .absent | .terminateReaped .absent false => true
  | _ => false

theorem new_quiescence_requires_an_absence_probe (state : State) (command : Command)
    (previouslyUnknown : state.quiescent = false) (nowKnown : (step state command).after.quiescent = true) :
    (step state command).call = some .probe ∧ reportsAbsence command = true := by
  obtain ⟨authority, terminated, quiescent⟩ := state
  revert previouslyUnknown nowKnown
  cases authority <;> cases terminated <;> cases quiescent
  all_goals
    cases command with
    | reap => decide
    | terminateLive result fail | terminateReaped result fail =>
      cases result <;> cases fail <;> decide
    | refresh result => cases result <;> decide

theorem successful_live_termination_still_requires_absence_evidence (result : OsResult)
    (initiallyAttached : Bool) (accepted : (step (initial initiallyAttached) (.terminateLive result false)).reply =
      .settled false) :
    (step (initial initiallyAttached) (.terminateLive result false)).after.quiescent = false := by
  revert accepted
  cases initiallyAttached <;> cases result <;> decide

end HoiminOracle.ProcessGroup
