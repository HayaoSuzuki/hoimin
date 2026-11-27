import Std

namespace HoiminOracle.ResumeRecheck

inductive Owner where
  | free | loader | peer
  deriving DecidableEq, BEq

structure Row where
  name : String
  ordinal : Nat
  fingerprint : Nat
  budget : Nat
  finished : Bool := false
  complete : Bool := false
  deriving DecidableEq

def chosen : Row := ⟨"selected", 2, 1, 255, false, false⟩
def original : List Row := [⟨"older", 1, 1, 1, false, false⟩, chosen]
def requested : Nat := 257

inductive Edit where
  | none | raiseBudget | restoreBudget | changeFingerprint | finish | complete | delete
  | insertNewer | replaceWithSameKey

def editChosen (rows : List Row) (change : Row → Row) : List Row :=
  rows.map fun row => if row.name = chosen.name then change row else row

def changeRows : Edit → List Row
  | .none | .restoreBudget => original
  | .raiseBudget => editChosen original ({ · with budget := 256 })
  | .changeFingerprint => editChosen original ({ · with fingerprint := 2 })
  | .finish => editChosen original ({ · with finished := true })
  | .complete => editChosen original ({ · with complete := true, finished := true })
  | .delete => original.filter (·.name != chosen.name)
  | .insertNewer => original ++ [⟨"newer", 3, 1, 256, false, false⟩]
  | .replaceWithSameKey =>
    (original.filter (·.name != chosen.name)) ++ [{ chosen with ordinal := 3 }]

def unchangedFields (actual : Row) : Prop :=
  actual.name = chosen.name ∧ actual.fingerprint = chosen.fingerprint ∧
    actual.complete = false ∧ actual.budget = chosen.budget

instance (row : Row) : Decidable (unchangedFields row) :=
  inferInstanceAs (Decidable (row.name = chosen.name ∧ row.fingerprint = chosen.fingerprint ∧
    row.complete = false ∧ row.budget = chosen.budget))

inductive Write where
  | normal | ignore | abort
  deriving DecidableEq, BEq

inductive Outcome where
  | resumed | changed | active | updateError
  deriving DecidableEq, BEq

structure Observation where
  outcome : Outcome
  owner : Owner
  rows : List Row
  deriving DecidableEq

def recheck (owner : Owner) (write : Write) (rows : List Row) : Observation :=
  if owner = .peer then ⟨.active, .peer, rows⟩
  else if rows.any (fun row => decide (unchangedFields row)) then
    match write with
    | .normal => ⟨.resumed, .loader, editChosen rows ({ · with budget := requested })⟩
    | .ignore => ⟨.changed, owner, rows⟩
    | .abort => ⟨.updateError, owner, rows⟩
  else ⟨.changed, owner, rows⟩

theorem rejected_update_preserves_rows_and_prior_owner (owner : Owner) (write : Write)
    (rows : List Row) (failed : (recheck owner write rows).outcome ≠ .resumed) :
    (recheck owner write rows).rows = rows ∧ (recheck owner write rows).owner = owner := by
  unfold recheck at failed ⊢
  split
  · rename_i held
    simp [held]
  · split
    · cases write <;> simp_all
    · simp

theorem no_matching_row_does_not_publish_a_new_lease (owner : Owner) (write : Write)
    (rows : List Row) (noMatch : rows.any (fun row => decide (unchangedFields row)) = false)
    (available : owner ≠ .peer) :
    recheck owner write rows = ⟨.changed, owner, rows⟩ := by
  simp [recheck, noMatch, available]

theorem peer_ownership_precedes_row_validation (write : Write) (rows : List Row) :
    recheck .peer write rows = ⟨.active, .peer, rows⟩ := by
  simp [recheck]

theorem finished_and_ordinal_do_not_enter_the_predicate (row : Row) (ordinal : Nat) (finished : Bool) :
    unchangedFields { row with ordinal, finished } ↔ unchangedFields row := Iff.rfl

end HoiminOracle.ResumeRecheck
