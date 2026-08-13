import HoiminOracle.NestedTryFlowProofs

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

inductive CandidateExpectation
  | notObserved
  | present
  | absent
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  family : String
  source : String
  marker : String
  entryCategory : ExitCategory
  expected : Exits
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

def mappingOnly : Env where
  source := .shadowed
  destination := .known .typing

def unknownSequence : Env where
  source := .unknown
  destination := .known .typing

def reimportSequence (environment : Env) : Exits :=
  .fallthroughOnly (environment.set .source (.known .typing))

def terminatingFinalizer (environment : Env) : Exits :=
  .categoryOnly .terminate (environment.set .source .shadowed)

def identityStep (environment : Env) : Exits :=
  .fallthroughOnly environment

def composeSingle
    (category : ExitCategory) (environment : Env)
    (finalizer : Env → Exits) : Exits :=
  composeTry (.categoryOnly category environment) .empty none identityStep finalizer

def strictFinallySource : String :=
  "from typing import Sequence\n\ntry:\n    if condition:\n        Sequence = object\n        raise RuntimeError\nfinally:\n    value: Sequence[int]  # finally_annotation\n"

def strictAfterSource : String :=
  "from typing import Sequence\n\ntry:\n    if condition:\n        Sequence = object\n        raise RuntimeError\nfinally:\n    pass\n\nvalue: Sequence[int]  # after_finally\n"

def breakSource : String :=
  "from typing import Mapping, Sequence\n\nwhile condition:\n    try:\n        Sequence = object\n        break  # break_exit\n    finally:\n        from typing import Sequence\n"

def continueSource : String :=
  "from typing import Mapping, Sequence\n\nwhile condition:\n    try:\n        Sequence = object\n        continue  # continue_exit\n    finally:\n        from typing import Sequence\n"

def returnSource : String :=
  "def f():\n    from typing import Mapping, Sequence\n    try:\n        Sequence = object\n        return None  # return_exit\n    finally:\n        from typing import Sequence\n"

def raiseSource : String :=
  "def f():\n    from typing import Mapping, Sequence\n    try:\n        Sequence = object\n        raise RuntimeError  # raise_exit\n    finally:\n        from typing import Sequence\n"

def abruptFallthroughSource : String :=
  "from typing import Mapping, Sequence\n\ntry:\n    pass  # abrupt_fallthrough\nfinally:\n    Sequence = object\n    raise RuntimeError\n"

def abruptBreakSource : String :=
  "from typing import Mapping, Sequence\n\nwhile condition:\n    try:\n        break  # abrupt_break\n    finally:\n        Sequence = object\n        raise RuntimeError\n"

def unreachableSource : String :=
  "def f():\n    from typing import Mapping, Sequence\n    try:\n        Sequence = object\n        return None  # unreachable_return\n        from typing import Sequence\n    finally:\n        pass\n"

def nonselectedHandlerSource : String :=
  "from typing import Mapping, Sequence\n\ntry:\n    Sequence = object\nexcept ValueError as Sequence:\n    from typing import Sequence\n    pass  # handler_meet\n"

