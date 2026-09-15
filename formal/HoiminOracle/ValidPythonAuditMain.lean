import HoiminOracle.ValidPythonModel
import Lean.Data.Json
open HoiminOracle.ValidPython HoiminOracle.BindingFlow

structure Site where
  anchor : String
  original : String
  replacement : String
  eligible : Bool
  broken : Option Bool := none
  fault : String := ""

structure Case where
  id : String
  mode : String := "strict"
  producer : String
  position : String
  binding : String
  source : String
  operator : String
  sites : List Site
  variants : Bool := false
  harness : String := ""
  baseline : String := ""
  mutant : String := ""

def sliceTupleCase (id interior : String) (elements : List TupleElement) : Case :=
  { id := "slice_tuple_" ++ id, producer := "ast", position := "subscript", binding := "unshadowed",
    source := "def subject(x):\n    return x[" ++ interior ++ "]\n",
    operator := "collection_list_tuple",
    sites := [{ anchor := interior, original := interior, replacement := "[" ++ interior ++ "]", eligible := tupleAllowed elements, broken := if tupleAllowed elements then none else some (brokenTupleAllowed elements), fault := "allow-slice-tuple" }] }

def sliceTupleCases : List Case :=
  (tupleDomain.zipIdx.map fun (elements, index) =>
    let parts := elements.map fun element => if element == .slice then ":" else "1"
    let interior := String.intercalate ", " parts ++ if elements.length == 1 then "," else ""
    sliceTupleCase (toString index) interior elements) ++
  (["1:", ":2", "::3", "1:2", "1:2:3", ":2:3", "1::3"].zipIdx.map fun (bound, index) =>
    sliceTupleCase ("bound_" ++ toString index) (bound ++ ",") [.slice]) ++ [
    { id := "slice_tuple_starred", producer := "ast", position := "subscript", binding := "unshadowed",
      source := "def subject(x, values):\n    return x[1, *values]\n", operator := "collection_list_tuple",
      sites := [{ anchor := "1, *values", original := "1, *values", replacement := "[1, *values]", eligible := tupleAllowed [.expression, .expression] }] },
    { id := "slice_tuple_nested", producer := "ast", position := "subscript", binding := "unshadowed",
      source := "def subject(x):\n    return x[:, (1, 2)]\n", operator := "collection_list_tuple",
      sites := [{ anchor := ":, (1, 2)", original := ":, (1, 2)", replacement := "[:, (1, 2)]", eligible := tupleAllowed [.slice, .expression] },
        { anchor := "(1, 2)", original := "(1, 2)", replacement := "[1, 2]", eligible := tupleAllowed [.expression, .expression] }] },
    { id := "slice_tuple_bound_recursion", producer := "ast", position := "subscript", binding := "unshadowed",
      source := "def subject(x):\n    return x[(1, 2):(3, 4):(5, 6),]\n", operator := "collection_list_tuple",
      sites := [{ anchor := "(1, 2):(3, 4):(5, 6),", original := "(1, 2):(3, 4):(5, 6),", replacement := "[(1, 2):(3, 4):(5, 6),]", eligible := tupleAllowed [.slice] },
        { anchor := "(1, 2)", original := "(1, 2)", replacement := "[1, 2]", eligible := tupleAllowed [.expression, .expression] },
        { anchor := "(3, 4)", original := "(3, 4)", replacement := "[3, 4]", eligible := tupleAllowed [.expression, .expression] },
        { anchor := "(5, 6)", original := "(5, 6)", replacement := "[5, 6]", eligible := tupleAllowed [.expression, .expression] }] },
    { id := "slice_tuple_nested_subscript", producer := "ast", position := "expression", binding := "unshadowed",
      source := "def subject(x):\n    return (x[:], 'a:b')\n", operator := "collection_list_tuple",
      sites := [{ anchor := "(x[:], 'a:b')", original := "(x[:], 'a:b')", replacement := "[x[:], 'a:b']", eligible := tupleAllowed [.expression, .expression] }] }
  ]

