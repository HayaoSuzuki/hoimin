import Std

namespace HoiminOracle.BeginTransaction

inductive Owner where
  | free | caller | peer
  deriving DecidableEq, BEq

inductive Metadata where
  | absent | legacy | current
  deriving DecidableEq, BEq

structure RunRow where
  digest : Nat
  budget : Nat
  finished : Bool
  complete : Bool
  deriving DecidableEq, BEq

structure State where
  owner : Owner
  fingerprint : Metadata
  row : Option RunRow
  deriving DecidableEq

inductive Fault where
  | none | transactionBusy | fingerprintAbort | runAbort | commitReject
  deriving DecidableEq, BEq

inductive Outcome where
  | started | active | beginError | fingerprintError | runError | duplicateRun | commitError
  deriving DecidableEq, BEq

structure Observation where
  outcome : Outcome
  after : State
  deriving DecidableEq

def requestedDigest : Nat := 3

def registerFingerprint : Metadata → Metadata
  | .absent => .current
  | present => present

def begin (before : State) (fault : Fault) (budget : Nat) : Observation :=
  let reject outcome := ⟨outcome, before⟩
  match before.owner, fault, before.row with
  | .peer, _, _ => reject .active
  | _, .transactionBusy, _ => reject .beginError
  | _, .fingerprintAbort, _ => reject .fingerprintError
  | _, .runAbort, _ => reject .runError
  | _, _, some _ => reject .duplicateRun
  | _, .commitReject, none => reject .commitError
  | _, .none, none =>
    ⟨.started, ⟨.caller, registerFingerprint before.fingerprint,
      some ⟨requestedDigest, budget, false, false⟩⟩⟩

theorem active_peer_precedes_storage (fingerprint : Metadata) (row : Option RunRow)
    (fault : Fault) (budget : Nat) :
    begin ⟨.peer, fingerprint, row⟩ fault budget = ⟨.active, ⟨.peer, fingerprint, row⟩⟩ := by
  simp [begin]

theorem failed_begin_preserves_run_fingerprint_and_owner (before : State) (fault : Fault)
    (budget : Nat) (failed : (begin before fault budget).outcome ≠ .started) :
    (begin before fault budget).after = before := by
  obtain ⟨owner, fingerprint, row⟩ := before
  cases owner <;> cases fault <;> cases row <;> simp_all [begin]

theorem success_registers_run_and_keeps_existing_metadata (before : State) (fault : Fault)
    (budget : Nat) (accepted : (begin before fault budget).outcome = .started) :
    (begin before fault budget).after =
      ⟨.caller, registerFingerprint before.fingerprint,
        some ⟨requestedDigest, budget, false, false⟩⟩ := by
  obtain ⟨owner, fingerprint, row⟩ := before
  cases owner <;> cases fault <;> cases row <;> simp_all [begin]

theorem fingerprint_registration_preserves_legacy_version :
    registerFingerprint .legacy = .legacy := rfl

theorem no_stale_budget_after_failed_commit (before : State) (first retry : Nat)
    (rejected : (begin before .commitReject first).outcome = .commitError) :
    begin (begin before .commitReject first).after .none retry =
      ⟨.started, ⟨.caller, registerFingerprint before.fingerprint,
        some ⟨requestedDigest, retry, false, false⟩⟩⟩ := by
  obtain ⟨owner, fingerprint, row⟩ := before
  cases owner <;> cases row <;> simp_all [begin]

theorem repeat_after_success_keeps_original_registration (before : State) (fault : Fault)
    (first retry : Nat) (accepted : (begin before fault first).outcome = .started) :
    begin (begin before fault first).after .none retry =
      ⟨.duplicateRun, (begin before fault first).after⟩ := by
  rw [success_registers_run_and_keeps_existing_metadata before fault first accepted]
  simp [begin]

end HoiminOracle.BeginTransaction