def nestedTryFlowCases : List OracleCase := [
  { id := "finally_annotation_meets_normal_and_raise"
    mode := "strict"
    family := "finally-annotation"
    source := strictFinallySource
    marker := "value: Sequence[int]"
    entryCategory := .fallthrough
    expected := composeTry .empty .empty none identityStep identityStep
    candidate := .absent },
  { id := "post_finally_uses_only_fallthrough"
    mode := "strict"
    family := "post-finally"
    source := strictAfterSource
    marker := "value: Sequence[int]"
    entryCategory := .fallthrough
    expected := composeSingle .fallthrough sequenceOnly identityStep
    candidate := .present
    candidateCount := 1
    candidatePath := some "target.py"
    candidateOperator := some "type_list_sequence"
    candidateOriginal := some "Sequence[int]"
    candidateReplacement := some "list[int]" },
  { id := "falling_finally_preserves_break"
    mode := "internal-fixture"
    family := "category-routing"
    source := breakSource
    marker := "# break_exit"
    entryCategory := .break
    expected := composeSingle .break mappingOnly reimportSequence },
  { id := "falling_finally_preserves_continue"
    mode := "internal-fixture"
    family := "category-routing"
    source := continueSource
    marker := "# continue_exit"
    entryCategory := .continue
    expected := composeSingle .continue mappingOnly reimportSequence },
  { id := "falling_finally_preserves_return_terminate"
    mode := "internal-fixture"
    family := "category-routing"
    source := returnSource
    marker := "# return_exit"
    entryCategory := .terminate
    expected := composeSingle .terminate mappingOnly reimportSequence },
  { id := "falling_finally_preserves_raise_terminate"
    mode := "internal-fixture"
    family := "category-routing"
    source := raiseSource
    marker := "# raise_exit"
    entryCategory := .terminate
    expected := composeSingle .terminate mappingOnly reimportSequence },
  { id := "abrupt_finally_replaces_fallthrough"
    mode := "internal-fixture"
    family := "abrupt-finally"
    source := abruptFallthroughSource
    marker := "# abrupt_fallthrough"
    entryCategory := .fallthrough
    expected := composeSingle .fallthrough knownBoth terminatingFinalizer },
  { id := "abrupt_finally_replaces_break"
    mode := "internal-fixture"
    family := "abrupt-finally"
    source := abruptBreakSource
    marker := "# abrupt_break"
    entryCategory := .break
    expected := composeSingle .break knownBoth terminatingFinalizer },
  { id := "unreachable_post_return_excluded"
    mode := "internal-fixture"
    family := "reachability"
    source := unreachableSource
    marker := "# unreachable_return"
    entryCategory := .terminate
    expected := composeSingle .terminate mappingOnly identityStep },
  { id := "nonselected_handler_meet"
    mode := "model-only"
    family := "handler-meet"
    source := nonselectedHandlerSource
    marker := "# handler_meet"
    entryCategory := .fallthrough
    expected := composeTry (.fallthroughOnly knownBoth)
      (.fallthroughOnly mappingOnly) none identityStep identityStep }
]

def brokenCleanupBeforeBody (name : Name) (handlerBody : Env → Exits)
    (entry : Env) : Exits :=
  handlerBody (cleanupName name entry)

def cleanupObservingFinalizer (environment : Env) : Exits :=
  if environment.get .source == .absent then
    .fallthroughOnly (environment.set .destination (.known .typing))
  else
    .categoryOnly .terminate environment

def cleanupBoundarySensitivity : Bool :=
  let handlerBody := fun environment =>
    .fallthroughOnly (environment.set .source (.known .typing))
  let handler := handlerBody mappingOnly
  let correct := composeTry .empty handler (some .source)
    identityStep cleanupObservingFinalizer
  let brokenHandler := brokenCleanupBeforeBody .source handlerBody mappingOnly
  let brokenBeforeBody := composeTry .empty brokenHandler none
    identityStep cleanupObservingFinalizer
  let brokenDelayed := cleanupExits .source
    (composeTry .empty handler none identityStep cleanupObservingFinalizer)
  correct != brokenBeforeBody && correct != brokenDelayed

def brokenFallthroughOnlyFinally
    (incoming : Exits) (finalizer : Env → Exits) : Exits :=
  match incoming.fallthrough with
  | none => incoming
  | some environment =>
      (incoming.withoutFallthrough).merge (finalizer environment)

def finalizerCoverageSensitivity : Bool :=
  let detects := fun category =>
    let incoming := Exits.categoryOnly category mappingOnly
    composeSingle category mappingOnly reimportSequence !=
      brokenFallthroughOnlyFinally incoming reimportSequence
  -- The two terminate witnesses correspond to distinct return and raise sources.
  detects .break && detects .continue && detects .terminate && detects .terminate

def brokenFlattenCategory
    (environment : Env) (finalizer : Env → Exits) : Exits :=
  finalizer environment

def categoryPreservationSensitivity : Bool :=
  let detects := fun category =>
    composeSingle category mappingOnly reimportSequence !=
      brokenFlattenCategory mappingOnly reimportSequence
  detects .break && detects .continue && detects .terminate

def brokenRetainIncomingCategory
    (category : ExitCategory) (environment : Env)
    (finalizer : Env → Exits) : Exits :=
  match (finalizer environment).states with
  | [] => .empty
  | first :: _ => .categoryOnly category first

def abruptReplacementSensitivity : Bool :=
  let detects := fun category =>
    composeSingle category knownBoth terminatingFinalizer !=
      brokenRetainIncomingCategory category knownBoth terminatingFinalizer
  detects .fallthrough && detects .break

def brokenSequentialMerge
    (first : Exits) (next : Env → Exits) (unreachableEntry : Env) : Exits :=
  first.merge (next unreachableEntry)

