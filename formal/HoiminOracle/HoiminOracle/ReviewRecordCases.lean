import HoiminOracle.ReviewRecordModel
import Lean.Data.Json

namespace HoiminOracle.ReviewRecord

private def req (candidate : Candidate) (expected : Nat) (mode : Mode := .record) : Request :=
  ⟨candidate, expected, 0, mode, .fresh⟩

private def cases : List (String × List Request) :=
  let alphabet :=
    [ ("first-0", req .first 0), ("first-1", req .first 1)
    , ("second-0", req .second 0), ("second-1", req .second 1)
    , ("first-rollback", req .first 0 .rollback), ("second-rollback", req .second 0 .rollback)
    , ("first-output", req .first 0 .failWrite), ("second-output", req .second 0 .failWrite)
    , ("first-newline", req .first 0 .failNewline), ("second-newline", req .second 0 .failNewline)
    ]
  let pairs := alphabet.flatMap fun (first, a) => alphabet.map fun (second, b) =>
    (s!"{first}-then-{second}", [a, b])
  let fixed :=
    [ ("rollback-then-retry-current-revision",
        [req .first 0 .rollback, req .first 0, req .first 1 .rollback, req .first 1])
    , ("both-output-failures-and-duplicate-retries",
        [req .first 0 .failWrite, req .first 0, req .first 1 .failNewline, req .first 1 .failWrite, req .first 2])
    , ("revisions-are-per-key",
        [req .second 0 .failNewline, req .first 0, req .second 0, req .first 1, req .second 1])
    , ("locked-before-first-event", [req .first 0 .busy, req .first 0])
    , ("locked-with-history", [req .first 0, req .first 1 .busy, req .second 0, req .first 1])
    , ("source-gates-store-creation", [
        { req .first 1 with source := .stale }, { req .second 0 with source := .unavailable }, req .first 0])
    , ("source-changes-after-output-loss", [
        req .first 0 .failWrite, { req .first 0 with source := .stale },
        { req .second 0 with source := .unavailable }, req .first 0, req .first 1])
    , ("revision-check-precedes-injected-rollback",
        [req .first 1 .rollback, req .first 0, req .first 0 .rollback, req .first 1])
    ]
  (pairs ++ fixed).map fun (id, requests) =>
    (id, requests.zipIdx.map fun (request, index) => { request with marker := index + 1 })

private def requestJson (request : Request) : Lean.Json :=
  Lean.Json.mkObj [
    ("candidate", .str (if request.candidate == .first then "first" else "second")),
    ("expected_revision", Lean.toJson request.expected), ("marker", Lean.toJson request.marker),
    ("mode", .str (match request.mode with
      | .record => "record" | .rollback => "rollback" | .busy => "busy"
      | .failWrite => "fail_write" | .failNewline => "fail_newline")),
    ("source", .str (match request.source with
      | .fresh => "fresh" | .stale => "stale" | .unavailable => "unavailable"))]

private def eventListJson (events : List Event) : Lean.Json :=
  .arr (events.toArray.map fun event =>
    Lean.Json.mkObj [("revision", Lean.toJson event.revision), ("marker", Lean.toJson event.marker)])

private def replyJson (reply : Reply) : Lean.Json :=
  let base := fun name => [("kind", Lean.Json.str name)]
  Lean.Json.mkObj (match reply with
    | .recorded revision => base "recorded" ++ [("revision", Lean.toJson revision)]
    | .savedOutput revision => base "saved_output" ++ [("revision", Lean.toJson revision)]
    | .notCurrent => base "not_current"
    | .revisionConflict => base "revision_conflict"
    | .rolledBack => base "rolled_back"
    | .busy => base "busy")

private def observationJson (result : Observation) : Lean.Json :=
  Lean.Json.mkObj [
    ("store", Lean.Json.mkObj [
      ("present", Lean.toJson result.store.present),
      ("first", eventListJson result.store.first),
      ("second", eventListJson result.store.second)]),
    ("reply", replyJson result.reply),
    ("output", .str (match result.output with
      | .empty => "empty" | .document => "document" | .line => "line"))]

def corpus : String :=
  String.join (cases.map fun (id, requests) =>
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
      ("requests", .arr (requests.toArray.map requestJson)),
      ("observations", .arr ((replay {} requests).toArray.map observationJson))
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Store → Request → Observation)) :=
    [ ("rollback_leaves_committed_event", fun store request =>
        let actual := step store request
        if actual.reply == .rolledBack then
          { actual with store := (step store { request with mode := .record }).store }
        else actual)
    , ("output_failure_rolls_back_commit", fun store request =>
        let actual := step store request
        match actual.reply with
        | .savedOutput _ => { actual with store := { store with present := true } }
        | _ => actual)
    , ("lost_receipt_names_previous_revision", fun store request =>
        let actual := step store request
        match actual.reply with
        | .savedOutput revision => { actual with reply := .savedOutput (revision - 1) }
        | _ => actual)
    , ("retry_ignores_expected_revision", fun store request =>
        step store { request with expected := store.revision request.candidate })
    , ("use_store_wide_revision", fun store request =>
        if request.source == .fresh && request.mode != .busy &&
            store.first.length + store.second.length != request.expected then
          ⟨{ store with present := true }, .revisionConflict, .empty⟩
        else step store request)
    , ("busy_attempt_writes_event", fun store request =>
        let actual := step store request
        if actual.reply == .busy then
          { actual with store := (step store { request with mode := .record }).store }
        else actual)
    , ("ignore_stale_source", fun store request => step store { request with source := .fresh })
    , ("newline_failure_loses_document", fun store request =>
        let actual := step store request
        if request.mode == .failNewline then { actual with output := .empty } else actual)
    ]
  for (name, variant) in variants do
    if let some (id, _) := cases.find? (fun (_, requests) =>
        replayWith variant {} requests != replay {} requests) then
      IO.println s!"review record sensitivity: {name}: {id}"
    else
      IO.eprintln s!"review record missed variant: {name}"
      return 1
  IO.println s!"review_record_cases={cases.length}"
  return 0

end HoiminOracle.ReviewRecord
