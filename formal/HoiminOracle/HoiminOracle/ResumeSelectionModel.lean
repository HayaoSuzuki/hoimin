import HoiminOracle.ResumeDiagnosticModel

namespace HoiminOracle.ResumeSelection

inductive Budget where
  | valid (value : Nat)
  | zero | short | wrongType
  deriving DecidableEq, BEq

inductive FingerprintSchema where
  | current | legacy | future | wrongType | missing
  deriving DecidableEq, BEq

structure Row where
  name : String
  fingerprint : Nat
  budget : Budget
  complete : Bool := false
  active : Bool := false
  schema : FingerprintSchema := .current

inductive Result where
  | resumed (name : String)
  | fresh (reason : String)
  | incompatible | schemaType | budgetType | budgetLength | budgetValue | active
  deriving DecidableEq, BEq

def budgetEligible (limit : Nat) : Budget → Bool
  | .valid value => decide (value ≤ limit)
  | _ => true

def eligible (fingerprint limit : Nat) (row : Row) : Bool :=
  row.fingerprint == fingerprint && !row.complete && budgetEligible limit row.budget

def pick (fingerprint limit : Nat) (rows : List Row) : Option Row :=
  rows.reverse.find? (eligible fingerprint limit)

def checkCandidate (row : Row) : Result :=
  match row.budget with
  | .wrongType => .budgetType
  | .short => .budgetLength
  | .zero => .budgetValue
  | .valid _ => if row.active then .active else .resumed row.name

def newestJoinedIncomplete (rows : List Row) : Option Row :=
  rows.reverse.find? fun row => !row.complete && row.schema != .missing

def historyReason (fingerprint limit : Nat) (rows : List Row) : String :=
  let decreased := rows.any fun row => row.fingerprint == fingerprint && !row.complete &&
    match row.budget with | .valid value => decide (limit < value) | _ => false
  HoiminOracle.ResumeDiagnostic.historyReason false decreased
    (rows.any fun row => row.fingerprint == fingerprint && row.complete)
    (rows.any fun row => !row.complete) (!rows.isEmpty)

def fallback (fingerprint limit : Nat) (rows : List Row) : Result :=
  match (newestJoinedIncomplete rows).map Row.schema with
  | some .legacy => .incompatible
  | some .wrongType => .schemaType
  | _ => .fresh (historyReason fingerprint limit rows)

def choose (fingerprint limit : Nat) (rows : List Row) : Result :=
  match pick fingerprint limit rows with
  | some row => checkCandidate row
  | none => fallback fingerprint limit rows

def budgetAfter (limit : Nat) (result : Result) (row : Row) : Budget :=
  if result = .resumed row.name then .valid limit else row.budget

theorem picked_candidate_short_circuits_fallback (fingerprint limit : Nat) (rows : List Row)
    (row : Row) (picked : pick fingerprint limit rows = some row) :
    choose fingerprint limit rows = checkCandidate row := by simp [choose, picked]

theorem invalid_budget_precedes_active_lock (row : Row) (bad : row.budget = .wrongType) :
    checkCandidate row = .budgetType := by simp [checkCandidate, bad]

theorem active_candidate_rejects_without_falling_back (fingerprint limit : Nat) (rows : List Row)
    (row : Row) (picked : pick fingerprint limit rows = some row) (value : Nat)
    (valid : row.budget = .valid value) (held : row.active = true) :
    choose fingerprint limit rows = .active := by
  simp [choose, picked, checkCandidate, valid, held]

theorem legacy_history_precedes_fresh_reason (fingerprint limit : Nat) (rows : List Row)
    (noCandidate : pick fingerprint limit rows = none)
    (legacy : (newestJoinedIncomplete rows).map (·.schema) = some .legacy) :
    choose fingerprint limit rows = .incompatible := by simp [choose, noCandidate, fallback, legacy]

theorem failed_selection_preserves_each_budget (limit : Nat) (result : Result) (row : Row)
    (unchanged : result ≠ .resumed row.name) : budgetAfter limit result row = row.budget := by
  simp [budgetAfter, unchanged]

end HoiminOracle.ResumeSelection