def unreachableJoinSensitivity : Bool :=
  let first := Exits.categoryOnly .terminate mappingOnly
  let next := fun environment => .fallthroughOnly
    (environment.set .source (.known .typing))
  let correct := composeTry first .empty none next identityStep
  let broken := routeFinally
    (brokenSequentialMerge first next mappingOnly) identityStep
  outgoingEnv correct != outgoingEnv broken

def omittedAbruptSensitivity : Bool :=
  let reachable := (Exits.fallthroughOnly knownBoth).merge
    (.categoryOnly .terminate mappingOnly)
  let correct := composeTry reachable .empty none identityStep identityStep
  let broken := composeTry (.fallthroughOnly knownBoth) .empty none
    identityStep identityStep
  composeTryOutgoing reachable .empty none identityStep identityStep !=
    composeTryOutgoing (.fallthroughOnly knownBoth) .empty none
      identityStep identityStep && correct != broken

def brokenCleanupName (name : Name) (environment : Env) : Env :=
  if environment.get name == .absent then
    environment.set name (.known .builtin)
  else
    cleanupName name environment

def brokenCleanupExits (name : Name) (exits : Exits) : Exits :=
  mapExits (brokenCleanupName name) exits

def cleanupIdempotencySensitivity : Bool :=
  let exits := Exits.fallthroughOnly knownBoth
  let once := composeTry .empty exits (some .source) identityStep identityStep
  let twice := composeTry .empty once (some .source) identityStep identityStep
  let brokenOnce := composeTry .empty (brokenCleanupExits .source exits) none
    identityStep identityStep
  let brokenTwice := composeTry .empty (brokenCleanupExits .source brokenOnce) none
    identityStep identityStep
  twice == once && brokenTwice != brokenOnce

def sensitivityPasses : Bool :=
  cleanupBoundarySensitivity && finalizerCoverageSensitivity &&
    categoryPreservationSensitivity && abruptReplacementSensitivity &&
    unreachableJoinSensitivity && omittedAbruptSensitivity &&
    cleanupIdempotencySensitivity

def validMode (mode : String) : Bool :=
  mode == "strict" || mode == "internal-fixture" || mode == "model-only"

def fixedExpectationSafe (item : OracleCase) : Bool :=
  match item.id with
  | "finally_annotation_meets_normal_and_raise" =>
      item.mode == "strict" &&
        item.expected == composeTry .empty .empty none identityStep identityStep &&
        item.marker == "value: Sequence[int]" && item.candidate == .absent
  | "post_finally_uses_only_fallthrough" =>
      item.mode == "strict" &&
        item.expected == composeSingle .fallthrough sequenceOnly identityStep &&
        item.marker == "value: Sequence[int]" && item.candidate == .present &&
        item.candidateCount == 1 && item.candidatePath == some "target.py" &&
        item.candidateOperator == some "type_list_sequence" &&
        item.candidateOriginal == some "Sequence[int]" &&
        item.candidateReplacement == some "list[int]" &&
        item.candidateSymbol == none
  | "falling_finally_preserves_break" =>
      item.expected == composeSingle .break mappingOnly reimportSequence
  | "falling_finally_preserves_continue" =>
      item.expected == composeSingle .continue mappingOnly reimportSequence
  | "falling_finally_preserves_return_terminate" |
      "falling_finally_preserves_raise_terminate" =>
      item.expected == composeSingle .terminate mappingOnly reimportSequence
  | "abrupt_finally_replaces_fallthrough" =>
      item.expected == composeSingle .fallthrough knownBoth terminatingFinalizer
  | "abrupt_finally_replaces_break" =>
      item.expected == composeSingle .break knownBoth terminatingFinalizer
  | "unreachable_post_return_excluded" =>
      item.expected == composeSingle .terminate mappingOnly identityStep
  | "nonselected_handler_meet" =>
      item.mode == "model-only" &&
        item.expected == composeTry (.fallthroughOnly knownBoth)
          (.fallthroughOnly mappingOnly) none identityStep identityStep
  | _ => false

def fixedCasesPass : Bool :=
  nestedTryFlowCases.length == 10 &&
    nestedTryFlowCases.all fun item =>
      item.schema == 1 && validMode item.mode && !item.id.isEmpty &&
        !item.family.isEmpty && !item.source.isEmpty && !item.marker.isEmpty &&
        ((item.candidate == .present && item.candidateCount == 1) ||
          (item.candidate != .present && item.candidateCount == 0 &&
            item.candidatePath == none && item.candidateOperator == none &&
            item.candidateOriginal == none && item.candidateReplacement == none &&
            item.candidateSymbol == none)) &&
        fixedExpectationSafe item

end HoiminOracle.NestedTryFlow
