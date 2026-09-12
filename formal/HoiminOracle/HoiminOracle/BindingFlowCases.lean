import HoiminOracle.BindingFlowProofs

namespace HoiminOracle.BindingFlow

def knownEnv (target : Target) : Env where
  source := .known target
  destination := .known target

def unknownEnv : Env where
  source := .unknown
  destination := .unknown

def destinationFact (fact : Fact) : Env where
  source := .absent
  destination := fact

def sourceFact (fact : Fact) : Env where
  source := fact
  destination := .absent

def resolvedEnv (candidate : Candidate) : Env where
  source := (resolveCandidate candidate).source
  destination := (resolveCandidate candidate).destination

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  family : String
  operator : String
  source : String
  siteMarker : String
  initial : Env
  program : Stmt
  fuel : Nat := 8
  sourceTarget : Target := .builtin
  target : Target
  expectedPresent : Bool := false
  original : String := "list[str]"
  replacement : String
  symbol : Option String := none
  loopBody : Option Stmt := none
  deriving Repr, DecidableEq, BEq

def caseResult (item : OracleCase) : Exits :=
  eval item.fuel item.program item.initial

def caseExpectedPresent (item : OracleCase) : Bool :=
  match (caseResult item).fallthrough with
  | none => false
  | some environment =>
      allowsCandidate environment item.sourceTarget item.target

def caseExpectedReplacement (item : OracleCase) : Option String :=
  if caseExpectedPresent item then some item.replacement else none

def caseExpectedSymbol (item : OracleCase) : Option String :=
  if caseExpectedPresent item then item.symbol else none

def normalizedKnownImports (environment : Env) : List String :=
  if environment.destination = .known .typing then
    ["direct:Sequence=typing.Sequence"]
  else
    []

def normalizedExitStates (states : List Env) : List (List String) :=
  states.map normalizedKnownImports

def caseExpectedFallthrough (item : OracleCase) : List (List String) :=
  normalizedExitStates (caseResult item).fallthrough.toList

def caseExpectedBreaks (item : OracleCase) : List (List String) :=
  normalizedExitStates (caseResult item).breaks

def caseExpectedContinues (item : OracleCase) : List (List String) :=
  normalizedExitStates (caseResult item).continues

def caseExpectedTerminates (item : OracleCase) : List (List String) :=
  normalizedExitStates (caseResult item).terminates

def caseExpectedLoopHead (item : OracleCase) : Option (List String) :=
  item.loopBody.bind fun body =>
    let transfer := fun head =>
      let bodyResult := eval (item.fuel - 1) body head
      meetStates item.initial
        (bodyResult.fallthrough.toList ++ bodyResult.continues)
    (iterateToFixedPoint (item.initial.rank + 1) transfer item.initial).map
      normalizedKnownImports

def typingInitial : Env where
  source := .known .builtin
  destination := .known .typing
def builtinInitial : Env := knownEnv .builtin

def functionWholeCandidate : Candidate := {
  path := [
    functionFrame 2 (destinationFact .shadowed),
    moduleFrame emptyEnv typingInitial
  ]
}

def classMethodCandidate : Candidate := {
  path := [
    functionFrame 2 emptyEnv,
    classFrame 1 (destinationFact .shadowed) (destinationFact .shadowed),
    moduleFrame emptyEnv typingInitial
  ]
}

def classDirectBeforeCandidate : Candidate := {
  path := [
    classFrame 1 emptyEnv (destinationFact .shadowed),
    moduleFrame emptyEnv typingInitial
  ]
}

def classDirectAfterCandidate : Candidate := {
  path := [
    classFrame 1 (destinationFact .shadowed) (destinationFact .shadowed),
    moduleFrame emptyEnv typingInitial
  ]
}

def globalUnknownCandidate : Candidate := {
  path := [
    functionFrame 2 emptyEnv .global,
    moduleFrame emptyEnv (destinationFact .unknown)
  ]
}

def nonlocalUnknownCandidate : Candidate := {
  path := [
    functionFrame 2 emptyEnv .nonlocal,
    functionFrame 1 (destinationFact .unknown),
    moduleFrame emptyEnv typingInitial
  ]
}

