import HoiminOracle.OperatorSelectorModel
import Lean.Data.Json

namespace HoiminOracle.OperatorSelector

private def backtickNames (value : String) : List String :=
  (value.splitOn "`").zipIdx.filterMap fun (item, index) =>
    if index % 2 == 1 then some item else none

private def parseRow (line : String) : Except String Operator :=
  match line.splitOn "|" with
  | [_, id, enabled, family, _] =>
    match backtickNames id, enabled.trimAscii.toString with
    | [name], "yes" => .ok ⟨name, true, backtickNames family⟩
    | [name], "opt-in" => .ok ⟨name, false, backtickNames family⟩
    | _, _ => .error s!"invalid operator index row: {line}"
  | _ => .error s!"invalid operator index row: {line}"

private def parseCatalog (document : String) : Except String (List Operator) := do
  let header := "| Operator ID | Default runtime selection | Selector families |"
  let table := (document.splitOn "\n").map (·.trimAsciiEnd.toString)
  let rows := (table.dropWhile (· != header)).drop 2 |>.takeWhile (·.startsWith "| ")
  if rows.isEmpty then throw "operator index table is missing"
  let catalog ← rows.mapM parseRow
  let names := catalog.map Operator.name
  if names != names.eraseDups || names != names.mergeSort (· ≤ ·) then
    throw "operator index IDs must be unique and in ascending order"
  pure catalog

private def loadCatalog : IO (List Operator) := do
  let document ← IO.FS.readFile "../../docs/user/reference/cli.md"
  match parseCatalog document with
  | .ok catalog => pure catalog
  | .error message => throw (IO.userError message)

private structure Fixture where
  id : String
  args : List Argument

private def familyNames (catalog : List Operator) : List String :=
  ((catalog.flatMap Operator.families).eraseDups).mergeSort (· ≤ ·)

private def families (catalog : List Operator) : List Fixture :=
  (familyNames catalog).flatMap fun family =>
    match (catalog.find? (fun operator => operator.families.contains family)).map Operator.name with
    | none => []
    | some first =>
      [⟨s!"family-{family}", [⟨.including, family⟩]⟩,
       ⟨s!"default-minus-{family}", [⟨.excluding, family⟩]⟩,
       ⟨s!"family-minus-member-{family}", [⟨.including, family⟩, ⟨.excluding, first⟩]⟩,
       ⟨s!"member-minus-family-{family}", [⟨.excluding, family⟩, ⟨.including, first⟩]⟩,
       ⟨s!"family-and-duplicate-{family}", [⟨.including, s!"{family},{first}"⟩,
          ⟨.including, family⟩]⟩]

private def familyPairs (catalog : List Operator) : List Fixture :=
  let names := familyNames catalog
  names.zipIdx.flatMap fun (first, index) =>
    (names.drop (index + 1)).map fun second =>
      ⟨s!"families-{first}-{second}", [⟨.excluding, first⟩,
        ⟨.including, s!"{second},{first}"⟩, ⟨.including, second⟩]⟩

private def fixtures (catalog : List Operator) : List Fixture :=
  [⟨"defaults", []⟩] ++
  (catalog.map fun operator =>
    ⟨s!"operator-{operator.name}", [⟨.including, operator.name⟩]⟩) ++
  families catalog ++ familyPairs catalog ++ [
    ⟨"first-unknown-include", [⟨.including, "compare_eq_ne,bad_first,bad_second"⟩]⟩,
    ⟨"first-unknown-exclude", [⟨.excluding, "compare_eq_ne,bad_first"⟩,
      ⟨.excluding, "bad_second"⟩]⟩,
    ⟨"unknown-include-before-exclude", [⟨.excluding, "bad_exclude"⟩,
      ⟨.including, "bad_include"⟩]⟩,
    ⟨"unknown-exclude-before-empty-selection", [⟨.including, "compare_eq_ne"⟩,
      ⟨.excluding, "compare_eq_ne,bad_exclude"⟩]⟩,
    ⟨"empty-field-leading", [⟨.including, ",compare_eq_ne"⟩]⟩,
    ⟨"empty-field-interior", [⟨.including, "compare_eq_ne,,binary_add_sub"⟩]⟩,
    ⟨"empty-field-trailing", [⟨.excluding, "compare_eq_ne,"⟩]⟩,
    ⟨"whitespace-is-literal", [⟨.including, "compare_eq_ne, binary_add_sub"⟩]⟩,
    ⟨"case-is-literal", [⟨.including, "Compare_Eq_Ne"⟩]⟩,
    ⟨"retired-alias", [⟨.including, "type_mapping"⟩]⟩]

private def flag : Direction → String
  | .including => "--operators" | .excluding => "--exclude-operators"

private def outcomeJson : Except Issue (List String) → Lean.Json
  | .ok members => Lean.Json.mkObj [("kind", .str "accepted"),
      ("operators", .arr (members.toArray.map Lean.Json.str))]
  | .error (.unknown selector) => Lean.Json.mkObj [
      ("kind", .str "unknown"), ("selector", .str selector)]
  | .error .empty => Lean.Json.mkObj [("kind", .str "empty")]

private def caseJson (catalog : List Operator) (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("args", .arr (item.args.toArray.map fun arg => .str s!"{flag arg.direction}={arg.value}")),
  ("expected", outcomeJson (resolve catalog item.args))]

def corpus : IO String := do
  let catalog ← loadCatalog
  pure (String.join ((fixtures catalog).map fun item => (caseJson catalog item).compress ++ "\n"))

private def unionWithDefaults (catalog : List Operator) (args : List Argument) :
    Except Issue (List String) :=
  if (selectorNames .including args).isEmpty then resolve catalog args
  else
    let defaults := (catalog.filter Operator.enabled).map
      (fun operator => Argument.mk .including operator.name)
    resolve catalog (defaults ++ args)

private def validateExcludesFirst (catalog : List Operator) (args : List Argument) :
    Except Issue (List String) := do
  let _ ← expandAll catalog (selectorNames .excluding args)
  resolve catalog args

private def ignoreExcludes (catalog : List Operator) (args : List Argument) :
    Except Issue (List String) :=
  resolve catalog (args.filter (·.direction == .including))

private def trimTokens (catalog : List Operator) (args : List Argument) :
    Except Issue (List String) :=
  resolve catalog (args.map fun arg => { arg with
    value := String.intercalate "," ((arg.value.splitOn ",").map (·.trimAscii.toString)) })

private def differs (first second : Except Issue (List String)) : Bool :=
  match first, second with
  | .ok a, .ok b => a != b
  | .error a, .error b => a != b
  | _, _ => true

def sensitivity : IO UInt32 := do
  let catalog ← loadCatalog
  for (name, candidate) in [("union_explicit_with_defaults", unionWithDefaults),
      ("validate_excludes_first", validateExcludesFirst), ("ignore_exclusions", ignoreExcludes),
      ("trim_selector_tokens", trimTokens)] do
    match (fixtures catalog).find? (fun item => differs (candidate catalog item.args) (resolve catalog item.args)) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"operator selector corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.OperatorSelector
