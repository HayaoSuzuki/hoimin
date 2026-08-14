import HoiminOracle.MultipleHandlerJoinProofs

namespace HoiminOracle.MultipleHandlerJoin

open HoiminOracle.BindingFlow

inductive ObservationKind
  | tryExit
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
  candidate : CandidateExpectation := .notObserved
  candidateCount : Nat := 0
  candidatePath : Option String := none
  candidateStart : Option Nat := none
  candidateLength : Option Nat := none
  candidateOperator : Option String := none
  candidateOriginal : Option String := none
  candidateReplacement : Option String := none
  candidateSymbol : Option String := none
  deriving Repr, DecidableEq, BEq

def bothKnown : Env where
  source := .known .typing
  destination := .known .typing

def mappingOnly : Env where
  source := .absent
  destination := .known .typing

def sequenceShadowedMappingKnown : Env where
  source := .shadowed
  destination := .known .typing

def normalAndHandlers (normal : Exits) (steps : List HandlerStep)
    (remainder : Option Env := some bothKnown) : Exits :=
  normal.merge (finishHandlers (routeHandlers remainder steps))

def fallthroughStep (environment : Env) (remainder : Option Env)
    (target : Option Name := none) : HandlerStep where
  selected := some (.fallthroughOnly environment)
  remainder
  target

def categoryStep (category : ExitCategory) (environment : Env)
    (remainder : Option Env) (target : Option Name := none) : HandlerStep where
  selected := some (.categoryOnly category environment)
  remainder
  target

def twoHandlersExpected : Exits :=
  normalAndHandlers (.fallthroughOnly bothKnown)
    [fallthroughStep bothKnown (some bothKnown),
      fallthroughStep sequenceShadowedMappingKnown none]

def threeHandlersExpected : Exits :=
  normalAndHandlers (.fallthroughOnly bothKnown)
    [fallthroughStep sequenceShadowedMappingKnown (some bothKnown),
      fallthroughStep bothKnown (some bothKnown),
      fallthroughStep bothKnown none]

def cleanupCategoriesExpected : Exits :=
  normalAndHandlers (.fallthroughOnly bothKnown)
    [fallthroughStep bothKnown (some bothKnown) (some .source),
      categoryStep .terminate bothKnown none (some .source)]

def breakContinueExpected : Exits :=
  normalAndHandlers (.fallthroughOnly bothKnown)
    [categoryStep .break bothKnown (some bothKnown) (some .source),
      categoryStep .continue bothKnown none (some .source)]

def unhandledRemainderExpected : Exits :=
  normalAndHandlers .empty
    [fallthroughStep bothKnown (some bothKnown),
      fallthroughStep bothKnown (some bothKnown)]

def twoHandlersSource : String :=
  "from typing import Mapping, Sequence\ntry:  # two_handlers_disagree_fallthrough\n    work()\nexcept FirstError:\n    from typing import Sequence\nexcept SecondError:\n    Sequence = object\nafter_disagreement: Sequence[int]\npreserved_mapping: Mapping[str, int]\n"

def threeHandlersSource : String :=
  "from typing import Mapping, Sequence\ntry:  # three_handlers_preserve_mapping\n    work()\nexcept FirstError:\n    Sequence = object\nexcept SecondError:\n    from typing import Sequence\nexcept ThirdError:\n    pass\nafter_three: Mapping[str, int]\n"

def cleanupCategoriesSource : String :=
  "def run(flag):\n    from typing import Mapping, Sequence\n    try:  # per_handler_cleanup_categories\n        work()\n    except FirstError as Sequence:\n        from typing import Sequence\n    except SecondError as Sequence:\n        from typing import Sequence\n        return flag\n"

def breakContinueSource : String :=
  "from typing import Mapping, Sequence\nwhile active:\n    try:  # different_handler_break_continue\n        work()\n    except FirstError as Sequence:\n        from typing import Sequence\n        break\n    except SecondError as Sequence:\n        from typing import Sequence\n        continue\n"

def unhandledRemainderSource : String :=
  "def run():\n    from typing import Mapping, Sequence\n    try:  # unhandled_remainder_terminates\n        raise UnknownError\n    except FirstError:\n        pass\n    except SecondError:\n        pass\n"

def publicPresentSource : String :=
  "from typing import Sequence\ntry:\n    work()\nexcept FirstError:\n    from typing import Sequence\nexcept SecondError:\n    from typing import Sequence\nall_handlers_preserve: Sequence[int]\n"

def publicAbsentSource : String :=
  "from typing import Sequence\ntry:\n    work()\nexcept FirstError:\n    from typing import Sequence\nexcept SecondError:\n    Sequence = object\none_handler_shadows: Sequence[int]\n"

def internalCase (id family source marker : String) (expected : Exits) : OracleCase where
  id
  mode := "internal-fixture"
  observationKind := .tryExit
  family
  source
  marker
  expectedExits := expected

def strictPresentCase : OracleCase where
  id := "all_handlers_preserve_public_candidate"
  mode := "strict"
  observationKind := .publicCandidate
  family := "handler-fallthrough-meet"
  source := publicPresentSource
  marker := "Sequence[int]"
  candidate := .present
  candidateCount := 1
  candidatePath := some "target.py"
  candidateStart := some 170
  candidateLength := some 13
  candidateOperator := some "type_list_sequence"
  candidateOriginal := some "Sequence[int]"
  candidateReplacement := some "list[int]"

def strictAbsentCase : OracleCase where
  id := "one_handler_shadows_public_candidate"
  mode := "strict"
  observationKind := .publicCandidate
  family := "handler-fallthrough-meet"
  source := publicAbsentSource
  marker := "Sequence[int]"
  candidate := .absent