def classGlobalClosurePath : List Frame := [
  classFrame 2 emptyEnv emptyEnv .global,
  functionFrame 1 (destinationFact .shadowed),
  moduleFrame emptyEnv emptyEnv
]

def classGlobalMethodCandidate : Candidate := {
  path := functionFrame 3 emptyEnv :: classGlobalClosurePath
}

def classGlobalDirectCandidate : Candidate := { path := classGlobalClosurePath }

def methodGlobalCandidate : Candidate := {
  path := functionFrame 3 emptyEnv .global :: classGlobalClosurePath
}

def cases : List OracleCase := [
  {
    id := "typing_if_identical"
    mode := "strict"
    family := "typing-import"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nif flag:\n    from typing import Sequence\nelse:\n    from typing import Sequence\nidentical_marker: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .branch (.bind .destination (.known .typing))
      (.bind .destination (.known .typing))
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
  },
  {
    id := "typing_if_disagrees"
    mode := "strict"
    family := "typing-import"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nif flag:\n    Sequence = local_sequence\nelse:\n    from typing import Sequence\ndisagree_marker: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .branch (.bind .destination .shadowed)
      (.bind .destination (.known .typing))
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_loop_zero_iteration"
    mode := "internal-fixture"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nfor item in items:\n    Sequence = local_sequence\nafter_loop: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .loop (.bind .destination .shadowed)
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_loop_continue_backedge"
    mode := "internal-fixture"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nwhile condition:\n    Sequence = local_sequence\n    continue\nafter_continue: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .loop (.seq (.bind .destination .shadowed) .continueNow)
    loopBody := some (.seq (.bind .destination .shadowed) .continueNow)
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_loop_break_exit"
    mode := "internal-fixture"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nwhile condition:\n    Sequence = local_sequence\n    break\nafter_break: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .loop (.seq (.bind .destination .shadowed) .breakNow)
    loopBody := some (.seq (.bind .destination .shadowed) .breakNow)
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_try_handler_join"
    mode := "strict"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ntry:\n    Sequence = local_sequence\nexcept Error:\n    from typing import Sequence\nafter_try: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .tryFlow (.bind .destination .shadowed)
      { typingInitial with destination := .unknown }
      (.bind .destination (.known .typing)) .skip .skip
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_finally_restores"
    mode := "strict"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ntry:\n    Sequence = local_sequence\nfinally:\n    from typing import Sequence\nafter_finally: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .tryFinally (.bind .destination .shadowed)
      (.bind .destination (.known .typing))
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
  },
  {
    id := "typing_abrupt_finally"
    mode := "model-only"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ntry:\n    pass\nfinally:\n    raise Error\nunreachable: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .tryFinally .skip .terminateNow
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_match_unmatched_path"
    mode := "strict"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nmatch value:\n    case 0:\n        Sequence = local_sequence\nafter_match: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .matchFlow (.bind .destination .shadowed) .skip
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_match_irrefutable_reimport"
    mode := "strict"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nSequence = local_sequence\nmatch value:\n    case _:\n        from typing import Sequence\nafter_irrefutable: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .matchIrrefutable
      (.seq (.bind .destination .shadowed)
        (.bind .destination (.known .typing)))
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
  },
  {
    id := "typing_match_guard_binding"
    mode := "internal-fixture"
    family := "control-flow"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nmatch value:\n    case 0 if (Sequence := local_sequence):\n        pass\n    case _:\n        guarded: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .matchFlow (.bind .destination .shadowed)
      (.bind .destination .shadowed)
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_function_whole_block_local"
    mode := "strict"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ndef local_scope():\n    hidden: list[str]\n    Sequence = local_sequence\n"
    siteMarker := "list[str]"
    initial := resolvedEnv functionWholeCandidate
    program := .scoped functionWholeCandidate .skip
    target := .typing
    replacement := "Sequence[str]"
    symbol := some "local_scope"
  },
  {
    id := "typing_method_skips_class"
    mode := "strict"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nclass Box:\n    Sequence = local_sequence\n    def method(self):\n        visible: list[str]\n"
    siteMarker := "list[str]"
    initial := resolvedEnv classMethodCandidate
    program := .scoped classMethodCandidate .skip
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
    symbol := some "Box.method"
  },
  {
    id := "typing_class_before_binding"
    mode := "strict"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nclass Before:\n    visible: list[str]\n    Sequence = local_sequence\n"
    siteMarker := "list[str]"
    initial := resolvedEnv classDirectBeforeCandidate
    program := .scoped classDirectBeforeCandidate .skip
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
    symbol := some "Before"
  },
  {
    id := "typing_class_after_binding"
    mode := "strict"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nclass After:\n    Sequence = local_sequence\n    hidden: list[str]\n"
    siteMarker := "list[str]"
    initial := resolvedEnv classDirectAfterCandidate
    program := .scoped classDirectAfterCandidate .skip
    target := .typing
    replacement := "Sequence[str]"
    symbol := some "After"
  },
  {
    id := "typing_global_unknown"
    mode := "model-only"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ndef global_scope():\n    global Sequence\n    hidden: list[str]\n"
    siteMarker := "list[str]"
    initial := resolvedEnv globalUnknownCandidate
    program := .scoped globalUnknownCandidate .skip
    target := .typing
    replacement := "Sequence[str]"
    symbol := some "global_scope"
  },
  {
    id := "typing_nonlocal_unknown"
    mode := "model-only"
    family := "scope"
    operator := "type_list_sequence"
    source := "from typing import Sequence\ndef outer():\n    def inner():\n        nonlocal Sequence\n        hidden: list[str]\n"
    siteMarker := "list[str]"
    initial := resolvedEnv nonlocalUnknownCandidate
    program := .scoped nonlocalUnknownCandidate .skip
    target := .typing
    replacement := "Sequence[str]"
    symbol := some "outer.inner"
  },
  {
    id := "typing_wildcard_unknown"
    mode := "strict"
    family := "typing-import"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nfrom helpers import *\nhidden_wildcard: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .bind .destination .unknown
    target := .typing
    replacement := "Sequence[str]"
  },
  {
    id := "typing_unconditional_reimport"
    mode := "strict"
    family := "typing-import"
    operator := "type_list_sequence"
    source := "from typing import Sequence\nSequence = local_sequence\nfrom typing import Sequence\nrestored: list[str]\n"
    siteMarker := "list[str]"
    initial := typingInitial
    program := .seq (.bind .destination .shadowed)
      (.bind .destination (.known .typing))
    target := .typing
    expectedPresent := true
    replacement := "Sequence[str]"
  },
  {
    id := "builtin_pair_clean"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "clean_marker = list(items)\n"
    siteMarker := "list"
    initial := builtinInitial
    program := .skip
    target := .builtin
    expectedPresent := true
    original := "list"
    replacement := "tuple"
  },
  {
    id := "builtin_source_shadowed"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "list = local_list\nhidden_source = list(items)\n"
    siteMarker := "list(items)"
    initial := builtinInitial
    program := .bind .source .shadowed
    target := .builtin
    original := "list"
    replacement := "tuple"
  },
  {
    id := "builtin_destination_shadowed"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "tuple = local_tuple\nhidden_destination = list(items)\n"
    siteMarker := "list(items)"
    initial := builtinInitial
    program := .bind .destination .shadowed
    target := .builtin
    original := "list"
    replacement := "tuple"
  },
  {
    id := "builtin_method_skips_class_global"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "def outer():\n    tuple = object()\n    class C:\n        global tuple\n        def method(self):\n            return list(range(2))\n    return C\n"
    siteMarker := "list(range(2))"
    initial := builtinInitial
    program := .scoped classGlobalMethodCandidate .skip
    target := .builtin
    expectedPresent := false
    original := "list"
    replacement := "tuple"
    symbol := some "outer.C.method"
  },
  {
    id := "builtin_direct_class_global"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "def outer():\n    tuple = object()\n    class C:\n        global tuple\n        value = list(range(2))\n    return C\n"
    siteMarker := "list(range(2))"
    initial := builtinInitial
    program := .scoped classGlobalDirectCandidate .skip
    target := .builtin
    expectedPresent := true
    original := "list"
    replacement := "tuple"
    symbol := some "outer.C"
  },
  {
    id := "builtin_method_own_global"
    mode := "strict"
    family := "builtin-pair"
    operator := "collection_list_tuple"
    source := "def outer():\n    tuple = object()\n    class C:\n        global tuple\n        def method(self):\n            global tuple\n            return list(range(2))\n    return C\n"
    siteMarker := "list(range(2))"
    initial := builtinInitial
    program := .scoped methodGlobalCandidate .skip
    target := .builtin
    expectedPresent := true
    original := "list"
    replacement := "tuple"
    symbol := some "outer.C.method"
  },
  {
    id := "exception_pair_clean"
    mode := "strict"
    family := "exception-pair"
    operator := "exception_type_pair"
    source := "raise ValueError\n"
    siteMarker := "ValueError"
    initial := builtinInitial
    program := .skip
    target := .builtin
    expectedPresent := true
    original := "ValueError"
    replacement := "TypeError"
  },
  {
    id := "exception_source_shadowed"
    mode := "strict"
    family := "exception-pair"
    operator := "exception_type_pair"
    source := "ValueError = CustomValueError\nraise ValueError\n"
    siteMarker := "raise ValueError"
    initial := builtinInitial
    program := .bind .source .shadowed
    target := .builtin
    original := "ValueError"
    replacement := "TypeError"
  },
  {
    id := "exception_destination_shadowed"
    mode := "strict"
    family := "exception-pair"
    operator := "exception_type_pair"
    source := "TypeError = CustomTypeError\nraise ValueError\n"
    siteMarker := "ValueError"
    initial := builtinInitial
    program := .bind .destination .shadowed
    target := .builtin
    original := "ValueError"
    replacement := "TypeError"
  }
]

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 && !item.id.isEmpty && !item.source.isEmpty &&
    !item.siteMarker.isEmpty && !item.original.isEmpty &&
    item.program.requiredFuel <= item.fuel &&
    (!item.loopBody.isSome || (caseExpectedLoopHead item).isSome) &&
    caseExpectedPresent item == item.expectedPresent &&
    (!caseExpectedPresent item || (caseExpectedReplacement item).isSome)

