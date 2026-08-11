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
def typeAliasFact : String := "direct:TypeAlias=typing.TypeAlias"

def normalizedFacts (environment : Env) : List String :=
  (if environment.destination == .known .typing then [sequenceFact] else []) ++
    (if environment.source == .known .typing then [typeAliasFact] else [])

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

def sequenceAndTypeAliasEnv : Env where
  source := .known .typing
  destination := .known .typing

def factsAtFallthrough (exits : Exits) : List String :=
  exits.fallthrough.map normalizedFacts |>.getD []

def factsAtFirst (states : List Env) : List String :=
  states.head?.map normalizedFacts |>.getD []

def handlerObservation : HandlerObservation :=
  observeHandler sequenceEnv .destination Exits.fallthroughOnly

def handlerFallthroughFacts : List String :=
  factsAtFallthrough handlerObservation.exits

def handlerTerminateFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .terminate (bindTarget sequenceEnv .destination))).terminates

def handlerBreakFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .break (bindTarget sequenceEnv .destination))).breaks

def handlerContinueFacts : List String :=
  factsAtFirst
    (cleanupHandlerExits .destination
      (.categoryOnly .continue (bindTarget sequenceEnv .destination))).continues

def handlerJoinResolution : Option String :=
  resolutionAtDestination
    (meetOption handlerObservation.exits.fallthrough (some sequenceEnv))

def handlerPreservedFacts : List String :=
  normalizedFacts (deleteName sequenceAndTypeAliasEnv .destination)

def capturedPattern : PatternResult where
  matched := bindTarget sequenceEnv .destination
  failed := some (bindTarget sequenceEnv .destination)

def guardedPattern : PatternResult where
  matched := sequenceEnv
  failed := some sequenceEnv

def matchCaptureFacts : List String :=
  (advanceCase capturedPattern none).body.map normalizedFacts |>.getD []

def matchPartialFailureResolution : Option String :=
  resolutionAtDestination (advanceCase capturedPattern none).nextCase

def matchFalseGuardFacts : List String :=
  (advanceCase guardedPattern (some false)
      (bindTarget sequenceEnv .destination)).nextCase.map normalizedFacts |>.getD []

def matchRefutableJoinFacts : List String :=
  (finishMatch (some sequenceEnv) [bindTarget sequenceEnv .destination]).map
      normalizedFacts |>.getD []

def matchIrrefutableIncoming : Env :=
  bindTarget sequenceEnv .destination

def matchIrrefutableStep : CaseStep :=
  advanceCase { matched := sequenceEnv, failed := none } none

def matchIrrefutableFacts : List String :=
  (finishMatch matchIrrefutableStep.nextCase matchIrrefutableStep.body.toList).map
      normalizedFacts |>.getD []

def matchPreservedFacts : List String :=
  let partiallyBound := bindTarget sequenceAndTypeAliasEnv .destination
  (finishMatch (some partiallyBound) [partiallyBound]).map normalizedFacts |>.getD []

def handlerTypeSource : String :=
  "from typing import Sequence\nclass Error(Exception):\n    pass\ndef risky():\n    raise Error()\ntry:\n    risky()\nexcept Error as Sequence:\n    handler_body_marker: list[str]\n"

def handlerFallthroughSource : String :=
  "from typing import Sequence\ntry:\n    risky()\nexcept Error as Sequence:\n    pass  # handler-fallthrough-exit\nafter_fallthrough: list[str]\n"

def handlerReturnSource : String :=
  "def run():\n    from typing import Sequence\n    try:\n        risky()\n    except Error as Sequence:\n        return 1  # handler-return-exit\n"

def handlerRaiseSource : String :=
  "def run():\n    from typing import Sequence\n    try:\n        risky()\n    except Error as Sequence:\n        raise Error  # handler-raise-exit\n"

def handlerBreakSource : String :=
  "from typing import Sequence\nwhile active:\n    try:\n        risky()\n    except Error as Sequence:\n        break  # handler-break-exit\n"

