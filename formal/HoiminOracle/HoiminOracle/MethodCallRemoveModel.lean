import Std

namespace HoiminOracle.MethodCallRemove

/- Correspondence worksheet (before finite fixture selection):
   receiver/argument/context syntax -> Expr/Arguments/Context -> public CLI
   plan source input; original/replacement -> expected -> saved candidates: strict.
   Action result/state/exception -> removed/original -> no execution in the
   plan adapter: model-only. Parsing and runtime binding are not proved.
   Attribute lookup (including descriptors) and call effects are intentionally
   removed. Attribute callees need not be bound methods: module attributes and
   constructors qualify too. This is not a behavior-preserving rewrite.
   Parentheses are a rendering obligation; Python compilation is adapter evidence.
   No concurrency, transactions, fairness or exhaustive Python grammar claims. -/

abbrev Action (S ε α : Type) := S → Except ε α × S

def bind (first : Action S ε α) (next : α → Action S ε β) : Action S ε β :=
  fun state =>
    match first state with
    | (.error error, after) => (.error error, after)
    | (.ok value, after) => next value after

def original (receiver : Action S ε α) (lookup : α → Action S ε β)
    (call : β → Action S ε γ) : Action S ε γ :=
  bind (bind receiver lookup) call

def removed (receiver : Action S ε α) : Action S ε α := receiver

theorem receiver_once (receiver : Action S ε α) (state : S) :
    removed receiver state = receiver state := rfl

theorem receiver_exception (receiver : Action S ε α) (state after : S) (error : ε)
    (fails : receiver state = (.error error, after)) :
    removed receiver state = (.error error, after) := fails

set_option maxHeartbeats 10000 in
theorem original_receiver_exception (receiver : Action S ε α)
    (lookup : α → Action S ε β) (call : β → Action S ε γ)
    (state after : S) (error : ε)
    (fails : receiver state = (.error error, after)) :
    original receiver lookup call state = (.error error, after) := by
  simp [original, bind, fails]

-- A syntax summary, not a Python parser. Unsafe nodes represent NamedExpr,
-- Await, Yield, YieldFrom or GeneratorExp; wrappers permit nested witnesses.
inductive Expr where
  | leaf (text : String)
  | wrap (before : String) (child : Expr) (after : String)
  | blocked (before : String) (child : Expr) (after : String)
  deriving Repr, DecidableEq

def Expr.render : Expr → String
  | .leaf text => text
  | .wrap before child after | .blocked before child after => before ++ child.render ++ after

def Expr.forbidden : Expr → Bool
  | .leaf _ => false
  | .wrap _ child _ => child.forbidden
  | .blocked _ _ _ => true

theorem nested_forbidden (before after : String) (child : Expr)
    (unsafeChild : child.forbidden = true) :
    (Expr.wrap before child after).forbidden = true := unsafeChild

inductive Arguments | none | positional | keyword | star | doubleStar
  deriving Repr, DecidableEq, BEq

def Arguments.render : Arguments → String
  | .none => ""
  | .positional => "item"
  | .keyword => "flag=True"
  | .star => "*items"
  | .doubleStar => "**options"

inductive Context | value | asyncValue | generatorValue | annotation | typeAlias | pattern | target
  deriving Repr, DecidableEq, BEq

def Context.isValue : Context → Bool
  | .value | .asyncValue | .generatorValue => true
  | _ => false

def Context.render (context : Context) (expression : String) : String :=
  match context with
  | .value => "result = " ++ expression ++ "\n"
  | .asyncValue => "async def example():\n    return " ++ expression ++ "\n"
  | .generatorValue => "def example():\n    return " ++ expression ++ "\n"
  | .annotation => "value: " ++ expression ++ " = None\n"
  | .typeAlias => "from typing import TypeAlias\nAlias: TypeAlias = " ++ expression ++ "\n"
  | .pattern => "match subject:\n    case " ++ expression ++ ":\n        pass\n"
  | .target => expression ++ ".field = item\n"

structure Input where
  id : String
  receiver : Expr := .leaf "obj"
  grouped : Bool := false
  attributeCallee : Bool := true
  method : String := "method"
  args : Arguments := .none
  context : Context := .value
  deriving Repr

def eligible (input : Input) : Bool :=
  input.attributeCallee && input.args == .none && input.context.isValue && !input.receiver.forbidden

def replacement (input : Input) : String := "(" ++ input.receiver.render ++ ")"

def callSource (input : Input) : String :=
  let receiver := if input.grouped then "(" ++ input.receiver.render ++ ")" else input.receiver.render
  receiver ++ (if input.attributeCallee then "." ++ input.method else "") ++ "(" ++ input.args.render ++ ")"