def fixedCasesPass : Bool := cases.all caseSafe

def brokenUnionMeet (left right : Fact) : Fact :=
  match left, right with
  | .known target, _ | _, .known target => .known target
  | _, _ => left.meet right

def brokenResolveThroughClass (name : Name) (candidate : Candidate) : Fact :=
  match candidate.path with
  | functionValue :: classValue :: _ =>
      if functionValue.kind == .function && classValue.kind == .class then
        factResolution (classValue.whole.get name) (resolve name candidate)
      else
        resolve name candidate
  | _ => resolve name candidate

def brokenRouteFinally
    (_category : ExitCategory)
    (finallyResult : Exits) : Exits :=
  finallyResult

def brokenLoopIteration (_head backEdge : Env) : Env := backEdge

def brokenAllowsSourceOnly (environment : Env) (target : Target) : Bool :=
  decide (environment.source = .known target)

def unionSensitivity : Bool :=
  (.known .typing : Fact).meet .shadowed = .unknown &&
    brokenUnionMeet (.known .typing) .shadowed = .known .typing

def classSensitivity : Bool :=
  resolve .destination classMethodCandidate = .known .typing &&
    brokenResolveThroughClass .destination classMethodCandidate = .shadowed

-- The pre-fix rule inspects an indirectly reached class directive before
-- deciding whether that class participates in lexical lookup.
def brokenClassDirectiveFirst (name : Name) (candidate : Candidate) : Fact :=
  match candidate.path with
  | method :: frame :: rest =>
      if method.kind == .function && frame.kind == .class &&
          frame.directive == .global then resolveModuleWhole name rest
      else resolve name candidate
  | _ => resolve name candidate

