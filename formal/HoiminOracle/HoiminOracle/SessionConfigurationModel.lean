import HoiminOracle.SessionAdmissionModel

namespace HoiminOracle.SessionConfiguration

open HoiminOracle.SessionAdmission

inductive Journal where
  | rollback | wal
  deriving DecidableEq, BEq

structure Durable where
  version : Int
  objects : Objects
  rows : Nat
  journal : Journal
  deriving DecidableEq

def fresh : Durable := ⟨0, .empty, 0, .rollback⟩

def initialized (previous : Durable) (rows : Nat) : Durable :=
  { previous with version := currentVersion, objects := .canonical, rows }

inductive JournalReply where
  | available | busy | refused
  deriving DecidableEq, BEq

structure Attempt where
  peerInitializes : Bool := false
  fullDuringSchema : Bool := false
  journal : JournalReply := .available

inductive Outcome where
  | opened | unsupportedVersion (version : Int) | invalidIdentity | diskFull | busy | journalRefused
  deriving DecidableEq, BEq

structure Observation where
  firstVersion : Option Int
  writerVersion : Option Int
  beforeJournal : Option Durable
  outcome : Outcome
  after : Durable
  deriving DecidableEq

def journalResult (committed : Durable) : JournalReply → Outcome × Durable
  | .available => (.opened, { committed with journal := .wal })
  | .busy => (.busy, committed)
  | .refused => (.journalRefused, committed)

def run (before : Durable) (attempt : Attempt) : Observation :=
  match admitImage ⟨before.version, 0, before.objects⟩ with
  | some (.unsupportedVersion version) => ⟨none, none, none, .unsupportedVersion version, before⟩
  | some _ => ⟨none, none, none, .invalidIdentity, before⟩
  | none =>
    let visible := if before.version = 0 && attempt.peerInitializes then initialized before 1 else before
    let writerVersion := if before.version = 0 then some visible.version else none
    if writerVersion = some 0 && attempt.fullDuringSchema then
      ⟨some before.version, writerVersion, none, .diskFull, visible⟩
    else
      let committed := if writerVersion = some 0 then initialized visible 0 else visible
      let (outcome, after) := journalResult committed attempt.journal
      ⟨some before.version, writerVersion, some committed, outcome, after⟩

theorem journal_failure_preserves_committed_state (committed : Durable) (reply : JournalReply)
    (failed : (journalResult committed reply).1 ≠ .opened) :
    (journalResult committed reply).2 = committed := by
  cases reply <;> simp_all [journalResult]

theorem journal_success_preserves_schema_and_rows (committed : Durable) (reply : JournalReply)
    (opened : (journalResult committed reply).1 = .opened) :
    let durable := (journalResult committed reply).2
    durable.version = committed.version ∧ durable.objects = committed.objects ∧
      durable.rows = committed.rows ∧ durable.journal = .wal := by
  cases reply <;> simp_all [journalResult]

theorem failed_initialization_skips_journal (journal : JournalReply) :
    run fresh { fullDuringSchema := true, journal } = ⟨some 0, some 0, none, .diskFull, fresh⟩ := by
  rfl

theorem busy_after_initialization_preserves_schema_for_reopen :
    let failure := run fresh { journal := .busy }
    failure.outcome = .busy ∧ failure.after = initialized fresh 0 ∧
      (run failure.after {}).writerVersion = none ∧ (run failure.after {}).outcome = .opened := by
  decide

theorem writer_reread_preserves_peer_initialization (journal : JournalReply) :
    let result := run fresh { peerInitializes := true, fullDuringSchema := true, journal }
    result.writerVersion = some currentVersion ∧ result.beforeJournal = some (initialized fresh 1) ∧
      result.after.rows = 1 := by
  cases journal <;> decide

end HoiminOracle.SessionConfiguration
