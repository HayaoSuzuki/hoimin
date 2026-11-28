import HoiminOracle.ReviewListingModel
import Lean.Data.Json

namespace HoiminOracle.ReviewListing

private structure Case where
  id : String
  sources : List Source := [.fresh, .fresh, .fresh]
  stored : List Stored := []
  reported : Option (List Reported) := none
  includeRetired : Bool := false

private def candidate (id : Nat) : Candidate :=
  ⟨id, if id = 0 then .booleanLiteral else .integerLiteralNeighbor, id + 1⟩

private def history (id : Nat) (assessments : List Assessment) : Stored :=
  ⟨candidate id, true, assessments⟩

private def partialHistory : List Stored :=
  [history 4 [.needsInvestigation], history 0 [.actionable, .notUseful], history 3 [.actionable]]

private def cases : List Case :=
  let profiles :=
    [("fresh", [Source.fresh, .fresh, .fresh]), ("mixed", [.stale, .unavailable, .fresh]),
      ("mixed-reversed", [.unavailable, .fresh, .stale]), ("changed", [.fresh, .stale, .unavailable])]
  let histories :=
    [("empty", []), ("partial", partialHistory), ("all",
      history 2 [.needsInvestigation] :: history 1 [.notUseful, .actionable] :: partialHistory)]
  let reports : List (String × Option (List Reported)) :=
    [("unloaded", none), ("empty", some []),
      ("partial", some [⟨4, true, .survived⟩, ⟨1, true, .killed⟩, ⟨0, true, .survived⟩]),
      ("all", some ([4, 0, 2, 3, 1].map fun id => ⟨id, true, .killed⟩))]
  let matrix := profiles.flatMap fun (profile, sources) => histories.flatMap fun (store, stored) =>
    reports.flatMap fun (report, reported) => [true, false].map fun includeRetired =>
      ⟨s!"{profile}-{store}-{report}-{includeRetired}", sources, stored, reported, includeRetired⟩
  let conflictingReport := some [⟨3, false, Status.killed⟩]
  let conflictingStore := { history 0 [.actionable] with descriptorMatches := false } ::
    [history 3 [.actionable]]
  matrix ++
    [ { id := "stored-conflict-fresh-before-retired", stored := conflictingStore,
        reported := conflictingReport, includeRetired := true }
    , { id := "stored-conflict-stale", sources := [.stale, .fresh, .fresh], stored := conflictingStore }
    , { id := "stored-conflict-unavailable", sources := [.unavailable, .fresh, .fresh], stored := conflictingStore }
    , { id := "retired-report-conflict-visible", stored := partialHistory,
        reported := conflictingReport, includeRetired := true }
    , { id := "retired-report-conflict-hidden", stored := partialHistory, reported := conflictingReport }
    , { id := "report-conflict-without-stored-candidate", reported := conflictingReport, includeRetired := true }
    ]

private def evaluate (item : Case) : Except Failure Listing :=
  let planned := item.sources.zipIdx.map fun (source, id) => ⟨candidate id, source⟩
  -- IDs 3 and 4 are labeled in the order of the stored keys in the adapter.
  let snapshot := item.stored.mergeSort (fun a b => a.candidate.id ≤ b.candidate.id)
  listReviews planned snapshot item.reported item.includeRetired

private def sourceName : Source → String
  | .fresh => "current"
  | .stale => "stale"
  | .unavailable => "source_unavailable"

private def assessmentName : Assessment → String
  | .actionable => "actionable"
  | .notUseful => "not_useful"
  | .needsInvestigation => "needs_investigation"

private def statusName : Status → String
  | .killed => "killed"
  | .survived => "survived"

private def operatorName : Operator → String
  | .booleanLiteral => "boolean_literal"
  | .integerLiteralNeighbor => "integer_literal_neighbor"

private def entries (values : List Lean.Json) : Lean.Json := .arr values.toArray

private def storedJson (row : Stored) : Lean.Json :=
  Lean.Json.mkObj [
    ("id", Lean.toJson row.candidate.id),
    ("descriptor_matches", Lean.toJson row.descriptorMatches),
    ("history", Lean.toJson (row.history.map assessmentName))]