def classDirectiveSensitivity : Bool :=
  resolve .destination classGlobalMethodCandidate == .shadowed &&
  brokenClassDirectiveFirst .destination classGlobalMethodCandidate == .known .builtin &&
  resolve .destination classGlobalDirectCandidate == .known .builtin &&
  resolve .destination methodGlobalCandidate == .known .builtin

def finallySensitivity : Bool :=
  routeCategory .continue (Exits.fallthroughOnly typingInitial) =
      Exits.categoryOnly .continue typingInitial &&
    brokenRouteFinally .continue (Exits.fallthroughOnly typingInitial) !=
      Exits.categoryOnly .continue typingInitial

def loopSensitivity : Bool :=
  loopIteration typingInitial { typingInitial with destination := .shadowed } =
      { typingInitial with destination := .unknown } &&
    brokenLoopIteration typingInitial
      { typingInitial with destination := .shadowed } !=
      { typingInitial with destination := .unknown }

def destinationSensitivity : Bool :=
  !allowsCandidate
      { builtinInitial with destination := .shadowed } .builtin .builtin &&
    brokenAllowsSourceOnly
      { builtinInitial with destination := .shadowed } .builtin

def sensitivityPasses : Bool :=
  unionSensitivity && classSensitivity && classDirectiveSensitivity && finallySensitivity &&
    loopSensitivity && destinationSensitivity

