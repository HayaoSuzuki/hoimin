import HoiminOracle.NestedMatchExitProofs

namespace HoiminOracle.NestedMatchExit

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

inductive ObservationKind
  | tryExit
  | loopHead
  | publicCandidate
  deriving Repr, DecidableEq, BEq

inductive CandidateExpectation
  | notObserved
  | present
  | absent
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  observationKind : ObservationKind
  family : String
  source : String
  marker : String
  expectedExits : Exits := .empty
  expectedFacts : List String := []
  candidate : CandidateExpectation := .notObserved
  candidateCount : Nat := 0
  candidatePath : Option String := none
  candidateOperator : Option String := none
  candidateOriginal : Option String := none
  candidateReplacement : Option String := none
  candidateSymbol : Option String := none
  deriving Repr, DecidableEq, BEq

def knownBoth : Env where
  source := .known .typing
  destination := .known .typing

def sequenceOnly : Env where
  source := .known .typing
  destination := .absent

def sequenceWithShadowedMapping : Env where
  source := .known .typing
  destination := .shadowed

def shadowedSequence : Env where
  source := .shadowed
  destination := .absent

def bothShadowed : Env where
  source := .shadowed
  destination := .shadowed

def identityStep (environment : Env) : Exits :=
  .fallthroughOnly environment

def reimportSequence (environment : Env) : Exits :=
  .fallthroughOnly (environment.set .source (.known .typing))

def handlerBreakContinueExpected : Exits :=
  composeTry (.fallthroughOnly sequenceOnly)
    (composeMatch
      [.categoryOnly .break sequenceOnly, .categoryOnly .continue bothShadowed]
      none)
    (some .source) identityStep reimportSequence

def handlerFallthroughTerminateExpected : Exits :=
  composeTry (.fallthroughOnly knownBoth)
    (composeMatch
      [.categoryOnly .terminate knownBoth,
        .categoryOnly .terminate knownBoth,
        .fallthroughOnly knownBoth]
      none)
    (some .source) identityStep reimportSequence

def unreachableBreakExpected : Exits :=
  composeTry (.fallthroughOnly knownBoth)
    (composeMatch [.categoryOnly .terminate knownBoth] none)
    (some .source) identityStep reimportSequence

def loopHeadBody : Exits :=
  composeMatch
    [.categoryOnly .continue shadowedSequence,
      .categoryOnly .break sequenceOnly]
    none

def postLoopBody : Exits :=
  composeMatch
    [.categoryOnly .break shadowedSequence,
      .categoryOnly .continue sequenceOnly]
    none

def postLoopExpected : Exits :=
  consumeLoop sequenceOnly postLoopBody reimportSequence

def handlerBreakContinueSource : String :=
  "from typing import Mapping, Sequence\n\nwhile condition:\n    try:  # handler_match_break_continue\n        risky()\n    except Error as Sequence:\n        match value:\n            case 0:\n                from typing import Sequence\n                break\n            case _:\n                Mapping = object\n                continue\n    finally:\n        from typing import Sequence\n"

def handlerFallthroughTerminateSource : String :=
  "def f():\n    from typing import Mapping, Sequence\n    try:  # handler_match_fallthrough_terminate\n        risky()\n    except Error as Sequence:\n        match value:\n            case 0:\n                from typing import Sequence\n                return None\n            case 1:\n                from typing import Sequence\n                raise RuntimeError\n            case _:\n                from typing import Sequence\n    finally:\n        from typing import Sequence\n"

def unreachableBreakSource : String :=
  "def f():\n    from typing import Mapping, Sequence\n    while condition:\n        try:  # irrefutable_terminate\n            risky()\n        except Error as Sequence:\n            match value:\n                case _:\n                    from typing import Sequence\n                    return None\n                case 1:\n                    break\n        finally:\n            from typing import Sequence\n"

def loopHeadSource : String :=
  "from typing import Sequence\n\nwhile condition:  # nested_continue_loop\n    match value:\n        case 0:\n            observed: Sequence[int]\n            Sequence = object\n            continue\n        case _:\n            break\n"

def postLoopSource : String :=
  "from typing import Sequence\n\nwhile condition:\n    match value:\n        case 0:\n            Sequence = object\n            break\n        case _:\n            continue\nelse:\n    from typing import Sequence\n\npost_loop: Sequence[int]\n"

def publicAbsent
    (id family source marker : String) : OracleCase where
  id
  mode := "strict"
  observationKind := .publicCandidate
  family
  source
  marker
  candidate := .absent

def nestedMatchExitCases : List OracleCase := [
  { id := "handler_match_break_continue_categories"
    mode := "internal-fixture"
    observationKind := .tryExit
    family := "nested-handler-categories"
    source := handlerBreakContinueSource
    marker := "# handler_match_break_continue"
    expectedExits := handlerBreakContinueExpected },
  { id := "handler_match_fallthrough_terminate_categories"
    mode := "internal-fixture"
    observationKind := .tryExit
    family := "nested-handler-categories"
    source := handlerFallthroughTerminateSource
    marker := "# handler_match_fallthrough_terminate"
    expectedExits := handlerFallthroughTerminateExpected },
  { id := "irrefutable_terminate_excludes_later_break"
    mode := "internal-fixture"
    observationKind := .tryExit
    family := "nested-reachability"
    source := unreachableBreakSource
    marker := "# irrefutable_terminate"
    expectedExits := unreachableBreakExpected },
  { id := "nested_continue_reaches_loop_head"
    mode := "internal-fixture"
    observationKind := .loopHead
    family := "loop-back-edge"
    source := loopHeadSource
    marker := "# nested_continue_loop"
    expectedFacts := [] },
  publicAbsent "nested_continue_suppresses_public_candidate"
    "loop-back-edge" loopHeadSource "Sequence[int]",
  publicAbsent "post_loop_meets_break_and_natural_exit"
    "loop-consumption" postLoopSource "Sequence[int]"
]