def expected (input : Input) : List (String × String) :=
  if eligible input then [(callSource input, replacement input)] else []

set_option maxHeartbeats 10000 in
theorem unsafe_rejected (input : Input) (unsafeReceiver : input.receiver.forbidden = true) :
    expected input = [] := by simp [expected, eligible, unsafeReceiver]

set_option maxHeartbeats 10000 in
theorem nonvalue_rejected (input : Input) (outside : input.context.isValue = false) :
    expected input = [] := by simp [expected, eligible, outside]

def fixtures : List Input := [
  { id := "name" },
  { id := "string", receiver := .leaf "'text'", method := "strip" },
  { id := "factory", receiver := .leaf "factory()" },
  { id := "binary", receiver := .leaf "left + right", grouped := true },
  { id := "conditional", receiver := .leaf "left if flag else right", grouped := true },
  { id := "module_attribute", receiver := .leaf "module", method := "Constructor" },
  { id := "positional", args := .positional },
  { id := "keyword", args := .keyword },
  { id := "star", args := .star },
  { id := "double_star", args := .doubleStar },
  { id := "bare_call", attributeCallee := false },
  { id := "walrus", receiver := .blocked "captured := " (.leaf "obj") "", grouped := true },
  { id := "nested_walrus", receiver := .wrap "factory(" (.blocked "captured := " (.leaf "obj") "") ")" },
  { id := "await", receiver := .blocked "await " (.leaf "factory()") "", grouped := true, context := .asyncValue },
  { id := "nested_await", receiver := .wrap "factory(" (.blocked "await " (.leaf "pending") "") ")", context := .asyncValue },
  { id := "yield", receiver := .blocked "yield " (.leaf "item") "", grouped := true, context := .generatorValue },
  { id := "yield_from", receiver := .blocked "yield from " (.leaf "items") "", grouped := true, context := .generatorValue },
  { id := "generator", receiver := .blocked "item for item in " (.leaf "items") "", grouped := true },
  { id := "nested_generator", receiver := .wrap "tuple(" (.blocked "item for item in " (.leaf "items") "") ")" },
  { id := "annotation", context := .annotation },
  { id := "type_alias", context := .typeAlias },
  { id := "pattern", context := .pattern },
  { id := "target", context := .target }]

inductive Event | receiver | lookup | call
  deriving Repr, DecidableEq, BEq

def receiverAction : Action (List Event) String Nat := fun events => (.ok 7, events ++ [.receiver])
def lookupAction (_ : Nat) : Action (List Event) String Nat := fun events => (.ok 8, events ++ [.lookup])
def callAction (_ : Nat) : Action (List Event) String Nat := fun events => (.ok 9, events ++ [.call])
def brokenDuplicate : Action (List Event) String Nat := bind receiverAction (fun _ => receiverAction)

def observe (result : Except String Nat × List Event) : Option String × Option Nat × List Event :=
  match result with
  | (.error error, events) => (some error, none, events)
  | (.ok value, events) => (none, some value, events)

def brokenEligibility (family : String) (input : Input) : Bool :=
  (family == "bare_callee" || input.attributeCallee) &&
  (family == "arguments" || input.args == .none) &&
  (family == "context" || input.context.isValue) &&
  (family == "unsafe_receiver" || !input.receiver.forbidden)

def sensitivityChecks : List (String × Bool) := [
  ("duplicate_receiver", observe (removed receiverAction []) != observe (brokenDuplicate [])),
  ("retained_lookup", observe (removed receiverAction []) != observe (bind receiverAction lookupAction [])),
  ("retained_call", observe (removed receiverAction []) != observe (original receiverAction lookupAction callAction [])),
  ("swallowed_exception", observe (removed (fun _ : List Event => ((Except.error "receiver failed" : Except String Nat), [Event.receiver])) []) != observe (.ok 7, [Event.receiver])),
  ("omit_parentheses", fixtures.any fun input => eligible input && replacement input != input.receiver.render)
  ] ++ ["bare_callee", "arguments", "context", "unsafe_receiver"].map (fun family =>
    (family, fixtures.any fun input => eligible input != brokenEligibility family input))

set_option maxHeartbeats 20000 in
example : sensitivityChecks.all (·.2) = true := by decide

set_option maxHeartbeats 10000 in
example : removed receiverAction [] = (.ok 7, [.receiver]) := rfl

set_option maxHeartbeats 10000 in
example : original receiverAction lookupAction callAction [] = (.ok 9, [.receiver, .lookup, .call]) := rfl

end HoiminOracle.MethodCallRemove