def cases : List Case := sliceTupleCases ++ [
  { id := "annotation_generic_destination", producer := "annotation", position := "annotation", binding := "generic-destination",
    source := "from typing import Sequence\ndef subject[Sequence](value: list[int]):\n    return value\n", operator := "type_list_sequence",
    sites := [{ anchor := "list[int]", original := "list[int]", replacement := "Sequence[int]", eligible := genericAllowed .destination true, broken := some (brokenGeneric .destination true), fault := "ignore-type-parameter" }] },
  { id := "match_key_unique", producer := "token", position := "match-key", binding := "unshadowed",
    source := "def subject(x):\n    match x:\n        case {False: a, 2: b}: return a\n", operator := "boolean_literal",
    sites := [{ anchor := "False: a", original := "False", replacement := "True", eligible := uniqueReplacement ⟨"True", 1, 0⟩ [⟨"2", 2, 0⟩] }] },
  { id := "bom_first_line_unicode", producer := "token", position := "expression", binding := "unshadowed",
    source := "﻿é = '☃'; value = 1+2\n", operator := "binary_add_sub",
    sites := [{ anchor := "1+2", original := "+", replacement := "-", eligible := true }],
    harness := "print(value)", baseline := "3\n", mutant := "-1\n" },
  { id := "annotation_import_source_shadow", producer := "annotation", position := "annotation", binding := "source-module",
    source := "from typing import Sequence\nSequence = object\nvalue: Sequence[int]\n", operator := "type_list_sequence",
    sites := [{ anchor := "Sequence[int]", original := "Sequence[int]", replacement := "list[int]", eligible := pairAllowed false true }] },
  { id := "except_tuple_ineligible", producer := "ast", position := "except", binding := "unshadowed",
    source := "try:\n    pass\nexcept (ValueError,):\n    pass\n", operator := "exception_type_pair",
    sites := [{ anchor := "ValueError", original := "ValueError", replacement := "TypeError", eligible := false }] },
  { id := "selection_boundary", producer := "token", position := "expression", binding := "unshadowed",
    source := "def subject():\n    left = 1 + 2\n    right = 3 + 4\n    return left, right\n", operator := "binary_add_sub",
    sites := [{ anchor := "1 + 2", original := "+", replacement := "-", eligible := true },
      { anchor := "3 + 4", original := "+", replacement := "-", eligible := true }], variants := true },
  { id := "token_expression", producer := "token", position := "expression", binding := "unshadowed", source := "def subject():\n\té = '☃'; return (1 + 2) # café\n", operator := "binary_add_sub",
    sites := [{ anchor := "1 + 2", original := "+", replacement := "-", eligible := true }], variants := true, harness := "print(subject())", baseline := "3\n", mutant := "-1\n" },
  { id := "token_default", producer := "token", position := "default", binding := "unshadowed", source := "def subject(value=1 + 2):\n    return value\n", operator := "binary_add_sub",
    sites := [{ anchor := "1 + 2", original := "+", replacement := "-", eligible := true }] },
  { id := "token_annotation", producer := "token", position := "annotation", binding := "unshadowed", source := "value: tuple[1 + 2]\n", operator := "binary_add_sub",
    sites := [{ anchor := "1 + 2", original := "+", replacement := "-", eligible := false }] },
  { id := "match_key_bool_integer", producer := "token", position := "match-key", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case {True: a, 0: b}: return a\n", operator := "boolean_literal",
    sites := [{ anchor := "True: a", original := "True", replacement := "False", eligible := uniqueReplacement ⟨"False", 0, 0⟩ [⟨"0", 0, 0⟩], broken := some (brokenLexical ⟨"False", 0, 0⟩ [⟨"0", 0, 0⟩]), fault := "lexical-key" }] },
  { id := "match_key_integer_bool", producer := "token", position := "match-key", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case {False: a, 1: b}: return a\n", operator := "boolean_literal",
    sites := [{ anchor := "False: a", original := "False", replacement := "True", eligible := uniqueReplacement ⟨"True", 1, 0⟩ [⟨"1", 1, 0⟩] }] },
  { id := "match_key_complex", producer := "token", position := "match-key", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case {1+2j: a, 1.0-2.0j: b}: return a\n", operator := "binary_add_sub",
    sites := [{ anchor := "1+2j", original := "+", replacement := "-", eligible := uniqueReplacement ⟨"1-2j", 1, -2⟩ [⟨"1.0-2.0j", 1, -2⟩] }, { anchor := "1.0-2.0j", original := "-", replacement := "+", eligible := uniqueReplacement ⟨"1.0+2.0j", 1, 2⟩ [⟨"1+2j", 1, 2⟩] }] },
  { id := "match_value", producer := "token", position := "match-value", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case {'key': True}: return 1\n", operator := "boolean_literal",
    sites := [{ anchor := "'key': True", original := "True", replacement := "False", eligible := true }] },
  { id := "match_guard", producer := "token", position := "match-guard", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case _ if 1 + 2: return 1\n", operator := "binary_add_sub",
    sites := [{ anchor := "1 + 2", original := "+", replacement := "-", eligible := true }] },
  { id := "unary_pattern", producer := "token", position := "match-value", binding := "unshadowed", source := "def subject(x):\n    match x:\n        case -1: return 1\n", operator := "unary_sign",
    sites := [{ anchor := "case -1", original := "-", replacement := "+", eligible := false }] },
  { id := "except_type", producer := "ast", position := "except", binding := "unshadowed", source := "try:\n    pass\nexcept (ValueError):\n    pass\n", operator := "exception_type_pair",
    sites := [{ anchor := "ValueError", original := "ValueError", replacement := "TypeError", eligible := true }] },
  { id := "except_star_type", producer := "ast", position := "except-star", binding := "unshadowed", source := "try:\n    pass\nexcept* (ValueError):\n    pass\n", operator := "exception_type_pair",
    sites := [{ anchor := "ValueError", original := "ValueError", replacement := "TypeError", eligible := true }] },
  { id := "ast_expression", producer := "ast", position := "expression", binding := "unshadowed", source := "def subject():\n    return list((range(1))) # grouped\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := true }], variants := true, harness := "print(type(subject()).__name__)", baseline := "list\n", mutant := "tuple\n" },
  { id := "ast_source_module", producer := "ast", position := "expression", binding := "source-module", source := "list = lambda x: x\nvalue = list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := pairAllowed false true }] },
  { id := "ast_destination_module", producer := "ast", position := "expression", binding := "destination-module", source := "tuple = lambda x: x\nvalue = list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := pairAllowed true false }] },
  { id := "ast_source_function", producer := "ast", position := "expression", binding := "source-function", source := "def subject(list):\n    return list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := genericAllowed .source true }] },
  { id := "ast_destination_class", producer := "ast", position := "expression", binding := "destination-class", source := "class Subject:\n    tuple = object\n    value = list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := pairAllowed true false }] },
  { id := "ast_global", producer := "ast", position := "expression", binding := "global", source := "list = lambda x: x\ndef subject():\n    global list\n    return list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := scopeAllowed [functionFrame 1 emptyEnv .global, moduleFrame (emptyEnv.set .source .shadowed) (emptyEnv.set .source .shadowed)] }], harness := "print(type(subject()).__name__)", baseline := "range\n" },
  { id := "ast_nonlocal", producer := "ast", position := "expression", binding := "nonlocal", source := "def outer():\n    list = lambda x: x\n    def subject():\n        nonlocal list\n        return list((range(1)))\n    return subject()\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := scopeAllowed [functionFrame 1 emptyEnv .nonlocal, functionFrame 2 (emptyEnv.set .source .shadowed), moduleFrame emptyEnv emptyEnv] }], harness := "print(type(outer()).__name__)", baseline := "range\n" },
  { id := "ast_generic_destination", producer := "ast", position := "expression", binding := "generic-destination", source := "def subject[tuple]():\n    return list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := genericAllowed .destination true, broken := some (brokenGeneric .destination true), fault := "ignore-type-parameter" }] },
  { id := "ast_generic_source", producer := "ast", position := "expression", binding := "generic-source", source := "def subject[list]():\n    return list((range(1)))\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := genericAllowed .source true, broken := some (brokenGeneric .source true), fault := "ignore-type-parameter" }], harness := "try:\n    subject()\nexcept TypeError:\n    print(\"TypeError\")", baseline := "TypeError\n" },
  { id := "ast_generic_default", producer := "ast", position := "default", binding := "generic-destination", source := "def subject[tuple](value=list((range(1)))):\n    return value\n", operator := "collection_list_tuple",
    sites := [{ anchor := "list((range(1)))", original := "list", replacement := "tuple", eligible := genericAllowed .destination false }] },
  { id := "ast_walrus", producer := "ast", position := "expression", binding := "walrus", source := "custom = lambda x: 7\n[(any := custom) for _ in [0]]\nvalue = any((0, 1))\n", operator := "collection_any_all",
    sites := [{ anchor := "any((0, 1))", original := "any", replacement := "all", eligible := walrusAllowed .source, broken := some (brokenWalrus .source), fault := "walrus-local" }], harness := "print(value)", baseline := "7\n", mutant := "" },
  { id := "ast_first_iterable", producer := "ast", position := "first-iterable", binding := "walrus", source := "values = [(all := 0, x)[1] for x in [any((0, 1))]]\n", operator := "collection_any_all",
    sites := [{ anchor := "any((0, 1))", original := "any", replacement := "all", eligible := (HoiminOracle.ComprehensionBinding.evaluate [HoiminOracle.ComprehensionBinding.comp, HoiminOracle.ComprehensionBinding.module] HoiminOracle.ComprehensionBinding.firstWitness).headD false }], harness := "print(values)", baseline := "[True]\n", mutant := "[False]\n" },
  { id := "ast_comprehension_body", producer := "ast", position := "comprehension-body", binding := "iteration-source", source := "values = [any((0, 1)) for any in [lambda x: 7]]\n", operator := "collection_any_all",
    sites := [{ anchor := "any((0, 1))", original := "any", replacement := "all", eligible := false }], harness := "print(values)", baseline := "[7]\n", mutant := "" },
  { id := "ast_after_iteration", producer := "ast", position := "expression", binding := "iteration-source", source := "[any for any in [0]]\nvalue = any((0, 1))\n", operator := "collection_any_all",
    sites := [{ anchor := "any((0, 1))", original := "any", replacement := "all", eligible := iterationAllowed }], harness := "print(value)", baseline := "True\n", mutant := "False\n" },
  { id := "ast_handler_target", producer := "ast", position := "except", binding := "source-function", source := "def subject():\n    try:\n        pass\n    except ValueError as any:\n        return any((0, 1))\n", operator := "collection_any_all",
    sites := [{ anchor := "any((0, 1))", original := "any", replacement := "all", eligible := handlerAllowed }] },
  { id := "annotation_expression", producer := "annotation", position := "annotation", binding := "unshadowed", source := "from typing import Sequence\ndef subject(value: list[int]):\n    return value\n", operator := "type_list_sequence",
    sites := [{ anchor := "list[int]", original := "list[int]", replacement := "Sequence[int]", eligible := true }], variants := true },
  { id := "annotation_builtin_source_boundary", mode := "model-only", producer := "annotation", position := "annotation", binding := "source-module", source := "from typing import Sequence\nlist = object\nvalue: list[int]\n", operator := "type_list_sequence",
    sites := [{ anchor := "list[int]", original := "list[int]", replacement := "Sequence[int]", eligible := pairAllowed false true }] },
  { id := "annotation_destination", producer := "annotation", position := "annotation", binding := "destination-module", source := "from typing import Sequence\nSequence = object\nvalue: list[int]\n", operator := "type_list_sequence",
    sites := [{ anchor := "list[int]", original := "list[int]", replacement := "Sequence[int]", eligible := pairAllowed true false }] },
  { id := "annotation_generic_source_boundary", mode := "model-only", producer := "annotation", position := "annotation", binding := "generic-source", source := "from typing import Sequence\ndef subject[list](value: list[int]):\n    return value\n", operator := "type_list_sequence",
    sites := [{ anchor := "list[int]", original := "list[int]", replacement := "Sequence[int]", eligible := genericAllowed .source true }], harness := "import annotationlib\ntry:\n    annotationlib.get_annotations(subject, format=annotationlib.Format.VALUE)\nexcept TypeError:\n    print(\"TypeError\")", baseline := "TypeError\n" },
  { id := "operator_expression", producer := "operator-import", position := "expression", binding := "unshadowed", source := "import operator\ndef subject():\n    return operator.add(5, 2)\n", operator := "operator_function",
    sites := [{ anchor := "operator.add", original := "add", replacement := "sub", eligible := true }], variants := true, harness := "print(subject())", baseline := "7\n", mutant := "3\n" },
  { id := "operator_source_module", producer := "operator-import", position := "expression", binding := "source-module", source := "import operator\noperator = object()\ndef subject():\n    return operator.add(5, 2)\n", operator := "operator_function",
    sites := [{ anchor := "operator.add", original := "add", replacement := "sub", eligible := pairAllowed false true }] },
  { id := "operator_source_function", producer := "operator-import", position := "expression", binding := "source-function", source := "import operator\ndef subject(operator):\n    return operator.add(5, 2)\n", operator := "operator_function",
    sites := [{ anchor := "operator.add", original := "add", replacement := "sub", eligible := pairAllowed false true }] },
  { id := "operator_generic", producer := "operator-import", position := "expression", binding := "generic-source", source := "import operator\ndef subject[operator]():\n    return operator.add(5, 2)\n", operator := "operator_function",
    sites := [{ anchor := "operator.add", original := "add", replacement := "sub", eligible := genericAllowed .source true, broken := some (brokenGeneric .source true), fault := "ignore-type-parameter" }], harness := "try:\n    subject()\nexcept AttributeError:\n    print(\"AttributeError\")", baseline := "AttributeError\n" },
  { id := "operator_protocol_exception", producer := "operator-import", position := "expression", binding := "unshadowed", source := "import operator\nclass Value:\n    def __add__(self, other): return 7\n    def __sub__(self, other): raise ValueError(\"contract\")\ndef subject():\n    return operator.add(Value(), 2)\n", operator := "operator_function",
    sites := [{ anchor := "operator.add", original := "add", replacement := "sub", eligible := true }], harness := "try:\n    print(subject())\nexcept ValueError:\n    print(\"ValueError\")", baseline := "7\n", mutant := "ValueError\n" }
 ]

def siteJson (source : String) (site : Site) : Lean.Json :=
  let start := ((source.splitOn site.anchor).headD "").toUTF8.size +
    ((site.anchor.splitOn site.original).headD "").toUTF8.size
  Lean.Json.mkObj [
    ("anchor", .str site.anchor), ("original", .str site.original),
    ("replacement", .str site.replacement), ("eligible", Lean.toJson site.eligible),
    ("broken", Lean.toJson site.broken), ("fault", .str site.fault),
    ("expected_bytes", Lean.toJson (replace (source.toUTF8.toList.map UInt8.toNat)
      (site.replacement.toUTF8.toList.map UInt8.toNat) start site.original.toUTF8.size))]

def sourceFor (item : Case) : String :=
  if item.variants then item.source ++ "\ndef empty():\n    pass\n" else item.source

def render : String := String.join (cases.map fun item =>
  (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("seed", Lean.toJson (489 : Nat)),
    ("mode", .str item.mode), ("id", .str item.id), ("producer", .str item.producer),
    ("position", .str item.position), ("binding", .str item.binding),
    ("source", .str (sourceFor item)), ("operator", .str item.operator),
    ("sites", .arr (item.sites.map (siteJson (sourceFor item))).toArray),
    ("variants", Lean.toJson item.variants), ("harness", .str item.harness),
    ("baseline", .str item.baseline), ("mutant", .str item.mutant)
  ]).compress ++ "\n")

def main (args : List String) : IO UInt32 := do
  unless sensitivity && ["walrus-local", "ignore-type-parameter", "lexical-key", "allow-slice-tuple"].all (fun fault =>
      cases.any (fun item => item.sites.any (fun site => site.fault == fault && site.broken.any (fun b => b != site.eligible)))) do
    IO.eprintln "broken variant sensitivity failed"
    return 2
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path render; return 0
  | ["--check", path] => return if (← IO.FS.readFile path) == render then 0 else 1
  | ["--sensitivity"] | ["--stats"] =>
      IO.println s!"cases={cases.length} seed=489 names=2 visibility_states=2 key_alias_pairs=3 max_frames=3 sensitivity={sensitivity}"
      return 0
  | _ => IO.eprintln "use --output PATH, --check PATH, --sensitivity or --stats"; return 2
