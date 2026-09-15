import DeclarationModel
import Lean.Data.Json
open DeclarationAudit Lean

structure Fixture where
  id : String
  source : String
  scope : Scope := .module
  value : Value := .missing
  fallback : Bool := true
  hasValue : Bool := false
  operator : String := "collection_any_all"
  original : String := "any"
  replacement : String := "all"
  deriving Repr

def fixtures : List Fixture := [
  { id := "module_source", source := "any: int\nobserved=any([])\n" },
  { id := "module_destination", source := "all: int\nobserved=any([])\n" },
  { id := "class_source", source := "class C:\n any:int\n observed=any([])\nobserved=C.observed\n", scope := .classBody },
  { id := "class_destination", source := "class C:\n all:int\n observed=any([])\nobserved=C.observed\n", scope := .classBody },
  { id := "outer_declaration", source := "any:int\ndef f():\n return any([])\nobserved=f()\n" },
  { id := "module_alias", source := "import operator as op\nop: object\nobserved=op.add(2,1)\n", value := .known, operator := "operator_function", original := "add", replacement := "sub" },
  { id := "direct_alias", source := "from operator import add as fn\nfn: object\nobserved=fn(2,1)\n", value := .known, operator := "operator_function", original := "fn", replacement := "__import__('operator').sub" },
  { id := "prior_custom", source := "def any(values): return 'custom'\nany:int\nobserved=any([])\n", value := .other },
  { id := "class_outer_custom", source := "def any(values): return 'custom'\nclass C:\n any:int\n observed=any([])\nobserved=C.observed\n", scope := .classBody, fallback := false },
  { id := "rhs_rebinding", source := "any:object=lambda values: 'custom'\nobserved=any([])\n", hasValue := true },
  { id := "function_local", source := "def f():\n any:int\n return any([])\ntry: observed=f()\nexcept UnboundLocalError: observed='unbound'\n", scope := .function },
  { id := "normal_builtin", source := "observed=any([])\n" },
  { id := "normal_alias", source := "import operator as op\nobserved=op.add(2,1)\n", value := .known, operator := "operator_function", original := "add", replacement := "sub" },
  { id := "normal_direct_alias", source := "from operator import add as fn\nobserved=fn(2,1)\n", value := .known, operator := "operator_function", original := "fn", replacement := "__import__('operator').sub" }]

def corpus : String := String.join (fixtures.map fun f =>
  let allowed := resolvesKnown f.scope f.fallback (annotate f.hasValue f.value)
  (Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson f.id), ("mode", toJson "strict"),
    ("source", toJson f.source), ("operator", toJson f.operator),
    ("pairs", toJson (if allowed then [(f.original, f.replacement)] else []))]).compress ++ "\n")

def main (args : List String) : IO Unit := do
  let scopes : List Scope := [.module, .classBody, .function]
  let values : List Value := [.missing, .known, .other]
  for count in [1, 2, 3] do
    let started ← IO.monoMsNow
    let mut cases := 0
    let mut badStore := 0
    let mut badIgnore := 0
    let mut badFallback := 0
    for scope in scopes.take count do
      for value in values do
        for fallback in [false, true] do
          for hasValue in [false, true] do
            cases := cases + 1
            let expected := resolvesKnown scope fallback (annotate hasValue value)
            if expected != resolvesKnown scope fallback (brokenAlwaysStore hasValue value) then badStore := badStore + 1
            if expected != resolvesKnown scope fallback (brokenIgnoreRhs hasValue value) then badIgnore := badIgnore + 1
            if expected != brokenFallbackInFunction scope fallback (annotate hasValue value) then badFallback := badFallback + 1
    IO.println s!"scopes={count} states={count * 3 * 2} actions=2 cases={cases} transitions={cases} always_store_mismatches={badStore} ignore_rhs_mismatches={badIgnore} fallback_mismatches={badFallback} elapsed_ms={(← IO.monoMsNow) - started}"
  unless resolvesKnown .module true (annotate false .missing) != resolvesKnown .module true (brokenAlwaysStore false .missing) do
    throw (IO.userError "no-value boundary undetected")
  unless resolvesKnown .module true (annotate true .missing) != resolvesKnown .module true (brokenIgnoreRhs true .missing) do
    throw (IO.userError "RHS boundary undetected")
  unless resolvesKnown .function true .missing != brokenFallbackInFunction .function true .missing do
    throw (IO.userError "function-local precedence undetected")
  IO.println "sensitivity: always-store, ignore-RHS, function-fallback detected"
  IO.println "first witness: module / missing / fallback=true / hasValue=false; value stays missing; expected=true; always-store=false"
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | _ => throw (IO.userError "usage: --output PATH | --check PATH")
