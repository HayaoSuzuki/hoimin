import HoiminOracle.ExceptStarFlowProofs

namespace HoiminOracle.ExceptStarFlow

open HoiminOracle.BindingFlow

inductive ObservationKind
  | tryExit
  | publicCandidate
  | modelWitness
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

def commonSource : String :=
  "from typing import Mapping, Sequence\ntry:  # starred_summary_preserves_common\n    work()\nexcept* FirstError:\n    from typing import Sequence\nexcept* SecondError:\n    from typing import Sequence\n"

def disagreementSource : String :=
  "from typing import Mapping, Sequence\ntry:  # starred_summary_meets_disagreement\n    work()\nexcept* FirstError:\n    from typing import Sequence\nexcept* SecondError:\n    Sequence = object\n"

def cleanupSource : String :=
  "def run():\n    from typing import Mapping, Sequence\n    try:  # starred_target_cleanup\n        work()\n    except* FirstError as Sequence:\n        from typing import Sequence\n    except* SecondError as Sequence:\n        from typing import Sequence\n"

def remainderSource : String :=
  "def run():\n    from typing import Mapping, Sequence\n    try:  # starred_unhandled_remainder\n        raise UnknownError\n    except* FirstError:\n        pass\n    except* SecondError:\n        pass\n"

def publicPresentSource : String :=
  "from typing import Sequence\ntry:\n    work()\nexcept* FirstError:\n    from typing import Sequence\nexcept* SecondError:\n    from typing import Sequence\nstarred_present: Sequence[int]\n"

def publicAbsentSource : String :=
  "from typing import Sequence\ntry:\n    work()\nexcept* FirstError:\n    from typing import Sequence\nexcept* SecondError:\n    Sequence = object\nstarred_absent: Sequence[int]\n"

def internalCase
    (id family source marker : String)
    (expected : Exits) : OracleCase where
  id
  mode := "internal-fixture"
  observationKind := .tryExit
  family
  source
  marker
  expectedExits := expected

def modelCase (id family : String) (expected : Exits) : OracleCase where
  id
  mode := "model-only"
  observationKind := .modelWitness
  family
  source := ""
  marker := ""
  expectedExits := expected

def strictPresentCase : OracleCase where
  id := "starred_public_candidate_present"
  mode := "strict"
  observationKind := .publicCandidate
  family := "collapsed-summary"
  source := publicPresentSource
  marker := "Sequence[int]"
  candidate := .present
  candidateCount := 1
  candidatePath := some "target.py"
  candidateStart := some 166
  candidateLength := some 13
  candidateOperator := some "type_list_sequence"
  candidateOriginal := some "Sequence[int]"
  candidateReplacement := some "list[int]"

def strictAbsentCase : OracleCase where
  id := "starred_public_candidate_absent"
  mode := "strict"
  observationKind := .publicCandidate
  family := "collapsed-summary"
  source := publicAbsentSource
  marker := "Sequence[int]"
  candidate := .absent

def twoSiblingRoute : StarRoute :=
  routeStarHandlers
    { env := bothKnown, activeRemainder := true }
    [
      { split := { matched := true, remainder := true }
        action := { source := .invalidate } },
      { split := { matched := true, remainder := false }
        action := { source := .restoreTyping } }
    ]

def raisedSiblingRoute : StarRoute :=
  routeStarHandlers
    { env := bothKnown, activeRemainder := true }
    [
      { split := { matched := true, remainder := true }
        action := { source := .invalidate }
        raises := true },
      { split := { matched := true, remainder := false }
        action := { source := .restoreTyping } }
    ]

def unhandledExpected : Exits where
  fallthrough := some bothKnown
  terminates := [bothKnown]

def exceptStarFlowCases : List OracleCase := [
  internalCase "starred_summary_preserves_common" "collapsed-summary"
    commonSource "# starred_summary_preserves_common"
    (.fallthroughOnly bothKnown),
  internalCase "starred_summary_meets_disagreement" "collapsed-summary"
    disagreementSource "# starred_summary_meets_disagreement"
    (.fallthroughOnly mappingOnly),
  internalCase "starred_target_cleanup" "target-cleanup"
    cleanupSource "# starred_target_cleanup"
    (.fallthroughOnly mappingOnly),
  internalCase "starred_unhandled_remainder" "unhandled-remainder"
    remainderSource "# starred_unhandled_remainder"
    unhandledExpected,
  modelCase "two_matching_siblings_exact_route" "sibling-order"
    (finishStarRoute twoSiblingRoute),
  modelCase "raised_handler_allows_later_sibling" "delayed-raise"
    (finishStarRoute raisedSiblingRoute),
  strictPresentCase,
  strictAbsentCase
]

