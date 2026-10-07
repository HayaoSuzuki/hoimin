import Std

namespace HoiminOracle.FunctionReturnConstant

/- Correspondence worksheet, established before fixture selection:
   direct annotation + deferred binding -> Annotation/Binding -> rendered source
   configured through public plan CLI: strict for the listed fixtures.
   own-scope return/suspension -> Stmt -> rendered source: strict fixtures;
   nested bodies are summarized separately from eagerly evaluated headers.
   exact tagged literal/no-op -> Returned/Literal -> saved original/replacement:
   strict. Parsing, name resolution and cooked-value extraction are not proved.
   body effects/results -> Action/replaced -> no execution in plan adapter:
   model-only. An annotation is not a Python type guarantee.
   preserved header/docstring/suffix -> Span -> model-only string contract;
   public candidate spans and CPython compilation supply separate evidence.
   Python 3.14 type parameters are modeled as non-builtin bindings, but omitted
   from this portable corpus. No coverage or pseudo-tested classification.
   No concurrent protocol, transaction or fairness claim. -/

abbrev Action (S α : Type) := S → α × S

def replaced (constant : α) : Action S α := fun state => (constant, state)

theorem returns_constant (constant : α) (state : S) :
    (replaced constant state).1 = constant := rfl

theorem erases_body_effects (constant : α) (state : S) :
    (replaced constant state).2 = state := rfl

def brokenRetainsBody (constant : α) (body : Action S β) : Action S α :=
  fun state => (constant, (body state).2)

structure Span where
  beforeText : String
  body : String
  suffix : String

def Span.replace (span : Span) (text : String) : String := span.beforeText ++ text ++ span.suffix

theorem span_keeps_surroundings (beforeText body suffix text : String) :
    (Span.mk beforeText body suffix).replace text = beforeText ++ text ++ suffix := rfl

inductive Kind | bool | int | str
  deriving Repr, DecidableEq, BEq

def Kind.name : Kind → String
  | .bool => "bool"
  | .int => "int"
  | .str => "str"

inductive Literal | boolean (value : Bool) | integer (value : Nat) | string (value : String) | none
  deriving Repr, DecidableEq, BEq

def constants : Kind → List Literal
  | .bool => [.boolean false, .boolean true]
  | .int => [.integer 0, .integer 1]
  | .str => [.string "", .string "A"]

def Literal.render : Literal → String
  | .boolean false => "False"
  | .boolean true => "True"
  | .integer value => toString value
  | .string value => "\"" ++ value ++ "\""
  | .none => "None"

-- Literal.render is used only for the fixed safe alphabet above; arbitrary
-- quoted strings are not serialized by this model.
set_option maxHeartbeats 10000 in
theorem two_distinct_constants (kind : Kind) :
    (constants kind).length = 2 ∧ (constants kind).Nodup := by
  cases kind <;> decide

inductive Annotation | direct (kind : Kind) | unsupported (text : String)
  deriving Repr, DecidableEq, BEq

def Annotation.render : Annotation → String
  | .direct kind => " -> " ++ kind.name
  | .unsupported "" => ""
  | .unsupported text => " -> " ++ text

def Annotation.kind? : Annotation → Option Kind
  | .direct kind => some kind
  | .unsupported _ => none

inductive Binding | builtin | shadowedBefore | shadowedAfter | ambiguous | typeParameter
  deriving Repr, DecidableEq, BEq

inductive Returned
  | expression (text : String)
  | literal (value : Literal) (spelling : String)
  | bare
  deriving Repr, DecidableEq

def Returned.render : Returned → String
  | .expression text | .literal _ text => "return " ++ text
  | .bare => "return"

def Returned.nonNone : Returned → Bool
  | .literal .none _ | .bare => false
  | _ => true

inductive Stmt
  | effect (text : String)
  | ret (value : Returned)
  | suspend (text : String)
  | nested (text : String) (eagerSuspends : Bool)
  deriving Repr, DecidableEq

def Stmt.render : Stmt → String
  | .effect text | .suspend text | .nested text _ => text
  | .ret value => value.render