private def reportJson (row : Reported) : Lean.Json :=
  Lean.Json.mkObj [
    ("id", Lean.toJson row.id), ("descriptor_matches", Lean.toJson row.descriptorMatches),
    ("status", .str (statusName row.status))]

private def executionJson : Execution → Lean.Json
  | .notLoaded => Lean.Json.mkObj [("state", .str "not_loaded")]
  | .notInReport => Lean.Json.mkObj [("state", .str "not_in_report")]
  | .result status => Lean.Json.mkObj [("state", .str "result"), ("status", .str (statusName status))]

private def rowJson (row : Row) : Lean.Json :=
  Lean.Json.mkObj [
    ("id", Lean.toJson row.candidate.id), ("sequence", Lean.toJson row.candidate.sequence),
    ("operator", .str (operatorName row.candidate.operator)), ("applicability", .str (sourceName row.source)),
    ("history", Lean.toJson (row.history.map assessmentName)), ("review", Lean.toJson (row.review.map assessmentName)),
    ("execution", executionJson row.execution)]

private def countsJson (operator : Operator) (counts : Counts) : Lean.Json :=
  let assessmentCounts := ([.actionable, .notUseful, .needsInvestigation] : List Assessment).filterMap fun assessment =>
    let count := (counts.assessments.filter (· == assessment)).length
    if count = 0 then none else some (assessmentName assessment, Lean.toJson count)
  Lean.Json.mkObj [
    ("operator", .str (operatorName operator)), ("total", Lean.toJson counts.total),
    ("reviewed", Lean.toJson counts.reviewed), ("unreviewed", Lean.toJson counts.unreviewed),
    ("stale", Lean.toJson counts.stale), ("source_unavailable", Lean.toJson counts.unavailable),
    ("assessments", Lean.Json.mkObj assessmentCounts)]

private def outcomeJson : Except Failure Listing → Lean.Json
  | .error failure => Lean.Json.mkObj [
      ("kind", .str "error"), ("failure", .str (match failure with
        | .storedIdentity => "stored_identity" | .reportDescriptor => "report_descriptor"))]
  | .ok listing => Lean.Json.mkObj [
      ("kind", .str "listed"), ("candidates", entries (listing.current.map rowJson)),
      ("stale_candidates", entries (listing.retired.map rowJson)),
      ("operators", entries (listing.operators.map fun (operator, counts) => countsJson operator counts))]

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
      ("sources", Lean.toJson (item.sources.map sourceName)), ("stored", entries (item.stored.map storedJson)),
      ("reported", item.reported.elim .null (entries ∘ List.map reportJson)),
      ("include_retired", Lean.toJson item.includeRetired), ("expected", outcomeJson (evaluate item))
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let mapListing := fun change item => (evaluate item).map change
  let variants : List (String × (Case → Except Failure Listing)) :=
    [ ("show_inactive_review", mapListing fun result =>
        { result with current := result.current.map fun row => { row with review := row.history.getLast? } })
    , ("hide_inactive_history", mapListing fun result =>
        { result with current := result.current.map fun row =>
            if row.source = .fresh then row else { row with history := [] } })
    , ("show_first_review", mapListing fun result =>
        { result with current := result.current.map fun row =>
            if row.source = .fresh then { row with review := row.history.head? } else row })
    , ("unloaded_report_is_empty", fun item => evaluate { item with reported := some (item.reported.getD []) })
    , ("count_retired_candidates", mapListing fun result =>
        { result with operators := operatorCounts (result.current ++ result.retired) })
    , ("reverse_retired_order", mapListing fun result => { result with retired := result.retired.reverse })
    , ("ignore_retired_report_descriptor", fun item =>
        let reported := item.reported.map (List.map fun row => { row with descriptorMatches := true })
        evaluate { item with reported })
    , ("ignore_stored_descriptor", fun item =>
        evaluate { item with stored := item.stored.map fun row => { row with descriptorMatches := true } })
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => outcomeJson (variant item) != outcomeJson (evaluate item)) then
      IO.println s!"review listing sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"review listing missed variant: {name}"
      return 1
  IO.println s!"review_listing_cases={cases.length}"
  return 0

end HoiminOracle.ReviewListing
