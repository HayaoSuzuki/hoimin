import HoiminOracle.PlanDiscoveryBudgetModel
import Lean.Data.Json

namespace HoiminOracle.PlanDiscoveryBudget

open HoiminOracle.BoundedCandidateDiscovery

private def filePath (index : Nat) : String := s!"src/file-{index}.py"

private def fileAt (count index : Nat) : File :=
  let path := filePath index
  ⟨path, (List.range' 1 count).map fun line =>
    { identity := line, orderKey := line, producer := .token, path, line }⟩

private def filesFor (counts : List Nat) : List File :=
  counts.zipIdx.map fun (count, index) => fileAt count index

private structure Position where
  path : String
  line : Nat
  deriving BEq

private def position (candidate : Candidate) : Position :=
  ⟨candidate.path, candidate.line⟩

private structure Case where
  id : String
  counts : List Nat
  limit : Nat
  requested : List Position

private def requests (files : List File) (saved : List Position) : List (List Position) :=
  if saved.isEmpty then [[]]
  else
    let singletons := saved.map fun candidate => [candidate]
    let byFile := files.map fun file => saved.filter (fun row => row.path == file.path)
    let finalFiles := saved.filter (fun row => !(files.head?.any (fun file => file.path == row.path)))
    (saved :: finalFiles :: (singletons ++ byFile)).filter (fun rows => !rows.isEmpty) |>.eraseDups

private def cases : List Case :=
  let inventories := [[2, 0, 3], [0, 1, 2], [2, 2, 2], [3, 1, 0, 2], [0, 0, 0], [2, 0, 0]]
  inventories.zipIdx.flatMap fun (counts, index) =>
    let files := filesFor counts
    (List.range' 1 (counts.sum + 1)).flatMap fun limit =>
      let saved := (discover files limit).candidates.map position
      (requests files saved).zipIdx.map fun (requested, number) =>
        ⟨s!"workspace-{index}-cap-{limit}-request-{number}", counts, limit, requested⟩

private def positionJson (row : Position) : Lean.Json :=
  Lean.Json.mkObj [("path", .str row.path), ("line", Lean.toJson row.line)]

private def positionsJson (rows : List Position) : Lean.Json :=
  .arr (rows.map positionJson).toArray

private def discoveryJson (files : List File) (result : TargetState) : Lean.Json :=
  let overflow := if result.truncated then
    (files[result.targetsRead - 1]?).map File.path else none
  Lean.Json.mkObj [("positions", positionsJson (result.candidates.map position)),
    ("truncated", Lean.toJson result.truncated), ("overflow_path", Lean.toJson overflow)]

private def selectedFiles (item : Case) (files : List File) : List File :=
  restrict files fun path => item.requested.any (fun row => row.path == path)

private def outcomeJson (item : Case) (files : List File) (plan : TargetState)
    (narrow : List File) (rediscovery : TargetState) : Lean.Json :=
  let confirmed := (rediscovery.candidates.map position).filter item.requested.contains
  Lean.Json.mkObj [("plan", discoveryJson files plan),
    ("exit_code", Lean.toJson (if plan.truncated then (4 : Nat) else 0)),
    ("rediscovery", if item.requested.isEmpty then .null else discoveryJson narrow rediscovery),
    ("confirmed", positionsJson confirmed)]

private def outcome (item : Case) : Lean.Json :=
  let files := filesFor item.counts
  let narrow := selectedFiles item files
  outcomeJson item files (discover files item.limit) narrow (discover narrow item.limit)

def corpus : String :=
  String.join (cases.map fun item => (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str item.id), ("counts", Lean.toJson item.counts),
    ("limit", Lean.toJson item.limit), ("requested", positionsJson item.requested),
    ("expected", outcome item)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Lean.Json)) :=
    [ ("no_target_projection", fun item =>
        let files := filesFor item.counts
        let initial := discover files item.limit
        outcomeJson item files initial files initial)
    , ("reuse_spent_capacity", fun item =>
        let files := filesFor item.counts
        let initial := discover files item.limit
        let narrow := selectedFiles item files
        outcomeJson item files initial narrow (discover narrow (item.limit - initial.candidates.length)))
    , ("project_retained_prefix_without_rediscovery", fun item =>
        let files := filesFor item.counts
        let initial := discover files item.limit
        let narrow := selectedFiles item files
        let pruned := initial.candidates.filter fun row => narrow.any (fun file => file.path == row.path)
        outcomeJson item files initial narrow { (discover narrow item.limit) with candidates := pruned })
    , ("reorder_requested_files", fun item =>
        let files := filesFor item.counts
        let narrow := (selectedFiles item files).reverse
        outcomeJson item files (discover files item.limit) narrow (discover narrow item.limit))
    , ("declare_exact_capacity_truncated", fun item =>
        let files := filesFor item.counts
        let initial := discover files item.limit
        let narrow := selectedFiles item files
        let marked := { initial with truncated := initial.truncated || initial.candidates.length == item.limit }
        outcomeJson item files marked narrow (discover narrow item.limit))
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => (variant item).compress != (outcome item).compress) then
      IO.println s!"plan discovery budget sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"plan discovery budget missed variant: {name}"
      return 1
  IO.println s!"plan_discovery_budget_cases={cases.length} public_previews={(cases.filter (fun item => !item.requested.isEmpty)).length}"
  return 0

end HoiminOracle.PlanDiscoveryBudget