def Stmt.ownReturn : Stmt → Bool
  | .ret value => value.nonNone
  | _ => false

def Stmt.ownSuspends : Stmt → Bool
  | .suspend _ => true
  | .nested _ eager => eager
  | _ => false

theorem nested_return_ignored (text : String) (eager : Bool) :
    (Stmt.nested text eager).ownReturn = false := rfl

theorem eager_header_is_observed (text : String) :
    (Stmt.nested text true).ownSuspends = true := rfl

structure Input where
  id : String
  annotation : Annotation := .direct .int
  binding : Binding := .builtin
  asynchronous : Bool := false
  dunder : Bool := false
  docstring : Bool := false
  beforeText : String := ""
  body : List Stmt := [.ret (.expression "compute()")]
  deriving Repr

def eligible (input : Input) : Bool :=
  input.annotation.kind?.isSome && input.binding == .builtin &&
    !input.asynchronous && !input.dunder && input.body.any Stmt.ownReturn &&
    !(input.body.any Stmt.ownSuspends)

def isNoop (body : List Stmt) (constant : Literal) : Bool :=
  match body with
  | [.ret (.literal value _)] => value == constant
  | _ => false

def choices (input : Input) : List Literal :=
  if eligible input then
    match input.annotation.kind? with
    | some kind => (constants kind).filter fun constant => !isNoop input.body constant
    | none => []
  else []

def originalBody (input : Input) : String :=
  String.intercalate "\n    " (input.body.map Stmt.render)

def source (input : Input) : String :=
  let bindingName := (input.annotation.kind?.getD .int).name
  let before := match input.binding with
    | .shadowedBefore => bindingName ++ " = Custom\n"
    | .ambiguous => "if flag:\n    " ++ bindingName ++ " = Custom\n"
    | _ => ""
  let after := if input.binding == .shadowedAfter then bindingName ++ " = Custom\n" else ""
  input.beforeText ++ before ++ (if input.asynchronous then "async " else "") ++
    "def " ++ (if input.dunder then "__example__" else "example") ++ "()" ++
    input.annotation.render ++ ":\n" ++
    (if input.docstring then "    \"\"\"preserved documentation\"\"\"\n" else "") ++
    "    " ++ originalBody input ++ "\n" ++ after

def expected (input : Input) : List (String × String) :=
  (choices input).map fun constant => (originalBody input, "return " ++ constant.render)

set_option maxHeartbeats 10000 in
theorem ineligible_has_no_choices (input : Input) (rejected : eligible input = false) :
    choices input = [] := by simp [choices, rejected]

set_option maxHeartbeats 10000 in
theorem no_noop_choice (input : Input) (constant : Literal) (selected : constant ∈ choices input) :
    isNoop input.body constant = false := by
  unfold choices at selected
  split at selected
  · split at selected
    · simpa using (List.mem_filter.mp selected).2
    · simp at selected
  · simp at selected