def flattenToFallthrough (exits : Exits) : Exits where
  fallthrough := meetAll? exits.states

def nestedCategorySensitivity : Bool :=
  let detects := fun category =>
    let correct := composeMatch [.categoryOnly category knownBoth] none
    correct != flattenToFallthrough correct
  detects .break && detects .continue && detects .terminate

def retainUnreachableSensitivity : Bool :=
  composeMatch [.categoryOnly .terminate knownBoth] none !=
    composeMatch
      [.categoryOnly .terminate knownBoth, .categoryOnly .break sequenceOnly]
      none

def omitAbruptSensitivity : Bool :=
  composeMatch
      [.categoryOnly .break knownBoth, .categoryOnly .continue sequenceOnly]
      none !=
    composeMatch [.categoryOnly .break knownBoth] none

def brokenCleanupAfterFinally (target : Name) (handler : Exits)
    (finalizer : Env → Exits) : Exits :=
  cleanupExits target (routeFinally handler finalizer)

def cleanupBoundarySensitivity : Bool :=
  let handler := composeMatch [.categoryOnly .break knownBoth] none
  let correct := composeTry .empty handler (some .source) identityStep
    reimportSequence
  correct != brokenCleanupAfterFinally .source handler reimportSequence

def brokenLoopNaturalEntry (zeroIteration : Env) (body : Exits) : Env :=
  meetAll? ([zeroIteration] ++ body.fallthrough.toList)
    |>.getD zeroIteration

def continueBackEdgeSensitivity : Bool :=
  loopNaturalEntry sequenceOnly loopHeadBody !=
    brokenLoopNaturalEntry sequenceOnly loopHeadBody

def sensitivityPasses : Bool :=
  nestedCategorySensitivity && retainUnreachableSensitivity &&
    omitAbruptSensitivity && cleanupBoundarySensitivity &&
    continueBackEdgeSensitivity

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "internal-fixture" ||
    mode == "model-only" || mode == "infrastructure-error"

def markerOccursOnce (item : OracleCase) : Bool :=
  !item.marker.isEmpty && (item.source.splitOn item.marker).length == 2

def publicFieldsValid (item : OracleCase) : Bool :=
  match item.candidate with
  | .notObserved =>
      item.candidateCount == 0 && item.candidatePath == none &&
        item.candidateOperator == none && item.candidateOriginal == none &&
        item.candidateReplacement == none && item.candidateSymbol == none
  | .absent =>
      item.candidateCount == 0 && item.candidatePath == none &&
        item.candidateOperator == none && item.candidateOriginal == none &&
        item.candidateReplacement == none && item.candidateSymbol == none
  | .present =>
      item.candidateCount == 1 && item.candidatePath.isSome &&
        item.candidateOperator.isSome && item.candidateOriginal.isSome &&
        item.candidateReplacement.isSome

def identityValid (item : OracleCase) : Bool :=
  match item.id with
  | "handler_match_break_continue_categories" =>
      item.mode == "internal-fixture" && item.observationKind == .tryExit &&
        item.family == "nested-handler-categories" &&
        item.source == handlerBreakContinueSource &&
        item.marker == "# handler_match_break_continue" &&
        item.expectedExits == handlerBreakContinueExpected
  | "handler_match_fallthrough_terminate_categories" =>
      item.mode == "internal-fixture" && item.observationKind == .tryExit &&
        item.family == "nested-handler-categories" &&
        item.source == handlerFallthroughTerminateSource &&
        item.marker == "# handler_match_fallthrough_terminate" &&
        item.expectedExits == handlerFallthroughTerminateExpected
  | "irrefutable_terminate_excludes_later_break" =>
      item.mode == "internal-fixture" && item.observationKind == .tryExit &&
        item.family == "nested-reachability" &&
        item.source == unreachableBreakSource &&
        item.marker == "# irrefutable_terminate" &&
        item.expectedExits == unreachableBreakExpected
  | "nested_continue_reaches_loop_head" =>
      item.mode == "internal-fixture" && item.observationKind == .loopHead &&
        item.family == "loop-back-edge" && item.source == loopHeadSource &&
        item.marker == "# nested_continue_loop" && item.expectedFacts == []
  | "nested_continue_suppresses_public_candidate" =>
      item.mode == "strict" && item.observationKind == .publicCandidate &&
        item.family == "loop-back-edge" && item.source == loopHeadSource &&
        item.marker == "Sequence[int]" && item.candidate == .absent
  | "post_loop_meets_break_and_natural_exit" =>
      item.mode == "strict" && item.observationKind == .publicCandidate &&
        item.family == "loop-consumption" && item.source == postLoopSource &&
        item.marker == "Sequence[int]" && item.candidate == .absent
  | _ => false

def fixedCasesPass : Bool :=
  nestedMatchExitCases.length == 6 &&
    nestedMatchExitCases.all fun item =>
      item.schema == 1 && validMode item.mode && markerOccursOnce item &&
        publicFieldsValid item && identityValid item

end HoiminOracle.NestedMatchExit
