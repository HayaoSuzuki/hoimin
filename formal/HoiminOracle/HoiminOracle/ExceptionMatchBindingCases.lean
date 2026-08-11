import HoiminOracle.ExceptionMatchBindingProofs

namespace HoiminOracle.ExceptionMatchBinding

open BindingFlow

inductive Family
  | handler
  | matchCase
  deriving Repr, DecidableEq, BEq

inductive ObservationKind
  | resolution
  | annotation
  | exits
  | publicCandidate
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  family : Family
  observationKind : ObservationKind
  source : String
  marker : String := ""
  name : String := "Sequence"
  expectedFacts : List String := []
  expectedResolution : Option String := none
  expectedExitCategory : Option String := none
  expectedPresent : Bool := false
  operator : Option String := none
  original : Option String := none
  replacement : Option String := none
  symbol : Option String := none
  deriving Repr, DecidableEq, BEq

def Family.label : Family → String
  | .handler => "handler"
  | .matchCase => "match-case"

def ObservationKind.label : ObservationKind → String
  | .resolution => "resolution"
  | .annotation => "annotation"
  | .exits => "exits"
  | .publicCandidate => "public-candidate"

def sequenceFact : String := "direct:Sequence=typing.Sequence"
def mappingFact : String := "direct:Mapping=typing.Mapping"

def normalizedFacts (environment : Env) : List String :=
  (if environment.destination == .known .typing then [sequenceFact] else []) ++
    (if environment.source == .known .typing then [mappingFact] else [])

def factResolutionLabel : Fact → String
  | .known .builtin => "definitely-builtin"
  | .known .typing => "typing-import"
  | .shadowed => "shadowed"
  | .absent | .unknown => "unknown"

def resolutionAtDestination (environment : Option Env) : Option String :=
  environment.map fun value => factResolutionLabel value.destination

def sequenceEnv : Env where
  source := .known .builtin
  destination := .known .typing

def sequenceAndMappingEnv : Env where
  source := .known .typing
  destination := .known .typing

def factsAtFallthrough (exits : Exits) : List String :=
  exits.fallthrough.map normalizedFacts |>.getD []

def factsAtFirst (states : List Env) : List String :=
  states.head?.map normalizedFacts |>.getD []

def handlerObservation : HandlerObservation :=
  observeHandler sequenceEnv .destination Exits.fallthroughOnly

def handlerFallthroughFacts : List String :=
  factsAtFallthrough
    (cleanupHandlerExits .destination
      (.categoryOnly .fallthrough sequenceEnv))

def handlerTerminateFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .terminate sequenceEnv)).terminates

def handlerBreakFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .break sequenceEnv)).breaks

def handlerContinueFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .continue sequenceEnv)).continues

def handlerJoinState : Option Env :=
  meetOption (some sequenceEnv) handlerObservation.exits.fallthrough

def handlerJoinResolution : Option String :=
  resolutionAtDestination handlerJoinState

def handlerJoinFacts : List String :=
  handlerJoinState.map normalizedFacts |>.getD []

def handlerPreservedFacts : List String :=
  normalizedFacts (deleteName sequenceAndMappingEnv .destination)

def capturedPattern : PatternResult where
  matched := bindTarget sequenceEnv .destination
  failed := some (bindTarget sequenceEnv .destination)

def guardOnlyPattern : PatternResult where
  matched := sequenceEnv
  failed := none

def matchCaptureFacts : List String :=
  (advanceCase capturedPattern none).body.map normalizedFacts |>.getD []

def matchPartialFailureResolution : Option String :=
  resolutionAtDestination (advanceCase capturedPattern none).nextCase

def observableEntryFacts (entry : Option Env) : Option (List String) :=
  entry.map normalizedFacts

def matchPartialFailureObservableFacts : Option (List String) :=
  observableEntryFacts (advanceCase capturedPattern none).nextCase

def matchFalseGuardStep : CaseStep :=
  advanceCase guardOnlyPattern (some false)
    (bindTarget sequenceEnv .destination)

def matchFalseGuardObservableFacts : Option (List String) :=
  observableEntryFacts matchFalseGuardStep.nextCase

def matchRefutableIncoming : Env :=
  bindTarget sequenceEnv .destination

def matchRefutableCompleted : Env :=
  sequenceEnv