def stopAfterFirstMatchSensitivity : Bool :=
  twoSiblingRoute.visits !=
    (routeStarHandler
      { env := bothKnown, activeRemainder := true }
      { split := { matched := true, remainder := true }
        action := { source := .invalidate } }).visits

def dropSiblingAfterRaiseSensitivity : Bool :=
  raisedSiblingRoute.env !=
    (routeStarHandler
      { env := bothKnown, activeRemainder := true }
      { split := { matched := true, remainder := true }
        action := { source := .invalidate }
        raises := true }).env

def lostRemainderSensitivity : Bool :=
  let correct : StarRoute :=
    { env := bothKnown, activeRemainder := true }
  finishStarRoute correct !=
    finishStarRoute { correct with activeRemainder := false }

def duplicatedRemainderSensitivity : Bool :=
  let correct := finishStarRoute
    { env := bothKnown, activeRemainder := true }
  correct != correct.merge correct

def eagerRaisePropagationSensitivity : Bool :=
  let afterFirst := routeStarHandler
    { env := bothKnown, activeRemainder := true }
    { split := { matched := true, remainder := true }
      action := { source := .keep }
      raises := true }
  let afterSibling := routeStarHandler afterFirst
    { split := { matched := true, remainder := false }
      action := { source := .invalidate } }
  finishStarRoute afterFirst != finishStarRoute afterSibling

def omittedTargetCleanupSensitivity : Bool :=
  let handler : StarHandler :=
    { split := { matched := true, remainder := false }
      action := { source := .restoreTyping }
      target := some .source }
  let correct := routeStarHandler
    { env := bothKnown, activeRemainder := true } handler
  correct.env != handler.action.apply bothKnown

def exclusiveHandlerCollapseSensitivity : Bool :=
  conservativeSummary bothKnown
      { source := .keep }
      { source := .invalidate } !=
    (EnvAction.apply { source := .keep } bothKnown)

def sensitivityPasses : Bool :=
  stopAfterFirstMatchSensitivity &&
    dropSiblingAfterRaiseSensitivity &&
    lostRemainderSensitivity &&
    duplicatedRemainderSensitivity &&
    eagerRaisePropagationSensitivity &&
    omittedTargetCleanupSensitivity &&
    exclusiveHandlerCollapseSensitivity

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "internal-fixture" ||
    mode == "model-only" || mode == "infrastructure-error"

def markerValid (item : OracleCase) : Bool :=
  if item.mode == "model-only" then
    item.source.isEmpty && item.marker.isEmpty
  else
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
  | "starred_summary_preserves_common" =>
      item == internalCase "starred_summary_preserves_common"
        "collapsed-summary" commonSource "# starred_summary_preserves_common"
        (.fallthroughOnly bothKnown)
  | "starred_summary_meets_disagreement" =>
      item == internalCase "starred_summary_meets_disagreement"
        "collapsed-summary" disagreementSource
        "# starred_summary_meets_disagreement" (.fallthroughOnly mappingOnly)
  | "starred_target_cleanup" =>
      item == internalCase "starred_target_cleanup" "target-cleanup"
        cleanupSource "# starred_target_cleanup" (.fallthroughOnly mappingOnly)
  | "starred_unhandled_remainder" =>
      item == internalCase "starred_unhandled_remainder" "unhandled-remainder"
        remainderSource "# starred_unhandled_remainder" unhandledExpected
  | "two_matching_siblings_exact_route" =>
      item == modelCase "two_matching_siblings_exact_route" "sibling-order"
        (finishStarRoute twoSiblingRoute)
  | "raised_handler_allows_later_sibling" =>
      item == modelCase "raised_handler_allows_later_sibling" "delayed-raise"
        (finishStarRoute raisedSiblingRoute)
  | "starred_public_candidate_present" => item == strictPresentCase
  | "starred_public_candidate_absent" => item == strictAbsentCase
  | _ => false

def caseValid (item : OracleCase) : Bool :=
  item.schema == 1 && validMode item.mode && markerValid item &&
    publicFieldsValid item && identityValid item &&
    match item.mode, item.observationKind with
    | "strict", .publicCandidate => true
    | "internal-fixture", .tryExit => true
    | "model-only", .modelWitness => true
    | _, _ => false

def fixedCasesPass : Bool :=
  exceptStarFlowCases.length == 8 &&
    (exceptStarFlowCases.map OracleCase.id).eraseDups.length == 8 &&
    exceptStarFlowCases.all caseValid

end HoiminOracle.ExceptStarFlow
