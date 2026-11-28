import HoiminOracle.StoredResultModel
import Lean.Data.Json

namespace HoiminOracle.StoredResult

open HoiminOracle.SessionAudit (Status)

private structure Case where
  id : String
  candidate : Bool
  row : OutcomeRow
  run : RunRow := .incomplete
  owned : Bool := true

private def termJson : Termination → Lean.Json
  | .exit code => Lean.Json.mkObj [("Exit", Lean.toJson code)]
  | .timeout => .str "Timeout"
  | .outOfMemory => .str "OutOfMemory"
  | .processLimit => .str "ProcessLimit"
  | .cancelled => .str "Cancelled"

private def executed (termination : Termination) (closed : Bool) (elapsed : Lean.Json) : Lean.Json :=
  Lean.Json.mkObj [("executed", Lean.Json.mkObj
    [("termination", termJson termination), ("output_state", .str (if closed then "complete" else "close_timed_out"))
    , ("output", Lean.Json.mkObj [("token", .str "saved-output"), ("retained", Lean.toJson (3 : Nat)),
         ("observed", Lean.toJson (5 : Nat))])
    , ("elapsed", elapsed) ])]

private def duration (seconds nanos : Nat) : Lean.Json :=
  Lean.Json.mkObj [("secs", Lean.toJson seconds), ("nanos", Lean.toJson nanos)]

private def results : List (String × OutcomeRow) :=
  let terms : List (String × Termination) :=
    [("exit-zero", .exit 0), ("exit-positive", .exit 1), ("exit-negative", .exit (-1)),
      ("timeout", .timeout), ("out-of-memory", .outOfMemory), ("process-limit", .processLimit),
      ("cancelled", .cancelled)]
  [("absent", .absent), ("blob", .blob), ("reused-killed", .reused true),
    ("reused-survived", .reused false), ("not-run", .notRun)] ++
  terms.flatMap (fun (name, term) =>
    [(name ++ "-closed", .executed term true), (name ++ "-output-timeout", .executed term false)]) ++
  [("invalid-json", .invalid "garbage")
  , ("null-json", .invalid "null")
  , ("unknown-reused-status", .invalid "{\"reused\":{\"status\":\"timeout\"}}")
  , ("extra-reused-field", .invalid "{\"reused\":{\"status\":\"killed\",\"elapsed\":null}}")
  , ("no-output-evidence", .invalid "{\"executed\":{\"termination\":{\"Exit\":0}}}")
  , ("array-duration", .invalid (executed (.exit 0) true (Lean.toJson ([0, 5] : List Nat))).compress)
  , ("oversized-nanos", .invalid (executed (.exit 0) true (duration 0 1000000000)).compress)
  , ("overflow-seconds", .invalid (executed .cancelled true (duration 18446744073709551616 0)).compress) ]

private def active : List Case :=
  [false, true].flatMap fun candidate =>
    results.map fun (name, row) =>
      { id := (if candidate then "candidate-" else "no-candidate-") ++ name, candidate, row }

private def gates : List Case :=
  let runs : List (String × RunRow × Bool) :=
    [("unowned", .incomplete, false), ("complete", .complete, true),
      ("invalid-flag", .invalidFlag, true), ("wrong-type", .wrongType, true), ("missing-run", .missing, true)]
  let rows : List (String × Bool × OutcomeRow) :=
    [("empty", false, .absent), ("invalid", true, .invalid "garbage"),
      ("orphan", false, .invalid "garbage"), ("blob", true, .blob), ("conclusive", true, .reused true)]
  runs.flatMap fun (gate, run, owned) =>
    rows.map fun (name, candidate, row) => ⟨gate ++ "-" ++ name, candidate, row, run, owned⟩

private def cases := active ++ gates

private def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

private def problemName (lookup : Bool) : Problem → String
  | .owner => if lookup then "session.lookup.owner" else "session.persist.owner"
  | .missingRun => "session.lookup.state"
  | .complete => if lookup then "session.lookup.complete" else "session.persist.complete"
  | .invalidFlag => "invalid complete flag"
  | .runRead | .resultRead => if lookup then "session.read" else "session.persist"
  | .inconsistentRows => "candidate and result rows are inconsistent"
  | .invalidOutcome => "invalid mutation outcome"
  | .duplicate => "session.duplicate_result"
  | .commit => "session.commit"

private def lookupName : Except Problem (Option Status) → String
  | .error reason => problemName true reason
  | .ok none => "missing"
  | .ok (some status) => statusName status

private def persistName : Except Problem Unit → String
  | .error reason => problemName false reason
  | .ok () => "stored"

private def reusable : Except Problem (Option Status) → Bool
  | .ok (some status) => status.isDeterminate
  | _ => false

private def observe (item : Case) : String × String × Bool :=
  let found := lookup item.owned item.run item.candidate item.row
  (lookupName found, persistName (persist item.owned item.run item.candidate item.row), reusable found)

private def rowJson : OutcomeRow → Lean.Json
  | .absent => Lean.Json.mkObj [("kind", .str "absent")]
  | .blob => Lean.Json.mkObj [("kind", .str "blob")]
  | row =>
    let encoded := match row with
      | .executed term closed => (executed term closed (duration 0 999999999)).compress
      | .reused killed => (Lean.Json.mkObj [("reused", Lean.Json.mkObj
            [("status", .str (if killed then "killed" else "survived"))])]).compress
      | .notRun => "\"not_run\""
      | .invalid raw => raw
      | _ => ""
    Lean.Json.mkObj [("kind", .str "text"), ("json", .str encoded)]

private def runName : RunRow → String
  | .missing => "missing"
  | .incomplete => "incomplete"
  | .complete => "complete"
  | .invalidFlag => "invalid_flag"
  | .wrongType => "wrong_type"

def corpus : String :=
  String.join (cases.map fun item =>
    let (lookup, persist, reuse) := observe item
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("owned", .bool item.owned), ("run", .str (runName item.run))
      , ("candidate", .bool item.candidate), ("row", rowJson item.row)
      , ("lookup", .str lookup), ("persist", .str persist), ("reuse", .bool reuse) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → String × String × Bool)) :=
    [ ("ignore_ownership", fun item => observe { item with owned := true })
    , ("ignore_completion", fun item =>
        observe { item with run := if item.run = .complete then .incomplete else item.run })
    , ("ignore_missing_candidate", fun item =>
        match item.row with | .absent => observe item | _ => observe { item with candidate := true })
    , ("ignore_output_close_timeout", fun item =>
        match item.row with
        | .executed term _ => observe { item with row := .executed term true }
        | _ => observe item)
    , ("reuse_inconclusive_outcome", fun item =>
        let (found, stored, reuse) := observe item
        (found, stored, reuse || found == "timeout"))
    , ("replace_conclusive_result", fun item =>
        let (found, stored, reuse) := observe item
        (found, if stored == "session.duplicate_result" then "stored" else stored, reuse)) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => variant item != observe item) then
      IO.println s!"stored result sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"stored result missed variant: {name}"
      return 1
  IO.println s!"stored_result_cases={cases.length}"
  return 0

end HoiminOracle.StoredResult