def matchRefutableJoinFacts : List String :=
  (finishMatch (some matchRefutableIncoming) [matchRefutableCompleted]).map
      normalizedFacts |>.getD []

def matchIrrefutableIncoming : Env :=
  bindTarget sequenceEnv .destination

def matchIrrefutableStep : CaseStep :=
  advanceCase { matched := sequenceEnv, failed := none } none

def matchIrrefutableFacts : List String :=
  (finishMatch matchIrrefutableStep.nextCase matchIrrefutableStep.body.toList).map
      normalizedFacts |>.getD []

def matchPreservedFacts : List String :=
  let partiallyBound := bindTarget sequenceAndMappingEnv .destination
  (finishMatch (some partiallyBound) [partiallyBound]).map normalizedFacts |>.getD []

def handlerTypeSource : String :=
  "from typing import Sequence\nclass Error(Exception):\n    pass\ndef risky():\n    raise Error()\ntry:\n    risky()\nexcept Error as Sequence:\n    handler_body_marker: list[str]\n"

def handlerFallthroughSource : String :=
  "from typing import Sequence\ntry:\n    risky()\nexcept Error as Sequence:\n    from typing import Sequence\n    pass  # handler-fallthrough-exit\nafter_fallthrough: list[str]\n"

def handlerReturnSource : String :=
  "def run():\n    from typing import Sequence\n    try:\n        risky()\n    except Error as Sequence:\n        from typing import Sequence\n        return 1  # handler-return-exit\n"

def handlerRaiseSource : String :=
  "def run():\n    from typing import Sequence\n    try:\n        risky()\n    except Error as Sequence:\n        from typing import Sequence\n        raise Error  # handler-raise-exit\n"

def handlerBreakSource : String :=
  "from typing import Sequence\nwhile active:\n    try:\n        risky()\n    except Error as Sequence:\n        from typing import Sequence\n        break  # handler-break-exit\n"

def handlerContinueSource : String :=
  "from typing import Sequence\nwhile active:\n    try:\n        risky()\n    except Error as Sequence:\n        from typing import Sequence\n        continue  # handler-continue-exit\n"

def handlerJoinSource : String :=
  "from typing import Sequence\ntry:\n    risky()\nexcept Error as Sequence:\n    pass\nexcept OtherError:\n    pass\nafter_nonselected: list[str]\n"

def matchPartialFailureObservableSource : String :=
  "from typing import Sequence\nmatch value:\n    case [Sequence, 0]:\n        pass\n    case _:\n        pattern_failure_observed: list[str]\n        from typing import Sequence\n"

def handlerPreservedSource : String :=
  "from typing import Sequence, Mapping\ntry:\n    risky()\nexcept Error as Sequence:\n    pass\npreserved_handler_import: tuple[Mapping]\n"

def matchCaptureSource : String :=
  "from typing import Sequence\nmatch value:\n    case [Sequence, 0]:\n        captured_body: list[str]\n    case _:\n        pass\n"

def matchPartialFailureSource : String :=
  "from typing import Sequence\nmatch value:\n    case [Sequence, 0]:\n        pass\n    case _:\n        partial_failure_next: list[str]\n"

def matchFalseGuardSource : String :=
  "from typing import Sequence\nmatch value:\n    case _ if ((Sequence := local_sequence) and False):\n        pass\n    case _:\n        false_guard_observed: list[str]\n        from typing import Sequence\n"

def matchRefutableSource : String :=
  "from typing import Sequence\nSequence = local_sequence\nmatch value:\n    case 0:\n        from typing import Sequence\nafter_refutable_match: list[str]\n"

def matchIrrefutableSource : String :=
  "from typing import Sequence\nSequence = local_sequence\nmatch value:\n    case _:\n        from typing import Sequence\nafter_irrefutable_match: list[str]\n"

def matchPreservedSource : String :=
  "from typing import Sequence, Mapping\nmatch value:\n    case [Sequence, 0]:\n        pass\n    case _:\n        pass\npreserved_match_import: tuple[Mapping]\n"

def annotationCase (id : String) (family : Family) (source marker : String)
    (facts : List String) (name : String := "Sequence") : OracleCase where
  id
  mode := "internal-fixture"
  family
  observationKind := .annotation
  source
  marker
  name
  expectedFacts := facts
  expectedPresent := !facts.isEmpty