def fixtures : List Input := [
  { id := "bool_pair", annotation := .direct .bool },
  { id := "int_pair" },
  { id := "str_pair", annotation := .direct .str },
  { id := "docstring_effects", docstring := true,
    body := [.effect "record()", .ret (.expression "compute()")] },
  { id := "bool_false", annotation := .direct .bool, body := [.ret (.literal (.boolean false) "False")] },
  { id := "bool_true", annotation := .direct .bool, body := [.ret (.literal (.boolean true) "True")] },
  { id := "int_zero", body := [.ret (.literal (.integer 0) "0")] },
  { id := "int_one", body := [.ret (.literal (.integer 1) "1")] },
  { id := "str_empty", annotation := .direct .str, body := [.ret (.literal (.string "") "''")] },
  { id := "str_cooked_a", annotation := .direct .str, body := [.ret (.literal (.string "A") "'\\x41'")] },
  { id := "parenthesized_false", annotation := .direct .bool, body := [.ret (.literal (.boolean false) "(False)")] },
  { id := "bool_integer_distinct", annotation := .direct .bool, body := [.ret (.literal (.integer 0) "0")] },
  { id := "int_boolean_distinct", body := [.ret (.literal (.boolean true) "True")] },
  { id := "async", asynchronous := true },
  { id := "dunder", dunder := true },
  { id := "generator", body := [.suspend "yield item", .ret (.expression "compute()")] },
  { id := "return_none", body := [.ret (.literal .none "None")] },
  { id := "bare_return", body := [.ret .bare] },
  { id := "nested_return_only", body := [.nested "def inner():\n        return 1" false] },
  { id := "nested_generator_ignored", body := [.nested "def inner():\n        yield item" false, .ret (.expression "compute()")] },
  { id := "nested_lambda_ignored", body := [.nested "callback = lambda: (yield item)" false, .ret (.expression "compute()")] },
  { id := "nested_class_only", body := [.nested "class Inner:\n        def value(self):\n            return 1" false] },
  { id := "default_yield", body := [.nested "def inner(value=(yield item)):\n        pass" true, .ret (.expression "compute()")] },
  { id := "decorator_yield", body := [.nested "@(yield decorator)\n    def inner():\n        pass" true, .ret (.expression "compute()")] },
  { id := "annotation_missing", annotation := .unsupported "" },
  { id := "annotation_quoted", annotation := .unsupported "'int'" },
  { id := "annotation_alias", annotation := .unsupported "Number", beforeText := "Number = int\n" },
  { id := "annotation_qualified", annotation := .unsupported "builtins.int", beforeText := "import builtins\n" },
  { id := "annotation_composite", annotation := .unsupported "int | None" },
  { id := "shadowed_before", binding := .shadowedBefore },
  { id := "shadowed_after", binding := .shadowedAfter },
  { id := "ambiguous_binding", binding := .ambiguous }]

def brokenEligible (family : String) (input : Input) : Bool :=
  (family == "annotation" || input.annotation.kind?.isSome) &&
  (family == "binding" || input.binding == .builtin) &&
  (family == "async" || !input.asynchronous) &&
  (family == "dunder" || !input.dunder) &&
  (family == "own_return" || input.body.any Stmt.ownReturn) &&
  (family == "suspension" || !(input.body.any Stmt.ownSuspends))

def brokenBooleanIntegerEquality (left right : Literal) : Bool :=
  match left, right with
  | .boolean false, .integer 0 | .integer 0, .boolean false => true
  | .boolean true, .integer 1 | .integer 1, .boolean true => true
  | _, _ => left == right

def sensitivityChecks : List (String × Bool) := [
  ("retained_body_effect", replaced 0 (0 : Nat) != brokenRetainsBody 0 (fun state => (7, state + 1)) 0),
  ("wrong_constant", replaced 0 (0 : Nat) != replaced 1 0),
  ("retained_literal_noop", choices { id := "w", body := [.ret (.literal (.integer 0) "0")] } != constants .int),
  ("boolean_integer_conflation", ((Literal.boolean false) == .integer 0) != brokenBooleanIntegerEquality (.boolean false) (.integer 0)),
  ("nested_return_counted", (Stmt.nested "def inner(): return 1" false).ownReturn != true),
  ("nested_body_suspension_counted", (Stmt.nested "def inner(): yield item" false).ownSuspends != true),
  ("eager_header_ignored", (Stmt.nested "def inner(x=(yield item)): pass" true).ownSuspends != false),
  ("docstring_removed", (Span.mk "def f():\n    \"doc\"\n    " "return old" "\n").replace "return 0" != "def f():\n    return 0\n")
  ] ++ ["annotation", "binding", "async", "dunder", "own_return", "suspension"].map (fun family =>
    (family, fixtures.any fun input => eligible input != brokenEligible family input))

set_option maxHeartbeats 20000 in
example : sensitivityChecks.all (·.2) = true := by decide

set_option maxHeartbeats 10000 in
example : choices { id := "bool-int", annotation := .direct .bool, body := [.ret (.literal (.integer 0) "0")] } =
    [.boolean false, .boolean true] := by decide

end HoiminOracle.FunctionReturnConstant