def multipleHandlerJoinCases : List OracleCase := [
  internalCase "two_handlers_disagree_fallthrough" "handler-fallthrough-meet"
    twoHandlersSource "# two_handlers_disagree_fallthrough" twoHandlersExpected,
  internalCase "three_handlers_preserve_mapping" "unrelated-fact-preservation"
    threeHandlersSource "# three_handlers_preserve_mapping" threeHandlersExpected,
  internalCase "per_handler_cleanup_categories" "per-handler-cleanup"
    cleanupCategoriesSource "# per_handler_cleanup_categories" cleanupCategoriesExpected,
  internalCase "different_handler_break_continue" "handler-abrupt-categories"
    breakContinueSource "# different_handler_break_continue" breakContinueExpected,
  internalCase "unhandled_remainder_terminates" "unhandled-remainder"
    unhandledRemainderSource "# unhandled_remainder_terminates"
      unhandledRemainderExpected,
  strictPresentCase,
  strictAbsentCase
]

def sensitivityFirst : HandlerStep :=
  fallthroughStep bothKnown (some bothKnown)

def sensitivityLast : HandlerStep :=
  fallthroughStep sequenceShadowedMappingKnown none

def sensitivitySteps : List HandlerStep :=
  [sensitivityFirst, sensitivityLast]

def sensitivityCorrect : Exits :=
  finishHandlers (routeHandlers (some bothKnown) sensitivitySteps)

def keepFirstSelectedSensitivity : Bool :=
  sensitivityCorrect != cleanSelected sensitivityFirst

def keepLastSelectedSensitivity : Bool :=
  sensitivityCorrect != cleanSelected sensitivityLast

def unreachableSelectedSensitivity : Bool :=
  let stop := fallthroughStep bothKnown none
  let unreachable := categoryStep .break bothKnown none
  let correct := finishHandlers (routeHandlers (some bothKnown) [stop, unreachable])
  let broken := (cleanSelected stop).merge (cleanSelected unreachable)
  correct != broken

def omittedCleanupSensitivity : Bool :=
  let step := categoryStep .terminate bothKnown none (some .source)
  cleanSelected step != step.selected.getD .empty

def flattenedCategorySensitivity : Bool :=
  let step := categoryStep .break bothKnown none
  cleanSelected step != .fallthroughOnly bothKnown

def unhandledRemainderSensitivity : Bool :=
  let route : HandlerRoute :=
    { exits := .fallthroughOnly bothKnown
      remainder := some mappingOnly }
  let correct := finishHandlers route
  let dropped := route.exits
  let duplicated := correct.merge (.categoryOnly .terminate mappingOnly)
  correct != dropped && correct != duplicated

def sensitivityPasses : Bool :=
  keepFirstSelectedSensitivity && keepLastSelectedSensitivity &&
    unreachableSelectedSensitivity && omittedCleanupSensitivity &&
    flattenedCategorySensitivity && unhandledRemainderSensitivity

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "internal-fixture" ||
    mode == "model-only" || mode == "infrastructure-error"

def markerOccursOnce (item : OracleCase) : Bool :=
  !item.marker.isEmpty && (item.source.splitOn item.marker).length == 2

def publicFieldsValid (item : OracleCase) : Bool :=
  match item.candidate with
  | .notObserved =>
      item.candidateCount == 0 && item.candidatePath == none &&
        item.candidateStart == none && item.candidateLength == none &&
        item.candidateOperator == none && item.candidateOriginal == none &&
        item.candidateReplacement == none && item.candidateSymbol == none
  | .absent =>
      item.candidateCount == 0 && item.candidatePath == none &&
        item.candidateStart == none && item.candidateLength == none &&
        item.candidateOperator == none && item.candidateOriginal == none &&
        item.candidateReplacement == none && item.candidateSymbol == none
  | .present =>
      item.candidateCount == 1 && item.candidatePath.isSome &&
        item.candidateStart.isSome && item.candidateLength.isSome &&
        item.candidateOperator.isSome && item.candidateOriginal.isSome &&
        item.candidateReplacement.isSome

def identityValid (item : OracleCase) : Bool :=
  match item.id with
  | "two_handlers_disagree_fallthrough" =>
      item == internalCase "two_handlers_disagree_fallthrough"
        "handler-fallthrough-meet" twoHandlersSource
        "# two_handlers_disagree_fallthrough" twoHandlersExpected
  | "three_handlers_preserve_mapping" =>
      item == internalCase "three_handlers_preserve_mapping"
        "unrelated-fact-preservation" threeHandlersSource
        "# three_handlers_preserve_mapping" threeHandlersExpected
  | "per_handler_cleanup_categories" =>
      item == internalCase "per_handler_cleanup_categories"
        "per-handler-cleanup" cleanupCategoriesSource
        "# per_handler_cleanup_categories" cleanupCategoriesExpected
  | "different_handler_break_continue" =>
      item == internalCase "different_handler_break_continue"
        "handler-abrupt-categories" breakContinueSource
        "# different_handler_break_continue" breakContinueExpected
  | "unhandled_remainder_terminates" =>
      item == internalCase "unhandled_remainder_terminates"
        "unhandled-remainder" unhandledRemainderSource
        "# unhandled_remainder_terminates" unhandledRemainderExpected
  | "all_handlers_preserve_public_candidate" => item == strictPresentCase
  | "one_handler_shadows_public_candidate" => item == strictAbsentCase
  | _ => false

def fixedCasesPass : Bool :=
  multipleHandlerJoinCases.length == 7 &&
    multipleHandlerJoinCases.all fun item =>
      item.schema == 1 && validMode item.mode && markerOccursOnce item &&
        publicFieldsValid item && identityValid item

end HoiminOracle.MultipleHandlerJoin