def modelResolutionCase (id : String) (family : Family) (source marker : String)
    (resolution : Option String) : OracleCase where
  id
  mode := "model-only"
  family
  observationKind := .resolution
  source
  marker
  expectedResolution := resolution
  expectedPresent := resolution == some "typing-import"

def exitCase (id source marker category : String)
    (facts : List String) : OracleCase where
  id
  mode := "internal-fixture"
  family := .handler
  observationKind := .exits
  source
  marker
  expectedFacts := facts
  expectedExitCategory := some category
  expectedPresent := !facts.isEmpty

def publicCaseFromInternal (id marker : String)
    (premise : OracleCase) : OracleCase where
  id
  mode := "strict"
  family := premise.family
  observationKind := .publicCandidate
  source := premise.source
  marker
  name := premise.name
  expectedPresent := premise.expectedPresent
  operator := some "type_list_sequence"
  original := some "list[str]"
  replacement := some "Sequence[str]"

def handlerBodyCase : OracleCase :=
  annotationCase "handler_body_after_target" .handler handlerTypeSource
    "list[str]" (normalizedFacts handlerObservation.bodyEntry)

def handlerFallthroughCase : OracleCase :=
  exitCase "handler_cleanup_fallthrough" handlerFallthroughSource
    "# handler-fallthrough-exit" "fallthrough" handlerFallthroughFacts

def handlerJoinModelCase : OracleCase :=
  modelResolutionCase "handler_nonselected_join" .handler handlerJoinSource
    "list[str]" handlerJoinResolution

def handlerJoinObservableCase : OracleCase :=
  annotationCase "handler_nonselected_join_observable" .handler handlerJoinSource
    "list[str]" handlerJoinFacts

def matchCaptureCase : OracleCase :=
  annotationCase "match_capture_body" .matchCase matchCaptureSource
    "list[str]" matchCaptureFacts

def matchPartialFailureModelCase : OracleCase :=
  modelResolutionCase "match_partial_failure_next_case" .matchCase
    matchPartialFailureSource "list[str]" matchPartialFailureResolution

def matchPartialFailureObservableCase : OracleCase :=
  annotationCase "match_partial_failure_observable" .matchCase
    matchPartialFailureObservableSource "list[str]"
      (matchPartialFailureObservableFacts.getD [])

def matchFalseGuardCase : OracleCase :=
  annotationCase "match_false_guard_next_case" .matchCase
    matchFalseGuardSource "list[str]" (matchFalseGuardObservableFacts.getD [])

def matchRefutableCase : OracleCase :=
  annotationCase "match_refutable_unmatched_join" .matchCase
    matchRefutableSource "list[str]" matchRefutableJoinFacts

def matchIrrefutableCase : OracleCase :=
  annotationCase "match_irrefutable_exhaustion" .matchCase
    matchIrrefutableSource "list[str]" matchIrrefutableFacts

def internalCases : List OracleCase := [
  annotationCase "handler_type_before_target" .handler handlerTypeSource
    "except Error as"
    (normalizedFacts handlerObservation.typeEntry),
  handlerBodyCase,
  handlerFallthroughCase,
  exitCase "handler_cleanup_return" handlerReturnSource
    "# handler-return-exit" "terminate" handlerTerminateFacts,
  exitCase "handler_cleanup_raise" handlerRaiseSource
    "# handler-raise-exit" "terminate" handlerTerminateFacts,
  exitCase "handler_cleanup_break" handlerBreakSource
    "# handler-break-exit" "break" handlerBreakFacts,
  exitCase "handler_cleanup_continue" handlerContinueSource
    "# handler-continue-exit" "continue" handlerContinueFacts,
  handlerJoinObservableCase,
  annotationCase "handler_preserves_unrelated_import" .handler
    handlerPreservedSource "tuple[Mapping]" handlerPreservedFacts "Mapping",
  matchCaptureCase,
  matchPartialFailureObservableCase,
  matchFalseGuardCase,
  matchRefutableCase,
  matchIrrefutableCase,
  annotationCase "match_preserves_unrelated_import" .matchCase
    matchPreservedSource "tuple[Mapping]" matchPreservedFacts "Mapping"
]

