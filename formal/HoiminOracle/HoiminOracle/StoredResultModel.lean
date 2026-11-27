import HoiminOracle.SessionModel

namespace HoiminOracle.StoredResult

open HoiminOracle.SessionAudit (Status)

inductive Termination where
  | exit (code : Int)
  | timeout | outOfMemory | processLimit | cancelled

def statusOf (termination : Termination) (outputClosed : Bool) : Status :=
  if !outputClosed then .error else
    match termination with
    | .exit 0 => .survived
    | .exit _ => .killed
    | .timeout => .timeout
    | .outOfMemory => .outOfMemory
    | .processLimit => .processLimit
    | .cancelled => .notRun

inductive OutcomeRow where
  | absent | blob
  | invalid (json : String)
  | executed (termination : Termination) (outputClosed : Bool)
  | reused (killed : Bool)
  | notRun

inductive RunRow where
  | missing | incomplete | complete | invalidFlag | wrongType
  deriving DecidableEq

inductive Problem where
  | owner | missingRun | complete | invalidFlag | runRead | resultRead
  | inconsistentRows | invalidOutcome | duplicate | commit
  deriving DecidableEq

def decode (candidate : Bool) (row : OutcomeRow) : Except Problem (Option Status) :=
  match row with
  | .blob => .error .resultRead
  | .absent => if candidate then .error .inconsistentRows else .ok none
  | _ =>
    if !candidate then .error .inconsistentRows else
      match row with
      | .invalid _ => .error .invalidOutcome
      | .executed termination outputClosed => .ok (some (statusOf termination outputClosed))
      | .reused killed => .ok (some (if killed then .killed else .survived))
      | .notRun => .ok (some .notRun)
      | _ => .error .inconsistentRows

def checkRun (owned : Bool) (row : RunRow) (isLookup : Bool) : Except Problem Unit :=
  if !owned then .error .owner else
    match row with
    | .missing => if isLookup then .error .missingRun else .ok ()
    | .incomplete => .ok ()
    | .complete => .error .complete
    | .invalidFlag => .error .invalidFlag
    | .wrongType => .error .runRead

def lookup (owned : Bool) (run : RunRow) (candidate : Bool) (row : OutcomeRow) :
    Except Problem (Option Status) := do
  checkRun owned run true
  decode candidate row

def persist (owned : Bool) (run : RunRow) (candidate : Bool) (row : OutcomeRow) :
    Except Problem Unit := do
  checkRun owned run false
  let existing ← decode candidate row
  if existing.any Status.isDeterminate then throw .duplicate
  if run = .missing then throw .commit

theorem incomplete_output_is_always_error (termination : Termination) :
    statusOf termination false = .error := rfl

theorem lookup_requires_ownership (run : RunRow) (candidate : Bool) (row : OutcomeRow) :
    lookup false run candidate row = .error .owner := rfl

theorem persistence_requires_ownership (run : RunRow) (candidate : Bool) (row : OutcomeRow) :
    persist false run candidate row = .error .owner := rfl

theorem orphan_text_is_rejected_before_payload_validation (json : String) :
    decode false (.invalid json) = .error .inconsistentRows := rfl

theorem healthy_empty_pair_has_no_reusable_result :
    lookup true .incomplete false .absent = .ok none := rfl

theorem conclusive_result_cannot_be_replaced (row : OutcomeRow) (status : Status)
    (found : decode true row = .ok (some status))
    (conclusive : status.isDeterminate = true) :
    persist true .incomplete true row = .error .duplicate := by
  simp [persist, checkRun, found, bind, Except.bind, conclusive, throw, throwThe, MonadExceptOf.throw]

end HoiminOracle.StoredResult
