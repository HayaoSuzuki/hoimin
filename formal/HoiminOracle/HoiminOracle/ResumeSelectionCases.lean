import HoiminOracle.ResumeSelectionModel
import Lean.Data.Json

namespace HoiminOracle.ResumeSelection

private structure Case where
  id : String
  rows : List Row

private def limit : Nat := 256
private def requested : Nat := 1

private def schemaName : FingerprintSchema → String
  | .current => "current"
  | .legacy => "legacy"
  | .future => "future"
  | .wrongType => "wrong_type"
  | .missing => "missing"

private def budgetName : Budget → String
  | .valid n => s!"valid-{n}"
  | .zero => "zero"
  | .short => "short"
  | .wrongType => "wrong_type"

private def candidates : List Case :=
  ([.valid 255, .valid 256, .valid 257, .zero, .short, .wrongType] : List Budget).flatMap fun budget =>
    [false, true].map fun active =>
      ⟨s!"newest-{budgetName budget}-active-{active}",
        [{ name := "z_first", fingerprint := 1, budget := .valid 255 },
         { name := "a_second", fingerprint := 1, budget, active }]⟩

private def joined : List Case :=
  ([.current, .legacy, .future, .wrongType, .missing] : List FingerprintSchema).flatMap fun first =>
    ([.current, .legacy, .future, .wrongType, .missing] : List FingerprintSchema).map fun second =>
      ⟨s!"fallback-{schemaName first}-then-{schemaName second}",
        [{ name := "z_first", fingerprint := 2, budget := .valid 255, schema := first },
         { name := "a_second", fingerprint := 3, budget := .valid 255, schema := second }]⟩

private def boundaries : List Case :=
  [ ⟨"matching-candidate-precedes-newer-legacy", [
      { name := "z_first", fingerprint := 1, budget := .valid 255 },
      { name := "a_second", fingerprint := 2, budget := .valid 255, schema := .legacy }]⟩
  , ⟨"matching-candidate-with-legacy-schema", [
      { name := "legacy", fingerprint := 1, budget := .valid 255, schema := .legacy }]⟩
  , ⟨"matching-candidate-with-missing-schema", [
      { name := "orphan", fingerprint := 1, budget := .valid 255, schema := .missing }]⟩
  , ⟨"matching-candidate-with-wrong-typed-schema", [
      { name := "matching", fingerprint := 1, budget := .valid 255, schema := .wrongType }]⟩
  , ⟨"matching-candidate-with-future-schema", [
      { name := "matching", fingerprint := 1, budget := .valid 255, schema := .future }]⟩
  , ⟨"legacy-history-precedes-budget-decrease", [
      { name := "legacy", fingerprint := 1, budget := .valid 257, schema := .legacy }]⟩
  , ⟨"complete-legacy-is-not-incomplete-history", [
      { name := "other", fingerprint := 2, budget := .valid 255 },
      { name := "complete", fingerprint := 1, budget := .valid 255, complete := true, schema := .legacy }]⟩
  , ⟨"corrupt-budget-on-complete-row-is-not-selected", [
      { name := "z_first", fingerprint := 1, budget := .valid 255 },
      { name := "a_second", fingerprint := 1, budget := .wrongType, complete := true }]⟩
  , ⟨"wrong-typed-newest-schema-precedes-fresh-reason", [
      { name := "matching", fingerprint := 1, budget := .valid 257 },
      { name := "other", fingerprint := 2, budget := .valid 255, schema := .wrongType }]⟩
  , ⟨"ignore-corrupt-budget-for-other-fingerprint", [
      { name := "matching", fingerprint := 1, budget := .valid 255 },
      { name := "other", fingerprint := 2, budget := .wrongType }]⟩
  , ⟨"only-complete-legacy", [
      { name := "complete", fingerprint := 2, budget := .valid 255, complete := true, schema := .legacy }]⟩
  , ⟨"no-history", []⟩ ]

private def cases := candidates ++ joined ++ boundaries

private def resultName : Result → String
  | .resumed name => "resumed:" ++ name
  | .fresh reason => reason
  | .incompatible => "session.resume.incompatible"
  | .schemaType => "session.read"
  | .budgetType => "invalid persisted mutant budget type"
  | .budgetLength => "invalid persisted mutant budget length"
  | .budgetValue => "invalid persisted mutant budget"
  | .active => "session.resume.active"

private def budgetJson : Budget → Lean.Json
  | .valid value => Lean.Json.mkObj [("kind", .str "valid"), ("value", Lean.toJson value)]
  | other => Lean.Json.mkObj [("kind", .str (budgetName other))]

def corpus : String :=
  String.join (cases.map fun item =>
    let outcome := choose requested limit item.rows
    let rows := item.rows.map fun row => Lean.Json.mkObj
      [ ("name", .str row.name), ("fingerprint", Lean.toJson row.fingerprint),
        ("complete", .bool row.complete), ("active", .bool row.active), ("schema", .str (schemaName row.schema)),
        ("budget", budgetJson row.budget), ("after", budgetJson (budgetAfter limit outcome row)) ]
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("requested", Lean.toJson requested), ("limit", Lean.toJson limit), ("rows", Lean.toJson rows)
      , ("outcome", .str (resultName outcome)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (List Row → Result)) :=
    [ ("select_oldest", fun rows => choose requested limit rows.reverse)
    , ("skip_corrupt_budgets", fun rows =>
        choose requested limit (rows.filter fun row => match row.budget with | .valid _ => true | _ => false))
    , ("fall_back_from_active", fun rows => choose requested limit (rows.filter fun row => !row.active))
    , ("check_history_before_selected_candidate", fun rows =>
        match fallback requested limit rows with
        | .incompatible => .incompatible
        | .schemaType => .schemaType
        | _ => choose requested limit rows)
    , ("ignore_legacy_history", fun rows => match choose requested limit rows with
        | .incompatible => .fresh (historyReason requested limit rows)
        | result => result)
    , ("let_orphan_hide_joined_history", fun rows =>
        let newest := (rows.reverse.find? (fun row => !row.complete)).map Row.schema
        if (pick requested limit rows).isNone && newest == some .missing then
          .fresh (historyReason requested limit rows)
        else choose requested limit rows) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => variant item.rows != choose requested limit item.rows) then
      IO.println s!"resume selection sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"resume selection missed variant: {name}"
      return 1
  IO.println s!"resume_selection_cases={cases.length}"
  return 0

end HoiminOracle.ResumeSelection