def modelCases : List OracleCase := [
  handlerJoinModelCase,
  matchPartialFailureModelCase
]

def strictPairs : List (OracleCase × OracleCase) := [
  (handlerBodyCase,
    publicCaseFromInternal "handler_body_after_target_public" "list[str]"
      handlerBodyCase),
  (handlerFallthroughCase,
    publicCaseFromInternal "handler_cleanup_fallthrough_public" "list[str]"
      handlerFallthroughCase),
  (handlerJoinObservableCase,
    publicCaseFromInternal "handler_nonselected_join_public" "list[str]"
      handlerJoinObservableCase),
  (matchCaptureCase,
    publicCaseFromInternal "match_capture_body_public" "list[str]"
      matchCaptureCase),
  (matchPartialFailureObservableCase,
    publicCaseFromInternal "match_partial_failure_next_case_public" "list[str]"
      matchPartialFailureObservableCase),
  (matchFalseGuardCase,
    publicCaseFromInternal "match_false_guard_next_case_public" "list[str]"
      matchFalseGuardCase),
  (matchRefutableCase,
    publicCaseFromInternal "match_refutable_unmatched_join_public" "list[str]"
      matchRefutableCase),
  (matchIrrefutableCase,
    publicCaseFromInternal "match_irrefutable_exhaustion_public" "list[str]"
      matchIrrefutableCase)
]

def strictCases : List OracleCase := strictPairs.map Prod.snd

def cases : List OracleCase := internalCases ++ modelCases ++ strictCases

def allowedMode (mode : String) : Bool :=
  ["internal-fixture", "strict", "model-only", "infrastructure-error"].contains mode

def allowedResolution (resolution : String) : Bool :=
  ["definitely-builtin", "typing-import", "shadowed", "unknown"].contains resolution

def allowedExitCategory (category : String) : Bool :=
  ["fallthrough", "break", "continue", "terminate"].contains category

def allowedFact (fact : String) : Bool :=
  fact == sequenceFact || fact == mappingFact

def factsSorted : List String → Bool
  | [] | [_] => true
  | left :: right :: rest => decide (left ≤ right) && factsSorted (right :: rest)

def markerOccursOnce (item : OracleCase) : Bool :=
  !item.marker.isEmpty && (item.source.splitOn item.marker).length == 2

def familyKindValid (family : Family) (kind : ObservationKind) : Bool :=
  match family, kind with
  | .handler, _ => true
  | .matchCase, .exits => false
  | .matchCase, _ => true

def modeKindValid (mode : String) (kind : ObservationKind) : Bool :=
  match kind with
  | .publicCandidate => mode == "strict"
  | .resolution => mode == "model-only"
  | _ => mode == "internal-fixture" || mode == "model-only" ||
      mode == "infrastructure-error"

def candidateFieldsAbsent (item : OracleCase) : Bool :=
  item.operator.isNone && item.original.isNone && item.replacement.isNone &&
    item.symbol.isNone

def fieldsMatchKind (item : OracleCase) : Bool :=
  match item.observationKind with
  | .annotation =>
      item.expectedResolution.isNone && item.expectedExitCategory.isNone &&
        candidateFieldsAbsent item &&
        item.expectedPresent == !item.expectedFacts.isEmpty
  | .resolution =>
      item.expectedFacts.isEmpty && item.expectedResolution.any allowedResolution &&
        item.expectedExitCategory.isNone && candidateFieldsAbsent item &&
        item.expectedPresent == (item.expectedResolution == some "typing-import")
  | .exits =>
      item.expectedResolution.isNone &&
        item.expectedExitCategory.any allowedExitCategory && candidateFieldsAbsent item &&
        item.expectedPresent == !item.expectedFacts.isEmpty
  | .publicCandidate =>
      item.expectedFacts.isEmpty && item.expectedResolution.isNone &&
        item.expectedExitCategory.isNone && item.operator.isSome &&
        item.original.isSome && item.replacement.isSome

def fixedExitProjectionValid (item : OracleCase) (source marker category : String)
    (facts : List String) : Bool :=
  item.source == source && item.marker == marker &&
    item.expectedExitCategory == some category && item.expectedFacts == facts