def auditAtoms : List Stmt := [
  .skip,
  .bind .source (.known .builtin),
  .bind .source .shadowed,
  .bind .destination (.known .builtin),
  .bind .destination .shadowed,
  .breakNow,
  .continueNow,
  .terminateNow
]

def normalizeStates (states : List Env) : List Env :=
  match meetAll? states with
  | none => []
  | some environment => [environment]

def normalizeExits (exits : Exits) : Exits where
  fallthrough := exits.fallthrough
  breaks := normalizeStates exits.breaks
  continues := normalizeStates exits.continues
  terminates := normalizeStates exits.terminates

structure Reachable where
  program : Stmt
  exits : Exits
  allowed : Bool
  deriving Repr, DecidableEq, BEq

def observeProgram (program : Stmt) : Reachable :=
  let exits := normalizeExits (eval 8 program builtinInitial)
  let allowed := match exits.fallthrough with
    | none => false
    | some environment => allowsCandidate environment .builtin .builtin
  { program, exits, allowed }

def sameObservation (left right : Reachable) : Bool :=
  left.exits == right.exits && left.allowed == right.allowed

def retainNew (retained : List Reachable) (candidate : Reachable) : List Reachable :=
  if retained.any fun item => sameObservation item candidate then retained
  else retained ++ [candidate]

def uniqueObservations (candidates : List Reachable) : List Reachable :=
  candidates.foldl retainNew []

def expandReachable (item : Reachable) : List Reachable :=
  auditAtoms.flatMap fun atom => [
    observeProgram (.seq item.program atom),
    observeProgram (.branch item.program atom)
  ] ++ [
    observeProgram (.loop item.program),
    observeProgram (.tryFinally item.program .skip)
  ]

def explorationLayers : Nat → List (List Reachable)
  | 0 => [uniqueObservations (auditAtoms.map observeProgram)]
  | depth + 1 =>
      let previous := explorationLayers depth
      let frontier := previous.getLastD []
      previous ++ [uniqueObservations (frontier.flatMap expandReachable)]

def reachableAtDepth (depth : Nat) : List Reachable :=
  (explorationLayers depth).flatten

def stateCount (depth : Nat) : Nat :=
  (explorationLayers depth).foldl (fun count layer => count + layer.length) 0

def transitionCount (depth : Nat) : Nat :=
  match explorationLayers depth with
  | [] | [_] => 0
  | layers =>
      (layers.dropLast).foldl
        (fun count layer => count + layer.length * (auditAtoms.length * 2 + 2)) 0

def boundedAuditPasses (depth : Nat) : Bool :=
  fixedCasesPass && sensitivityPasses && stateCount depth <= 1024

end HoiminOracle.BindingFlow