def handlerContinueSource : String :=
  "from typing import Sequence\nwhile active:\n    try:\n        risky()\n    except Error as Sequence:\n        continue  # handler-continue-exit\n"

def handlerJoinSource : String :=
  "from typing import Sequence\ntry:\n    risky()\nexcept Error as Sequence:\n    pass\nexcept OtherError:\n    pass\nafter_nonselected: list[str]\n"

def handlerPreservedSource : String :=
  "from typing import Sequence, TypeAlias\ntry:\n    risky()\nexcept Error as Sequence:\n    pass\npreserved_handler_import: tuple[TypeAlias]\n"

def matchCaptureSource : String :=
  "from typing import Sequence\nmatch value:\n    case [Sequence, 0]:\n        captured_body: list[str]\n    case _:\n        pass\n"

def matchPartialFailureSource : String :=
  "from typing import Sequence\nmatch value:\n    case [Sequence, 0]:\n        pass\n    case _:\n        partial_failure_next: list[str]\n"

def matchFalseGuardSource : String :=
  "from typing import Sequence\nmatch value:\n    case 0 if ((Sequence := local_sequence) and False):\n        pass\n    case _:\n        false_guard_next: list[str]\n"

def matchRefutableSource : String :=
  "from typing import Sequence\nmatch value:\n    case 0:\n        Sequence = local_sequence\nafter_refutable_match: list[str]\n"

def matchIrrefutableSource : String :=
  "from typing import Sequence\nSequence = local_sequence\nmatch value:\n    case _:\n        from typing import Sequence\nafter_irrefutable_match: list[str]\n"

def matchPreservedSource : String :=
  "from typing import Sequence, TypeAlias\nmatch value:\n    case [Sequence, 0]:\n        pass\n    case _:\n        pass\npreserved_match_import: tuple[TypeAlias]\n"

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

def resolutionCase (id : String) (family : Family) (source marker : String)
    (resolution : Option String) : OracleCase where
  id
  mode := "internal-fixture"
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

def handlerJoinCase : OracleCase :=
  resolutionCase "handler_nonselected_join" .handler handlerJoinSource
    "list[str]" handlerJoinResolution

def matchCaptureCase : OracleCase :=
  annotationCase "match_capture_body" .matchCase matchCaptureSource
    "list[str]" matchCaptureFacts

def matchPartialFailureCase : OracleCase :=
  resolutionCase "match_partial_failure_next_case" .matchCase
    matchPartialFailureSource "list[str]" matchPartialFailureResolution

def matchFalseGuardCase : OracleCase :=
  annotationCase "match_false_guard_next_case" .matchCase
    matchFalseGuardSource "list[str]" matchFalseGuardFacts

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
  handlerJoinCase,
  annotationCase "handler_preserves_unrelated_import" .handler
    handlerPreservedSource "tuple[TypeAlias]" handlerPreservedFacts "TypeAlias",
  matchCaptureCase,
  matchPartialFailureCase,
  matchFalseGuardCase,
  matchRefutableCase,
  matchIrrefutableCase,
  annotationCase "match_preserves_unrelated_import" .matchCase
    matchPreservedSource "tuple[TypeAlias]" matchPreservedFacts "TypeAlias"
]