def fixedProjectionValid (item : OracleCase) : Bool :=
  match item.id with
  | "handler_type_before_target" =>
      item.expectedFacts == normalizedFacts handlerObservation.typeEntry
  | "handler_cleanup_fallthrough" =>
      fixedExitProjectionValid item handlerFallthroughSource
        "# handler-fallthrough-exit" "fallthrough" handlerFallthroughFacts
  | "handler_cleanup_return" =>
      fixedExitProjectionValid item handlerReturnSource
        "# handler-return-exit" "terminate" handlerTerminateFacts
  | "handler_cleanup_raise" =>
      fixedExitProjectionValid item handlerRaiseSource
        "# handler-raise-exit" "terminate" handlerTerminateFacts
  | "handler_cleanup_break" =>
      fixedExitProjectionValid item handlerBreakSource
        "# handler-break-exit" "break" handlerBreakFacts
  | "handler_cleanup_continue" =>
      fixedExitProjectionValid item handlerContinueSource
        "# handler-continue-exit" "continue" handlerContinueFacts
  | "handler_nonselected_join" =>
      item.expectedResolution == handlerJoinResolution
  | "handler_nonselected_join_observable" =>
      item.expectedFacts == handlerJoinFacts
  | "handler_preserves_unrelated_import" =>
      item.source == handlerPreservedSource && item.marker == "tuple[Mapping]" &&
        item.name == "Mapping" && item.expectedFacts == handlerPreservedFacts
  | "match_partial_failure_next_case" =>
      item.expectedResolution == matchPartialFailureResolution
  | "match_partial_failure_observable" =>
      item.source == matchPartialFailureObservableSource && item.marker == "list[str]" &&
        item.expectedFacts == matchPartialFailureObservableFacts.getD []
  | "match_false_guard_next_case" =>
      item.source == matchFalseGuardSource && item.marker == "list[str]" &&
        item.expectedFacts == matchFalseGuardObservableFacts.getD []
  | "match_refutable_unmatched_join" =>
      item.source == matchRefutableSource && item.marker == "list[str]" &&
        item.expectedFacts == matchRefutableJoinFacts
  | "match_irrefutable_exhaustion" =>
      item.expectedFacts == matchIrrefutableFacts
  | "match_preserves_unrelated_import" =>
      item.source == matchPreservedSource && item.marker == "tuple[Mapping]" &&
        item.name == "Mapping" && item.expectedFacts == matchPreservedFacts
  | _ => true

def strictExpectationMatches (premise candidate : OracleCase) : Bool :=
  premise.mode == "internal-fixture" && candidate.mode == "strict" &&
    candidate.family == premise.family && candidate.source == premise.source &&
    candidate.name == premise.name &&
    candidate.expectedPresent == premise.expectedPresent

def strictPairValid (allCases : List OracleCase) (item : OracleCase) : Bool :=
  match item.observationKind with
  | .publicCandidate =>
      match strictPairs.find? fun pair => pair.2.id == item.id with
      | none => false
      | some pair =>
          item == pair.2 && allCases.contains pair.1 &&
            strictExpectationMatches pair.1 item
  | _ => true

def OracleCase.valid (allCases : List OracleCase) (item : OracleCase) : Bool :=
  item.schema == 1 && !item.id.isEmpty && !item.source.isEmpty &&
    allowedMode item.mode && markerOccursOnce item &&
    (allCases.map OracleCase.id).count item.id == 1 &&
    factsSorted item.expectedFacts && item.expectedFacts.all allowedFact &&
    item.expectedFacts.eraseDups.length == item.expectedFacts.length &&
    familyKindValid item.family item.observationKind &&
    modeKindValid item.mode item.observationKind && fieldsMatchKind item &&
    fixedProjectionValid item && strictPairValid allCases item

def fixedCasesPass : Bool :=
  cases.length == 25 && internalCases.length == 15 && modelCases.length == 2 &&
    strictPairs.length == 8 &&
    cases.all (OracleCase.valid cases)

private def brokenBindBeforeType : HandlerObservation :=
  { handlerObservation with
      typeEntry := bindTarget sequenceEnv .destination }

private def brokenCleanupLeavesFallthrough : Exits :=
  .categoryOnly .fallthrough sequenceEnv

private def brokenCleanupLeavesBreak : Exits :=
  .categoryOnly .break sequenceEnv

private def brokenCleanupLeavesContinue : Exits :=
  .categoryOnly .continue sequenceEnv

