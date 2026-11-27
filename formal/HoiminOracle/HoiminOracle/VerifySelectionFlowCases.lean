import HoiminOracle.VerifySelectionFlowModel
import Lean.Data.Json

namespace HoiminOracle.VerifySelectionFlow

open HoiminOracle.LineDiverse (Candidate Policy)

private inductive Shape | complete | truncated | empty | emptyTruncated
  deriving BEq

private def discovered : List String :=
  ["a-add", "a-first", "a-second", "a-next", "b-first", "b-second", "b-add"]

private def ranked : List Candidate :=
  [ ⟨"a-first", "src/a.py", 2, 1, 100⟩, ⟨"a-second", "src/a.py", 2, 2, 100⟩
  , ⟨"a-next", "src/a.py", 3, 3, 100⟩, ⟨"b-first", "src/b.py", 1, 4, 100⟩
  , ⟨"b-second", "src/b.py", 1, 5, 100⟩, ⟨"a-add", "src/a.py", 1, 6, 70⟩
  , ⟨"b-add", "src/b.py", 2, 7, 70⟩ ]

private structure Case where
  id : String
  request : Request
  budget : Nat := 7
  shape : Shape := .complete

private def discoveryFor : Shape → List String
  | .complete => discovered
  | .truncated => discovered.take 6
  | .empty | .emptyTruncated => []

private def fixture (item : Case) : Plan :=
  ⟨ranked.filter (fun row => (discoveryFor item.shape).contains row.id),
    item.shape == .truncated || item.shape == .emptyTruncated, item.budget⟩

private def explicit (ids : List String) : Request := .explicit (ids.map RequestId.candidate)

private def policyName : Policy → String
  | .strict => "strict"
  | .diverse => "diverse"
  | .lineDiverse => "line-diverse"

private def topCases : List Case :=
  ([Policy.strict, .diverse, .lineDiverse]).flatMap fun policy =>
    [(0, 7), (1, 3), (4, 2), (6, 9)].map fun (offset, count) =>
      { id := s!"top-{policyName policy}-{offset}-{count}", request := .top policy offset count }

private def cases : List Case := topCases ++
  [ ⟨"explicit-order", explicit ["b-first", "a-first", "a-add", "a-next", "a-add"], 7, .complete⟩
  , ⟨"explicit-duplicates-at-budget", explicit ["b-first", "b-first", "a-add"], 2, .complete⟩
  , ⟨"explicit-budget-before-unknown", explicit ["missing", "a-first", "b-first"], 2, .complete⟩
  , ⟨"explicit-unknown", explicit ["a-first", "missing"], 2, .complete⟩
  , ⟨"explicit-malformed-before-budget-and-unknown",
      .explicit [.candidate "missing", .candidate "a-first", .malformed "bad-id", .malformed "other"], 1, .complete⟩
  , ⟨"explicit-from-truncated", explicit ["b-second", "a-add"], 2, .truncated⟩
  , ⟨"top-out-of-range", .top .lineDiverse 7 5, 1, .complete⟩
  , ⟨"top-budget-with-page", .top .diverse 1 3, 2, .complete⟩
  , ⟨"top-clipped-page-fits-budget", .top .lineDiverse 5 99, 2, .complete⟩
  , ⟨"top-from-truncated", .top .lineDiverse 2 9, 7, .truncated⟩
  , ⟨"top-empty", .top .strict 0 1, 1, .empty⟩
  , ⟨"top-offset-before-empty", .top .strict 1 1, 1, .empty⟩
  , ⟨"sample-seed-zero", .sample 3 0, 7, .complete⟩
  , ⟨"sample-seed-42", .sample 3 42, 7, .complete⟩
  , ⟨"sample-seed-max", .sample 5 0xffffffffffffffff, 7, .complete⟩
  , ⟨"sample-clipped-count", .sample 99 42, 7, .complete⟩
  , ⟨"sample-budget", .sample 99 42, 2, .complete⟩
  , ⟨"sample-truncated-before-budget", .sample 99 42, 2, .truncated⟩
  , ⟨"sample-truncated-before-empty", .sample 1 42, 2, .emptyTruncated⟩
  , ⟨"sample-empty", .sample 1 42, 2, .empty⟩
  ]

private def shapeName : Shape → String
  | .complete => "complete"
  | .truncated => "truncated"
  | .empty => "empty"
  | .emptyTruncated => "empty_truncated"

private def requestJson : Request → Lean.Json
  | .explicit ids => Lean.Json.mkObj [("kind", .str "explicit"), ("ids", Lean.toJson (ids.map requestText))]
  | .top policy offset count => Lean.Json.mkObj [("kind", .str "top"), ("policy", .str (policyName policy)),
      ("offset", Lean.toJson offset), ("count", Lean.toJson count)]
  | .sample count seed => Lean.Json.mkObj [("kind", .str "sample"), ("seed", Lean.toJson seed.toNat),
      ("count", Lean.toJson count)]

