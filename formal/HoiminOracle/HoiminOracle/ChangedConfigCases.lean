import HoiminOracle.ChangedConfigModel
import Lean.Data.Json

namespace HoiminOracle.ChangedConfig

private structure Fixture where
  id : String
  request : Request

private def selectorName : Selector → String
  | .source => "source" | .file => "file" | .line => "line" | .symbol => "symbol"

private def selectorSets : List (List Selector) :=
  ([.source, .file, .line, .symbol] : List Selector).foldr
    (fun item rest => rest ++ rest.map (item :: ·)) [[]]

private def contexts : List (Option Nat) :=
  [none, some 0, some 1, some maxContext, some (maxContext + 1), some 4294967295]

private def fixtures : List Fixture :=
  (selectorSets.flatMap fun selectors =>
    [false, true].flatMap fun enabled =>
      contexts.flatMap fun context =>
        ([none, some "topic~2"] : List (Option String)).map fun base =>
          let key := String.intercalate "-" (selectors.map selectorName)
          let id := s!"{key}_changed-{enabled}_context-{(context.map toString).getD "default"}_base-{base.isSome}"
          ⟨id, ⟨selectors, enabled, context, base⟩⟩) ++ [
    ⟨"explicit-empty-base", ⟨[.source], true, some 0, some ""⟩⟩,
    ⟨"opaque-base", ⟨[.source, .file], true, none, some "topic with spaces"⟩⟩]

private def issueName : Issue → String
  | .invalidCliContext => "invalid_cli_context"
  | .cliContextRequiresChanged => "cli_context_requires_changed"
  | .missingSelector => "missing_selector"
  | .changedRequiresSource => "changed_requires_source"
  | .symbolRequiresSource => "symbol_requires_source"
  | .contextRequiresChanged => "context_requires_changed"
  | .diffBaseRequiresChanged => "diff_base_requires_changed"
  | .contextTooLarge _ => "context_too_large"

private def scopeJson : Option Scope → Lean.Json
  | none => .null
  | some scope => Lean.Json.mkObj [
      ("context", Lean.toJson scope.context), ("diff_base", Lean.toJson scope.base)]

private def outcomeJson : Except Issue (Option Scope) → Lean.Json
  | .ok scope => Lean.Json.mkObj [("kind", .str "accepted"), ("changed", scopeJson scope)]
  | .error (.contextTooLarge context) => Lean.Json.mkObj [
      ("kind", .str "context_too_large"), ("context", Lean.toJson context),
      ("maximum", Lean.toJson maxContext)]
  | .error issue => Lean.Json.mkObj [("kind", .str (issueName issue))]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let request := item.request
  let scope := savedScope request
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
    ("selectors", .arr (request.selectors.toArray.map fun value => .str (selectorName value))),
    ("enabled", Lean.toJson request.enabled), ("context", Lean.toJson request.context),
    ("diff_base", Lean.toJson request.base), ("saved_scope", scopeJson scope),
    ("cli", outcomeJson (fromCli request)), ("raw", outcomeJson (fromRaw request)),
    ("saved", outcomeJson (fromSaved request.selectors scope))]

def corpus : String :=
  String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def elideExplicitZero (request : Request) : Except Issue (Option Scope) :=
  fromCli (if request.context == some 0 then { request with context := none } else request)

private def validateSourceFirst (request : Request) : Except Issue (Option Scope) :=
  if request.enabled && decide (Selector.source ∉ request.selectors) then .error .changedRequiresSource
  else fromRaw request

private def ignoreDiffBase (request : Request) : Except Issue (Option Scope) :=
  fromRaw { request with base := none }

private def differs (first second : Except Issue (Option Scope)) : Bool :=
  match first, second with
  | .ok a, .ok b => a != b
  | .error a, .error b => a != b
  | _, _ => true

def sensitivity : IO UInt32 := do
  for (name, candidate, correct) in [
    ("elide_explicit_zero", elideExplicitZero, fromCli),
    ("validate_source_before_context_bound", validateSourceFirst, fromRaw),
    ("ignore_explicit_diff_base", ignoreDiffBase, fromRaw)] do
    match fixtures.find? (fun item => differs (candidate item.request) (correct item.request)) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"changed config corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.ChangedConfig