private def brokenCleanupLeavesTerminate : Exits :=
  .categoryOnly .terminate sequenceEnv

private def brokenJoinUsesLeft : Option Env :=
  some sequenceEnv

private def brokenUsePrePatternFailure : CaseStep :=
  { body := some capturedPattern.matched, nextCase := some sequenceEnv }

private def brokenUsePreGuardFailure : CaseStep :=
  { body := none, nextCase := some sequenceEnv }

private def brokenDiscardRefutableUnmatched : Option Env :=
  finishMatch none [matchRefutableCompleted]

private def brokenRetainIrrefutableUnmatched : Option Env :=
  some matchIrrefutableIncoming

def handlerJoinBrokenResolution : Option String :=
  resolutionAtDestination brokenJoinUsesLeft

def handlerJoinBrokenFacts : List String :=
  brokenJoinUsesLeft.map normalizedFacts |>.getD []

def patternFailureBrokenResolution : Option String :=
  resolutionAtDestination brokenUsePrePatternFailure.nextCase

def patternFailureBrokenObservableFacts : Option (List String) :=
  observableEntryFacts brokenUsePrePatternFailure.nextCase

def guardFailureBrokenObservableFacts : Option (List String) :=
  observableEntryFacts brokenUsePreGuardFailure.nextCase

def refutableUnmatchedBrokenFacts : List String :=
  brokenDiscardRefutableUnmatched.map normalizedFacts |>.getD []

def irrefutableBrokenFacts : List String :=
  (finishMatch brokenRetainIrrefutableUnmatched
      matchIrrefutableStep.body.toList).map normalizedFacts |>.getD []

def bindBeforeTypeSensitivity : Bool :=
  normalizedFacts brokenBindBeforeType.typeEntry !=
    normalizedFacts handlerObservation.typeEntry

def handlerFallthroughCleanupSensitivity : Bool :=
  factsAtFallthrough brokenCleanupLeavesFallthrough != handlerFallthroughFacts

def handlerBreakCleanupSensitivity : Bool :=
  factsAtFirst brokenCleanupLeavesBreak.breaks != handlerBreakFacts

def handlerContinueCleanupSensitivity : Bool :=
  factsAtFirst brokenCleanupLeavesContinue.continues != handlerContinueFacts

def handlerTerminateCleanupSensitivity : Bool :=
  factsAtFirst brokenCleanupLeavesTerminate.terminates != handlerTerminateFacts

def handlerExitCleanupSensitivity : Bool :=
  handlerFallthroughCleanupSensitivity && handlerBreakCleanupSensitivity &&
    handlerContinueCleanupSensitivity && handlerTerminateCleanupSensitivity

def handlerJoinMeetSensitivity : Bool :=
  handlerJoinBrokenResolution != handlerJoinResolution &&
    handlerJoinBrokenFacts != handlerJoinFacts

def patternFailureSensitivity : Bool :=
  patternFailureBrokenResolution != matchPartialFailureResolution &&
    patternFailureBrokenObservableFacts != matchPartialFailureObservableFacts

def guardFailureSensitivity : Bool :=
  guardFailureBrokenObservableFacts != matchFalseGuardObservableFacts

def refutableUnmatchedSensitivity : Bool :=
  let strict := publicCaseFromInternal
    "match_refutable_unmatched_join_public" "list[str]" matchRefutableCase
  matchRefutableCase.expectedFacts == matchRefutableJoinFacts &&
    matchRefutableCase.expectedFacts != refutableUnmatchedBrokenFacts &&
    matchRefutableCase.expectedPresent != !refutableUnmatchedBrokenFacts.isEmpty &&
    strict.expectedPresent == matchRefutableCase.expectedPresent &&
    strict.expectedPresent != !refutableUnmatchedBrokenFacts.isEmpty

def irrefutableExhaustionSensitivity : Bool :=
  irrefutableBrokenFacts != matchIrrefutableFacts

def sensitivityPasses : Bool :=
  bindBeforeTypeSensitivity && handlerExitCleanupSensitivity &&
    handlerJoinMeetSensitivity && patternFailureSensitivity &&
    guardFailureSensitivity && refutableUnmatchedSensitivity &&
    irrefutableExhaustionSensitivity

end HoiminOracle.ExceptionMatchBinding
