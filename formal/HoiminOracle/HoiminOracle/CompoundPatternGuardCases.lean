import HoiminOracle.CompoundPatternGuardProofs

namespace HoiminOracle.CompoundPatternGuard

open HoiminOracle.BindingFlow

inductive ObservationKind
  | caseEntry
  | nextCaseEntry
  | publicCandidate
  | modelWitness
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  observationKind : ObservationKind
  family : String
  source : String
  marker : String
  expectedFacts : List String := []
  candidateCount : Nat := 0
  candidatePath : Option String := none
  candidateStart : Option Nat := none
  candidateLength : Option Nat := none
  candidateOperator : Option String := none
  candidateOriginal : Option String := none
  candidateReplacement : Option String := none
  candidateSymbol : Option String := none
  deriving Repr, DecidableEq, BEq

def mappingFact : String := "direct:Mapping=typing.Mapping"
def sequenceFact : String := "direct:Sequence=typing.Sequence"

def normalizedFacts (environment : Env) : List String :=
  (if environment.source == .known .typing then [mappingFact] else []) ++
    (if environment.destination == .known .typing then [sequenceFact] else [])

def optionalFacts : Option Env → List String
  | none => []
  | some environment => normalizedFacts environment

def bothKnown : Env where
  source := .known .typing
  destination := .known .typing

def sequenceOnly : Env where
  source := .absent
  destination := .known .typing

def mappingKnown : Env where
  source := .known .typing
  destination := .shadowed

def sequenceKnown : Env where
  source := .shadowed
  destination := .known .typing

def bothShadowed : Env where
  source := .shadowed
  destination := .shadowed

def refutableThenCapture (incoming : Env) (name : Name) : Attempt :=
  thenAttempt (pureTest incoming true true) (capture name)

def orAttempt : Attempt :=
  orPattern [refutableThenCapture bothKnown .destination,
    refutableThenCapture bothKnown .destination]

def asAttempt : Attempt :=
  asPattern (pureTest sequenceOnly true true) .destination

def preservedAttempt : Attempt :=
  asPattern (pureTest bothKnown true true) .destination

def mappingAttempt : Attempt :=
  mappingPattern (pureTest sequenceOnly true true) [] (some .destination)

def classAttempt : Attempt :=
  classPattern (pureTest sequenceOnly true true)
    [fun incoming => pureTest incoming true true, capture .destination]

def falseGuardAttempt : Attempt :=
  thenAttempt (pureTest bothKnown true true) (capture .source)

def falseGuardResult : GuardResult :=
  applyGuard falseGuardAttempt
    (some fun incoming => (incoming.set .destination .shadowed, false))

def orBodyFacts : List String := optionalFacts orAttempt.summary.matched
def orFailureFacts : List String := optionalFacts orAttempt.summary.failed
def asFailureFacts : List String := optionalFacts asAttempt.summary.failed
def mappingFailureFacts : List String := optionalFacts mappingAttempt.summary.failed
def classFailureFacts : List String := optionalFacts classAttempt.summary.failed
def falseGuardFacts : List String := optionalFacts falseGuardResult.nextCase
def preservedFacts : List String := optionalFacts preservedAttempt.summary.failed

def orSource : String :=
  "from typing import Mapping, Sequence\nmatch value:\n    case [0, Sequence] | {\"item\": Sequence}:\n        or_body_marker: list[str]\n    case _:\n        or_failure_marker: list[str]\n"

def asPrefix : String :=
  "from typing import Sequence\nmatch value:\n    case [0] as Sequence:\n        pass\n    case _:\n        as_failure_marker: "

def asSource : String := asPrefix ++ "list[int]\n"

def mappingPrefix : String :=
  "from typing import Sequence\nmatch value:\n    case {\"tag\": 0, **Sequence}:\n        pass\n    case _:\n        mapping_failure_marker: "

def mappingSource : String := mappingPrefix ++ "list[int]\n"

def classPrefix : String :=
  "from typing import Sequence\nmatch value:\n    case Point(0, tail=Sequence):\n        pass\n    case _:\n        class_failure_marker: "

def classSource : String := classPrefix ++ "list[int]\n"

def falseGuardSource : String :=
  "from typing import Mapping, Sequence\nmatch value:\n    case [Mapping] if ((Sequence := local_sequence) and False):\n        pass\n    case _:\n        false_guard_marker: list[str]\n"

def preservedSource : String :=
  "from typing import Mapping, Sequence\nmatch value:\n    case [0] as Sequence:\n        pass\n    case _:\n        preserved_mapping_marker: tuple[Mapping]\n"

def internalCase (id : String) (kind : ObservationKind) (family source marker : String)
    (facts : List String) : OracleCase where
  id
  mode := "internal-fixture"
  observationKind := kind
  family
  source
  marker
  expectedFacts := facts

def strictCase (id family source marker : String) (start : Nat) : OracleCase where
  id
  mode := "strict"
  observationKind := .publicCandidate
  family
  source
  marker
  candidateCount := 1
  candidatePath := some "target.py"
  candidateStart := some start
  candidateLength := some marker.length
  candidateOperator := some "type_list_sequence"
  candidateOriginal := some "list[int]"
  candidateReplacement := some "Sequence[int]"

