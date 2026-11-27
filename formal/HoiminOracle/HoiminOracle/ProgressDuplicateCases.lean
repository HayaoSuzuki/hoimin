import HoiminOracle.ProgressDuplicateModel
import HoiminOracle.ProgressDecisionModel
import Lean.Data.Json

namespace HoiminOracle.ProgressDuplicate

private def initialFiles : Files := [some 0, some 0, some 1, some 0, some 2, some 2]

private structure Case where
  id : String
  strict : Bool
  actions : List Action

private def stable (path : Nat) : Action :=
  ⟨⟨path, (contents initialFiles path).getD 0⟩, none⟩

private def strictCases : List Case :=
  let make := fun paths : List Nat =>
    ⟨"strict-" ++ String.intercalate "-" (paths.map toString), true, paths.map stable⟩
  let paths := List.range 4
  let pairs := paths.flatMap fun a => paths.map fun b => make [a, b]
  let triples := paths.flatMap fun a => paths.flatMap fun b => paths.map fun c => make [a, b, c]
  pairs ++ triples ++ [[0, 1, 3, 1, 0], [4, 0, 5], [0, 4, 5, 4, 1, 3], [4, 4, 5, 5]].map make

private def collision (path : Nat) : Action := ⟨⟨path, 0⟩, none⟩
private def missing (target : Action) (path : Nat) : Action := { target with edit := some ⟨path, none⟩ }

private def internalCases : List Case :=
  let alpha :=
    [ ("a", stable 0), ("b", stable 1), ("collision", collision 2), ("d", stable 3)
    , ("different-fingerprint-d", ⟨⟨3, 1⟩, none⟩)
    , ("different-fingerprint-a", ⟨⟨0, 1⟩, none⟩)
    , ("missing-a", missing (stable 1) 0)
    , ("rewritten-a", { collision 2 with edit := some ⟨0, some 1⟩ })
    ]
  let pairs := alpha.flatMap fun (a, first) => alpha.map fun (b, second) =>
    ⟨s!"internal-{a}-then-{b}", false, [first, second]⟩
  pairs ++
    [ ⟨"earliest-exact-after-collision", false, [collision 2, stable 0, stable 1, stable 3]⟩
    , ⟨"same-path-uses-first-occurrence", false, [stable 1, stable 0, stable 0, stable 0]⟩
    , ⟨"skip-unavailable-earliest", false, [stable 0, stable 1, missing (stable 3) 0]⟩
    , ⟨"same-path-with-missing-file", false,
        [stable 0, missing ⟨⟨0, 1⟩, none⟩ 0, stable 3]⟩
    , ⟨"both-files-missing", false, [missing (stable 0) 0, missing (stable 1) 1]⟩
    , ⟨"rewrite-earliest-and-find-next", false,
        [stable 0, stable 1, { stable 3 with edit := some ⟨0, some 1⟩ }]⟩
    , ⟨"restore-earliest-after-unavailable", false,
        [stable 0, missing (stable 1) 0, { stable 3 with edit := some ⟨0, some 0⟩ }]⟩
    ]

private def cases := strictCases ++ internalCases

private def actionJson (action : Action) : Lean.Json :=
  Lean.Json.mkObj [
    ("path", Lean.toJson action.input.path),
    ("fingerprint", Lean.toJson action.input.fingerprint),
    ("edit", match action.edit with
      | none => .null
      | some edit => Lean.Json.mkObj [("path", Lean.toJson edit.path), ("bytes", Lean.toJson edit.bytes)])]

private def duplicateJson : Option Duplicate → Lean.Json
  | none => .null
  | some duplicate => Lean.Json.mkObj [
    ("previous", Lean.toJson duplicate.previous),
    ("reason", .str (match duplicate.reason with
      | .samePath => "same_path"
      | .identicalBytes => "identical_bytes"))]

private def historyJson (rows : List (Input × Option Duplicate)) : Lean.Json :=
  let reports := rows.map fun (input, _) =>
    if contents initialFiles input.path == some 2 then ProgressDecision.Report.unusable
    else .usable [⟨0, 0, .killed⟩]
  let observation := ProgressDecision.compareHistory reports 3
  Lean.Json.mkObj [
    ("inputs", Lean.toJson reports.length),
    ("comparisons", Lean.toJson (observation.comparisons.map fun comparison =>
      match comparison.state with
      | .stalled => "stalled" | .indeterminate => "indeterminate"
      | .improving => "improving" | .regressing => "regressing")),
    ("consecutive_stalls", Lean.toJson observation.consecutiveStalls),
    ("latest", .str (match observation.latest with
      | .saturated => "saturated" | .stalled => "stalled" | .indeterminate => "indeterminate"
      | .improving => "improving" | .regressing => "regressing"))]

def corpus : String :=
  String.join (cases.map fun item =>
    let annotated := annotateWith classify initialFiles [] item.actions
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("id", .str item.id),
      ("mode", .str (if item.strict then "strict" else "internal-fixture")),
      ("initial", Lean.toJson initialFiles),
      ("actions", .arr (item.actions.toArray.map actionJson)),
      ("expected", .arr (annotated.toArray.map (duplicateJson ∘ Prod.snd))),
      ("history", if item.strict then historyJson annotated else .null)
    ]).compress ++ "\n")

private def lastMatch (test : Input → Bool) (history : List Input) : Option Nat :=
  (history.reverse.findIdx? test).map fun index => history.length - 1 - index

def sensitivity : IO UInt32 := do
  let variants : List (String × (Files → List Input → Input → Option Duplicate)) :=
    [ ("same_path_loses_priority", fun files history current =>
        (history.findIdx? (confirmed files current)).map (⟨·, .identicalBytes⟩))
    , ("same_path_uses_latest", fun files history current =>
        match lastMatch (samePath current) history with
        | some index => some ⟨index, .samePath⟩
        | none => classify files history current)
    , ("exact_match_uses_latest", fun files history current =>
        if (history.findIdx? (samePath current)).isSome then classify files history current
        else (lastMatch (confirmed files current) history).map (⟨·, .identicalBytes⟩))
    , ("fingerprint_alone_confirms", classifyWith fun _ current previous =>
        previous.fingerprint == current.fingerprint)
    , ("cross_fingerprint_match", classifyWith fun files current previous =>
        sameBytes files previous.path current.path)
    , ("missing_files_compare_equal", classifyWith fun files current previous =>
        previous.fingerprint == current.fingerprint &&
          contents files previous.path == contents files current.path)
    , ("first_unconfirmed_candidate_stops_scan", fun files history current =>
        if (history.findIdx? (samePath current)).isSome then classify files history current
        else do
          let index ← history.findIdx? (fun previous => previous.fingerprint == current.fingerprint)
          let previous ← history[index]?
          if confirmed files current previous then some ⟨index, .identicalBytes⟩ else none)
    ]
  for (name, variant) in variants do
    let observations := fun step item =>
      (annotateWith step initialFiles [] item.actions).map Prod.snd
    if let some item := cases.find? (fun item => observations variant item != observations classify item) then
      IO.println s!"progress duplicate sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"progress duplicate missed variant: {name}"
      return 1
  IO.println s!"progress_duplicate_strict={strictCases.length} internal={internalCases.length}"
  return 0

end HoiminOracle.ProgressDuplicate
