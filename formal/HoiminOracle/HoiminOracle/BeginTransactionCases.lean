import HoiminOracle.BeginTransactionModel
import Lean.Data.Json

namespace HoiminOracle.BeginTransaction

private structure Case where
  before : State
  fault : Fault

private def original : RunRow := ⟨2, 255, true, false⟩
private def firstBudget : Nat := 256
private def retryBudget : Nat := 257

private def ownerName : Owner → String
  | .free => "free"
  | .caller => "caller"
  | .peer => "peer"

private def fingerprintName : Metadata → String
  | .absent => "absent"
  | .legacy => "legacy"
  | .current => "current"

private def faultName : Fault → String
  | .none => "none"
  | .transactionBusy => "transaction_busy"
  | .fingerprintAbort => "fingerprint_abort"
  | .runAbort => "run_abort"
  | .commitReject => "commit_reject"

private def outcomeName : Outcome → String
  | .started => "started"
  | .active => "active"
  | .beginError => "begin_error"
  | .fingerprintError => "fingerprint_error"
  | .runError => "run_error"
  | .duplicateRun => "duplicate_run"
  | .commitError => "commit_error"

private def Case.id (item : Case) :=
  let row := if item.before.row.isSome then "existing" else "missing"
  s!"{ownerName item.before.owner}-{fingerprintName item.before.fingerprint}-{row}-{faultName item.fault}"

private def cases : List Case :=
  ([.free, .caller, .peer] : List Owner).flatMap fun owner =>
    ([.absent, .legacy, .current] : List Metadata).flatMap fun fingerprint =>
      [none, some original].flatMap fun row =>
        ([.none, .transactionBusy, .fingerprintAbort, .runAbort, .commitReject] : List Fault).map
          fun fault => ⟨⟨owner, fingerprint, row⟩, fault⟩

private def observe (item : Case) : Observation := begin item.before item.fault firstBudget

private def rowJson (row : RunRow) : Lean.Json :=
  .mkObj [("digest", Lean.toJson row.digest), ("budget", Lean.toJson row.budget),
    ("finished", .bool row.finished), ("complete", .bool row.complete)]

private def stateJson (state : State) : Lean.Json :=
  .mkObj [("owner", .str (ownerName state.owner)),
    ("fingerprint", .str (fingerprintName state.fingerprint)),
    ("row", state.row.elim .null rowJson)]

private def observationJson (observation : Observation) : Lean.Json :=
  .mkObj [("outcome", .str (outcomeName observation.outcome)), ("after", stateJson observation.after)]

def corpus : String :=
  String.join (cases.map fun item =>
    let first := observe item
    let retry := begin first.after .none retryBudget
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("before", stateJson item.before), ("fault", .str (faultName item.fault))
      , ("digest", Lean.toJson requestedDigest), ("first_budget", Lean.toJson firstBudget)
      , ("retry_budget", Lean.toJson retryBudget)
      , ("first", observationJson first), ("retry", observationJson retry) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("start_sql_before_checking_peer", fun item =>
        if item.fault == .transactionBusy then ⟨.beginError, item.before⟩ else observe item)
    , ("check_duplicate_before_fingerprint_insert", fun item =>
        if item.before.owner != .peer && item.before.row.isSome && item.fault == .fingerprintAbort
        then ⟨.duplicateRun, item.before⟩ else observe item)
    , ("retain_new_lock_on_failure", fun item =>
        let result := observe item
        if item.before.owner == .free && result.outcome == .commitError
        then { result with after := { result.after with owner := .caller } } else result)
    , ("release_preexisting_lock_on_failure", fun item =>
        let result := observe item
        if item.before.owner == .caller && result.outcome == .commitError
        then { result with after := { result.after with owner := .free } } else result)
    , ("leak_fingerprint_after_run_insert_error", fun item =>
        let result := observe item
        let registered := registerFingerprint result.after.fingerprint
        if result.outcome == .runError || result.outcome == .duplicateRun
        then { result with after := { result.after with fingerprint := registered } } else result)
    , ("publish_run_after_failed_commit", fun item =>
        let result := observe item
        let inserted := some ⟨requestedDigest, firstBudget, false, false⟩
        if result.outcome == .commitError
        then { result with after := { result.after with row := inserted } } else result)
    , ("overwrite_legacy_metadata", fun item =>
        let result := observe item
        if result.outcome == .started
        then { result with after := { result.after with fingerprint := .current } } else result)
    , ("skip_publishing_new_lock", fun item =>
        let result := observe item
        if result.outcome == .started
        then { result with after := { result.after with owner := item.before.owner } } else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"begin transaction sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"begin transaction missed variant: {name}"
      return 1
  IO.println s!"begin_transaction_cases={cases.length}"
  return 0

end HoiminOracle.BeginTransaction