def unequalOrCase : OracleCase where
  id := "unequal_or_capture_sets"
  mode := "model-only"
  observationKind := .modelWitness
  family := "or-success-meet"
  source := ""
  marker := ""
  expectedFacts := normalizedFacts (mappingKnown.meet sequenceKnown)

def compoundPatternGuardCases : List OracleCase := [
  internalCase "or_success_meets_arms" .caseEntry "or-success-meet"
    orSource "or_body_marker" orBodyFacts,
  internalCase "or_failure_meets_arms" .nextCaseEntry "or-failure-meet"
    orSource "or_failure_marker" orFailureFacts,
  internalCase "as_child_failure_precedes_alias" .nextCaseEntry
    "as-binding-point" asSource "list[int]" asFailureFacts,
  internalCase "mapping_child_failure_precedes_rest" .nextCaseEntry
    "mapping-rest-binding-point" mappingSource "list[int]" mappingFailureFacts,
  internalCase "class_early_failure_precedes_capture" .nextCaseEntry
    "class-capture-binding-point" classSource "list[int]" classFailureFacts,
  internalCase "false_guard_uses_post_guard" .nextCaseEntry
    "false-guard" falseGuardSource "false_guard_marker" falseGuardFacts,
  internalCase "compound_preserves_mapping" .nextCaseEntry
    "unrelated-fact" preservedSource "tuple[Mapping]" preservedFacts,
  strictCase "as_failure_public_candidate" "as-binding-point"
    asSource "list[int]" asPrefix.length,
  strictCase "mapping_failure_public_candidate" "mapping-rest-binding-point"
    mappingSource "list[int]" mappingPrefix.length,
  strictCase "class_failure_public_candidate" "class-capture-binding-point"
    classSource "list[int]" classPrefix.length,
  unequalOrCase
]

def prePatternCorrect : Attempt :=
  thenAttempt (capture .destination bothKnown) fun incoming =>
    pureTest incoming false true

def prePatternFailureSensitivity : Bool :=
  prePatternCorrect.summary.failed != some bothKnown

def lateCaptureFailureSensitivity : Bool :=
  asAttempt.summary.failed != some bothShadowed

def unequalOrAttempt : Attempt :=
  orPattern [{ matched := [mappingKnown] }, { matched := [sequenceKnown] }]

def orKeepFirstSuccessSensitivity : Bool :=
  unequalOrAttempt.summary.matched != some mappingKnown

def orKeepLastSuccessSensitivity : Bool :=
  unequalOrAttempt.summary.matched != some sequenceKnown

def unequalFailureAttempt : Attempt :=
  orPattern [{ failed := [mappingKnown] }, { failed := [sequenceKnown] }]

def orDropFailureSensitivity : Bool :=
  unequalFailureAttempt.summary.failed != some mappingKnown &&
    unequalFailureAttempt.summary.failed != some sequenceKnown

def preGuardFailureSensitivity : Bool :=
  let attempt : Attempt := { matched := [bothKnown] }
  let correct := applyGuard attempt
    (some fun incoming => (incoming.set .destination .shadowed, false))
  correct.nextCase != some bothKnown

def unreachableOutcomeSensitivity : Bool :=
  let correct := (orPattern [{ matched := [bothKnown] }, {}]).summary.matched
  let broken := meetAll? [bothKnown, bothShadowed]
  correct != broken

def overbroadCleanupSensitivity : Bool :=
  (capture .destination bothKnown).summary.matched != some bothShadowed

def sensitivityPasses : Bool :=
  prePatternFailureSensitivity && lateCaptureFailureSensitivity &&
    orKeepFirstSuccessSensitivity && orKeepLastSuccessSensitivity &&
    orDropFailureSensitivity && preGuardFailureSensitivity &&
    unreachableOutcomeSensitivity && overbroadCleanupSensitivity

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "internal-fixture" ||
    mode == "model-only" || mode == "infrastructure-error"

def markerOccursOnce (item : OracleCase) : Bool :=
  !item.marker.isEmpty && (item.source.splitOn item.marker).length == 2

def candidateFieldsValid (item : OracleCase) : Bool :=
  if item.mode == "strict" then
    item.observationKind == .publicCandidate && item.expectedFacts.isEmpty &&
      item.candidateCount == 1 && item.candidatePath == some "target.py" &&
      item.candidateStart.isSome && item.candidateLength == some item.marker.length &&
      item.candidateOperator == some "type_list_sequence" &&
      item.candidateOriginal == some "list[int]" &&
      item.candidateReplacement == some "Sequence[int]" &&
      item.candidateSymbol == none
  else
    item.candidateCount == 0 && item.candidatePath == none &&
      item.candidateStart == none && item.candidateLength == none &&
      item.candidateOperator == none && item.candidateOriginal == none &&
      item.candidateReplacement == none && item.candidateSymbol == none

def caseValid (item : OracleCase) : Bool :=
  item.schema == 1 && !item.id.isEmpty && !item.family.isEmpty &&
    validMode item.mode && candidateFieldsValid item &&
    if item.mode == "model-only" then
      item.observationKind == .modelWitness && item.source.isEmpty &&
        item.marker.isEmpty
    else markerOccursOnce item

def fixedCasesPass : Bool :=
  compoundPatternGuardCases.length == 11 &&
    (compoundPatternGuardCases.map OracleCase.id).eraseDups.length == 11 &&
    compoundPatternGuardCases.all caseValid

end HoiminOracle.CompoundPatternGuard