private def issueText : Issue → String
  | .malformed text => s!"invalid requested candidate id {text}"
  | .unknown id => s!"candidate id is not in the plan: {id}"
  | .offset offset population => s!"--offset {offset} is outside the {population} retained candidates"
  | .explicitBudget selected budget => s!"requested {selected} candidates exceeds max_mutants {budget}"
  | .orderedBudget selected budget => s!"selected {selected} candidates exceeds max_mutants {budget}"
  | .emptyExplicit => "at least one candidate id is required"
  | .emptyTop => "--top requires at least one candidate; the plan has no retained candidates"
  | .emptySample => "--sample requires at least one candidate; the plan is empty"
  | .truncatedSample => "--sample requires a complete plan; truncated plans cannot be sampled"
  | .sampleUnavailable => "model sampling fuel exhausted"

private def provenance (selection : Selection) : Lean.Json :=
  let (mode, policy, scope, details) : String × String × String × List (String × Lean.Json) :=
    match selection.mode with
    | .explicit => ("candidate_ids", "explicit_candidates", "explicit_candidates", [])
    | .top policy =>
      let policy := match policy with
        | .strict => "strict"
        | .diverse => "file_round_robin_v1"
        | .lineDiverse => "line_round_robin_v1"
      ("top", policy, "retained_candidates", [])
    | .sample population seed => ("sample", "splitmix64_fisher_yates_v1", "sampled_candidates",
        [("sampling", Lean.Json.mkObj [("population", Lean.toJson population), ("seed", Lean.toJson seed.toNat),
          ("selected_ids", Lean.toJson selection.ids)])])
  Lean.Json.mkObj ([("mode", .str mode), ("policy", .str policy), ("scope", .str scope),
    ("requested", Lean.toJson selection.requested), ("selected", Lean.toJson selection.ids.length),
    ("plan_truncated", Lean.toJson selection.truncated)] ++ details)

private def selectionJson (item : Case) (selection : Selection) (shown : List String)
    (started : Option (List String)) : Lean.Json :=
  let offset := match item.request with
    | .top _ offset _ => some offset
    | _ => none
  Lean.Json.mkObj [("decision", .str "accepted"), ("preview", Lean.toJson shown),
    ("started", Lean.toJson started), ("offset", Lean.toJson offset),
    ("provenance", provenance selection),
    ("run_exit", Lean.toJson (if selection.truncated then (4 : Nat) else 0))]

private def observeWith (item : Case) (plan : Plan) : Lean.Json :=
  match select plan item.request with
  | .error issue => Lean.Json.mkObj [("decision", .str "rejected"), ("message", .str (issueText issue))]
  | .ok choice =>
    let found := discoveryFor item.shape
    selectionJson item choice (preview choice found) (dispatch choice found)

private def observe (item : Case) : Lean.Json := observeWith item (fixture item)

private def rowJson (candidate : Candidate) : Lean.Json :=
  let column : Nat := match candidate.id with
    | "a-second" | "b-second" => 18
    | "b-add" => 7
    | _ => 6
  Lean.Json.mkObj [("id", .str candidate.id), ("path", .str candidate.path),
    ("line", Lean.toJson candidate.line), ("column", Lean.toJson column),
    ("operator", .str (if candidate.score == 100 then "compare_eq_ne" else "binary_add_sub"))]

def corpus : String :=
  String.join (cases.map fun item => (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str item.id), ("shape", .str (shapeName item.shape)),
    ("budget", Lean.toJson item.budget), ("request", requestJson item.request),
    ("ranked", .arr ((fixture item).ranked.map rowJson).toArray), ("expected", observe item)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for item in cases do
    match select (fixture item) item.request with
    | .error .sampleUnavailable => return 1
    | .error _ => pure ()
    | .ok choice =>
      let found := discoveryFor item.shape
      if dispatch choice found != some (preview choice found) then
        IO.eprintln s!"selection preview and dispatch differ for {item.id}"
        return 1
  let variants : List (String × (Case → Lean.Json)) :=
    [ ("allow_sampling_truncated_plan", fun item =>
        match item.request with
        | .sample _ _ => observeWith item { (fixture item) with truncated := false }
        | _ => observe item)
    , ("ignore_selection_budget", fun item => observe { item with budget := 7 })
    , ("ignore_page_offset", fun item =>
        match item.request with
        | .top policy _ count => observe { item with request := .top policy 0 count }
        | _ => observe item)
    , ("explicit_argument_order", fun item =>
        match select (fixture item) item.request with
        | .ok choice =>
          match choice.mode with
          | .explicit => selectionJson item choice choice.ids (dispatch choice (discoveryFor item.shape))
          | _ => observe item
        | .error _ => observe item)
    , ("dispatch_ordered_by_discovery", fun item =>
        match select (fixture item) item.request with
        | .ok choice =>
          let found := discoveryFor item.shape
          selectionJson item choice (preview choice found) (some (discoveredProjection choice found))
        | .error _ => observe item)
    , ("resample_with_fixed_seed", fun item =>
        match item.request with
        | .sample count _ => observe { item with request := .sample count 0 }
        | _ => observe item)
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => (variant item).compress != (observe item).compress) then
      IO.println s!"verify selection flow sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"verify selection flow missed variant: {name}"
      return 1
  IO.println s!"verify_selection_flow_cases={cases.length}"
  return 0

end HoiminOracle.VerifySelectionFlow
