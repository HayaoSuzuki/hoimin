import Std

namespace HoiminOracle.CgroupMember

inductive Errno where
  | noSuchProcess | permission | unsupported | tooManyFiles | invalid | io
  deriving DecidableEq, BEq

inductive Membership where
  | present | absent | unreadable | malformed | invalidUtf8 | decimalOverflow
  deriving DecidableEq, BEq

inductive Event where
  | openHandle (pid : Nat) | readMembership | signal (handle : Nat) | release (handle : Nat)
  deriving DecidableEq, BEq

inductive Failure where
  | pidOverflow | invalidMembership | syscall (errno : Errno)
  deriving DecidableEq, BEq

structure Observation where
  trace : List Event
  failure : Option Failure := none
  deriving DecidableEq

def largestSignedPid : Nat := 2147483647

def withAcquiredHandle (handle : Nat) (membership : Membership) (sendError : Option Errno) : Observation :=
  match membership with
  | .present =>
    ⟨[.readMembership, .signal handle, .release handle], match sendError with
      | none | some .noSuchProcess => none
      | some error => some (.syscall error)⟩
  | .absent => ⟨[.readMembership, .release handle], none⟩
  | .unreadable => ⟨[.readMembership, .release handle], some (.syscall .io)⟩
  | .malformed | .invalidUtf8 | .decimalOverflow =>
    ⟨[.readMembership, .release handle], some .invalidMembership⟩

def selectMember (pid handle : Nat) (openError : Option Errno) (membership : Membership)
    (sendError : Option Errno) : Observation :=
  if pid > largestSignedPid then ⟨[], some .pidOverflow⟩
  else
    let opened := Event.openHandle pid
    match openError with
    | none =>
      let remaining := withAcquiredHandle handle membership sendError
      { remaining with trace := opened :: remaining.trace }
    | some .noSuchProcess => ⟨[opened], none⟩
    | some error => ⟨[opened], some (.syscall error)⟩

theorem overflow_stops_before_any_operation (pid handle : Nat) (opening : Option Errno)
    (membership : Membership) (sending : Option Errno) (overflow : pid > largestSignedPid) :
    selectMember pid handle opening membership sending = ⟨[], some .pidOverflow⟩ := by
  simp [selectMember, overflow]

theorem signaling_requires_rechecking_and_uses_the_acquired_handle (pid handle signaled : Nat)
    (opening sending : Option Errno) (membership : Membership)
    (witness : Event.signal signaled ∈ (selectMember pid handle opening membership sending).trace) :
    pid ≤ largestSignedPid ∧ opening = none ∧ membership = .present ∧ signaled = handle ∧
      (selectMember pid handle opening membership sending).trace =
        [.openHandle pid, .readMembership, .signal handle, .release handle] := by
  by_cases overflow : pid > largestSignedPid
  · simp [selectMember, overflow] at witness
  · have fits := Nat.le_of_not_gt overflow
    cases opening with
    | some error => cases error <;> simp [selectMember, overflow] at witness
    | none =>
      cases membership <;>
        simp only [selectMember, if_neg overflow, withAcquiredHandle] at witness ⊢ <;>
        simp_all

theorem acquired_handle_is_released_on_every_result (pid handle : Nat)
    (membership : Membership) (sending : Option Errno) (fits : pid ≤ largestSignedPid) :
    (selectMember pid handle none membership sending).trace.getLast? = some (.release handle) := by
  have overflow : ¬pid > largestSignedPid := Nat.not_lt_of_ge fits
  cases membership <;> simp [selectMember, withAcquiredHandle, overflow]

theorem no_such_process_at_open_is_benign_and_skips_membership (pid handle : Nat)
    (membership : Membership) (sending : Option Errno) (fits : pid ≤ largestSignedPid) :
    selectMember pid handle (some .noSuchProcess) membership sending = ⟨[.openHandle pid], none⟩ := by
  simp [selectMember, Nat.not_lt_of_ge fits]

theorem old_handle_can_disappear_even_after_confirmed_membership (pid handle : Nat)
    (fits : pid ≤ largestSignedPid) :
    selectMember pid handle none .present (some .noSuchProcess) =
      ⟨[.openHandle pid, .readMembership, .signal handle, .release handle], none⟩ := by
  simp [selectMember, withAcquiredHandle, Nat.not_lt_of_ge fits]

end HoiminOracle.CgroupMember
