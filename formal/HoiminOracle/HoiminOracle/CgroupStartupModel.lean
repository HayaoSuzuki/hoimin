import Std

namespace HoiminOracle.CgroupStartup

inductive FileStage where
  | missing | directory | memory | pids | oom
  deriving DecidableEq, BEq

inductive Registration where
  | absent | inactive | active
  deriving DecidableEq, BEq

structure State where
  files : FileStage := .missing
  registration : Registration := .absent
  memberWritten : Bool := false
  closed : Bool := false
  cleaned : Bool := false
  deriving DecidableEq

inductive Call where
  | validate | create | memory | pids | oom | wait | writeMembership | resume
  deriving DecidableEq, BEq

inductive Failure where
  | closed | validate | accounting | create | memory | pids | oom | initialCounters
  | missingPid | pidOverflow | wait | writeMembership | missingRoot | resume
  deriving DecidableEq, BEq

inductive PrepareFault where
  | none | validate | accounting | create | memory | pids | oom | initialCounters
  deriving DecidableEq, BEq

inductive AttachFault where
  | none | wait | accounting | writeMembership | resume
  deriving DecidableEq, BEq

inductive Pid where
  | missing | overflow | valid
  deriving DecidableEq, BEq

inductive Timing where
  | openRun | alreadyClosed | closeDuringWait
  deriving DecidableEq, BEq

structure Observation where
  after : State
  calls : List Call
  failure : Option Failure := none
  deriving DecidableEq

def prepare (closed : Bool) (fault : PrepareFault) : Observation :=
  let start : State := { closed }
  if closed then ⟨start, [], some .closed⟩
  else match fault with
  | .validate => ⟨start, [.validate], some .validate⟩
  | .accounting => ⟨start, [.validate], some .accounting⟩
  | .create => ⟨start, [.validate, .create], some .create⟩
  | .memory => ⟨{ start with files := .directory }, [.validate, .create, .memory], some .memory⟩
  | .pids => ⟨{ start with files := .memory }, [.validate, .create, .memory, .pids], some .pids⟩
  | .oom => ⟨{ start with files := .pids }, [.validate, .create, .memory, .pids, .oom], some .oom⟩
  | .initialCounters =>
    ⟨{ start with files := .oom }, [.validate, .create, .memory, .pids, .oom], some .initialCounters⟩
  | .none =>
    ⟨{ start with files := .oom, registration := .inactive }, [.validate, .create, .memory, .pids, .oom], none⟩

def attach (pid : Pid) (registered : Bool) (timing : Timing) (fault : AttachFault) : Observation :=
  let start : State := ⟨.oom, if registered then .inactive else .absent, false, timing == .alreadyClosed, false⟩
  match pid with
  | .missing => ⟨start, [], some .missingPid⟩
  | .overflow => ⟨start, [], some .pidOverflow⟩
  | .valid =>
    if fault == .wait then ⟨start, [.wait], some .wait⟩
    else if timing == .closeDuringWait then
      ⟨{ closed := true, cleaned := true }, [.wait], some .closed⟩
    else if start.closed then ⟨start, [.wait], some .closed⟩
    else if fault == .accounting then ⟨start, [.wait], some .accounting⟩
    else if fault == .writeMembership then ⟨start, [.wait, .writeMembership], some .writeMembership⟩
    else
      let member := { start with memberWritten := true }
      if !registered then ⟨member, [.wait, .writeMembership], some .missingRoot⟩
      else ⟨{ member with registration := .active }, [.wait, .writeMembership, .resume],
        if fault == .resume then some .resume else none⟩

def cleanupCalls (state : State) : List String :=
  if state.cleaned then []
  else if state.registration == .absent then ["run"]
  else ["root", "run"]

theorem failed_preparation_does_not_publish_root (closed : Bool) (fault : PrepareFault)
    (failed : (prepare closed fault).failure.isSome) :
    (prepare closed fault).after.registration = .absent := by
  revert failed
  cases closed <;> cases fault <;> decide

theorem successful_preparation_publishes_an_inactive_root (closed : Bool) (fault : PrepareFault)
    (succeeded : (prepare closed fault).failure = none) :
    (prepare closed fault).after.registration = .inactive ∧
      (prepare closed fault).after.files = .oom := by
  revert succeeded
  cases closed <;> cases fault <;> decide

theorem resume_requires_a_stopped_registered_member (pid : Pid) (registered : Bool)
    (timing : Timing) (fault : AttachFault) (resumes : (attach pid registered timing fault).calls.contains .resume) :
    (attach pid registered timing fault).calls = [.wait, .writeMembership, .resume] ∧
      (attach pid registered timing fault).after.registration = .active ∧
      (attach pid registered timing fault).after.memberWritten = true := by
  revert resumes
  cases pid <;> cases registered <;> cases timing <;> cases fault <;> decide

theorem close_during_wait_prevents_attaching (registered : Bool) (fault : AttachFault) :
    (attach .valid registered .closeDuringWait fault).calls = [.wait] := by
  cases registered <;> cases fault <;> decide

theorem failed_resume_leaves_the_root_registered_for_cleanup :
    cleanupCalls (attach .valid true .openRun .resume).after = ["root", "run"] := rfl

end HoiminOracle.CgroupStartup