def strictPairs : List (OracleCase × OracleCase) := [
  (handlerBodyCase,
    publicCaseFromInternal "handler_body_after_target_public" "list[str]"
      handlerBodyCase),
  (handlerFallthroughCase,
    publicCaseFromInternal "handler_cleanup_fallthrough_public" "list[str]"
      handlerFallthroughCase),
  (handlerJoinCase,
    publicCaseFromInternal "handler_nonselected_join_public" "list[str]"
      handlerJoinCase),
  (matchCaptureCase,
    publicCaseFromInternal "match_capture_body_public" "list[str]"
      matchCaptureCase),
  (matchPartialFailureCase,
    publicCaseFromInternal "match_partial_failure_next_case_public" "list[str]"
      matchPartialFailureCase),
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

def cases : List OracleCase := internalCases ++ strictCases

def allowedMode (mode : String) : Bool :=
  ["internal-fixture", "strict", "model-only", "infrastructure-error"].contains mode

def allowedResolution (resolution : String) : Bool :=
  ["definitely-builtin", "typing-import", "shadowed", "unknown"].contains resolution

def allowedExitCategory (category : String) : Bool :=
  ["fallthrough", "break", "continue", "terminate"].contains category

def allowedFact (fact : String) : Bool :=
  fact == sequenceFact || fact == typeAliasFact

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
  | _ => mode != "strict"

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

def fixedProjectionValid (item : OracleCase) : Bool :=
  match item.id with
  | "handler_type_before_target" =>
      item.expectedFacts == normalizedFacts handlerObservation.typeEntry
  | "handler_cleanup_return" | "handler_cleanup_raise" =>
      item.expectedFacts == handlerTerminateFacts
  | "handler_nonselected_join" =>
      item.expectedResolution == handlerJoinResolution
  | "match_partial_failure_next_case" =>
      item.expectedResolution == matchPartialFailureResolution
  | "match_false_guard_next_case" =>
      item.expectedFacts == matchFalseGuardFacts
  | "match_irrefutable_exhaustion" =>
      item.expectedFacts == matchIrrefutableFacts
  | _ => true

def strictExpectationMatches (premise candidate : OracleCase) : Bool :=
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
  cases.length == 23 && strictPairs.length == 8 &&
    cases.all (OracleCase.valid cases)

private def brokenBindBeforeType : HandlerObservation :=
  { handlerObservation with
      typeEntry := bindTarget sequenceEnv .destination }

private def brokenCleanupLeavesTerminate : Exits :=
  .categoryOnly .terminate sequenceEnv

private def brokenJoinUsesLeft : Option Env :=
  handlerObservation.exits.fallthrough

private def brokenDiscardPatternFailure : CaseStep :=
  { body := some capturedPattern.matched, nextCase := none }

private def brokenDiscardGuardFailure : CaseStep :=
  { body := none, nextCase := guardedPattern.failed }

private def brokenRetainIrrefutableUnmatched : Option Env :=
  some matchIrrefutableIncoming

def handlerJoinBrokenResolution : Option String :=
  resolutionAtDestination brokenJoinUsesLeft

def patternFailureBrokenResolution : Option String :=
  resolutionAtDestination brokenDiscardPatternFailure.nextCase

def irrefutableBrokenFacts : List String :=
  (finishMatch brokenRetainIrrefutableUnmatched
      matchIrrefutableStep.body.toList).map normalizedFacts |>.getD []

def bindBeforeTypeSensitivity : Bool :=
  normalizedFacts brokenBindBeforeType.typeEntry !=
    normalizedFacts handlerObservation.typeEntry

def handlerExitCleanupSensitivity : Bool :=
  factsAtFirst brokenCleanupLeavesTerminate.terminates !=
    factsAtFirst
      (cleanupHandlerExits .destination
        (.categoryOnly .terminate sequenceEnv)).terminates

def handlerJoinMeetSensitivity : Bool :=
  handlerJoinBrokenResolution != handlerJoinResolution

def patternFailureSensitivity : Bool :=
  patternFailureBrokenResolution != matchPartialFailureResolution

def guardFailureSensitivity : Bool :=
  (brokenDiscardGuardFailure.nextCase.map normalizedFacts |>.getD []) !=
    matchFalseGuardFacts

def irrefutableExhaustionSensitivity : Bool :=
  irrefutableBrokenFacts != matchIrrefutableFacts

def sensitivityPasses : Bool :=
  bindBeforeTypeSensitivity && handlerExitCleanupSensitivity &&
    handlerJoinMeetSensitivity && patternFailureSensitivity &&
    guardFailureSensitivity && irrefutableExhaustionSensitivity

end HoiminOracle.ExceptionMatchBinding
