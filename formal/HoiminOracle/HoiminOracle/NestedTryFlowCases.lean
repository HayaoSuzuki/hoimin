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
    expected := .empty
    candidate := .absent },
  { id := "post_finally_uses_only_fallthrough"
    mode := "strict"
    family := "post-finally"
    source := strictAfterSource
    marker := "value: Sequence[int]"
    entryCategory := .fallthrough
    expected := .fallthroughOnly sequenceOnly
    candidate := .present },
  { id := "falling_finally_preserves_break"
    mode := "internal-fixture"
    family := "category-routing"
    source := breakSource
    marker := "# break_exit"
    entryCategory := .break
    expected := routeFinally (.categoryOnly .break mappingOnly) reimportSequence },
  { id := "falling_finally_preserves_continue"
    mode := "internal-fixture"
    family := "category-routing"
    source := continueSource
    marker := "# continue_exit"
    entryCategory := .continue
    expected := routeFinally (.categoryOnly .continue mappingOnly) reimportSequence },
  { id := "falling_finally_preserves_return_terminate"
    mode := "internal-fixture"
    family := "category-routing"
    source := returnSource
    marker := "# return_exit"
    entryCategory := .terminate
    expected := routeFinally (.categoryOnly .terminate mappingOnly) reimportSequence },
  { id := "falling_finally_preserves_raise_terminate"
    mode := "internal-fixture"
    family := "category-routing"
    source := raiseSource
    marker := "# raise_exit"
    entryCategory := .terminate
    expected := routeFinally (.categoryOnly .terminate mappingOnly) reimportSequence },
  { id := "abrupt_finally_replaces_fallthrough"
    mode := "internal-fixture"
    family := "abrupt-finally"
    source := abruptFallthroughSource
    marker := "# abrupt_fallthrough"
    entryCategory := .fallthrough
    expected := routeFinally (.categoryOnly .fallthrough knownBoth) terminatingFinalizer },
  { id := "abrupt_finally_replaces_break"
    mode := "internal-fixture"
    family := "abrupt-finally"
    source := abruptBreakSource
    marker := "# abrupt_break"
    entryCategory := .break
    expected := routeFinally (.categoryOnly .break knownBoth) terminatingFinalizer },
  { id := "unreachable_post_return_excluded"
    mode := "internal-fixture"
    family := "reachability"
    source := unreachableSource
    marker := "# unreachable_return"
    entryCategory := .terminate
    expected := .categoryOnly .terminate mappingOnly },
  { id := "nonselected_handler_meet"
    mode := "model-only"
    family := "handler-meet"
    source := nonselectedHandlerSource
    marker := "# handler_meet"
    entryCategory := .fallthrough
    expected := .fallthroughOnly unknownSequence }
]

def brokenCleanupBeforeBody (name : Name) (handlerBody : Env → Exits)
    (entry : Env) : Exits :=
  handlerBody (cleanupName name entry)

def cleanupBoundarySensitivity : Bool :=
  let handlerBody := fun environment =>
    .fallthroughOnly (environment.set .source (.known .typing))
  let correct := cleanupExits .source (handlerBody mappingOnly)
  let broken := brokenCleanupBeforeBody .source handlerBody mappingOnly
  correct != broken

def brokenFallthroughOnlyFinally
    (incoming : Exits) (finalizer : Env → Exits) : Exits :=
  match incoming.fallthrough with
  | none => incoming
  | some environment =>
      (incoming.withoutFallthrough).merge (finalizer environment)

def finalizerCoverageSensitivity : Bool :=
  let incoming := Exits.categoryOnly .break mappingOnly
  routeFinally incoming reimportSequence !=
    brokenFallthroughOnlyFinally incoming reimportSequence

def brokenFlattenCategory
    (environment : Env) (finalizer : Env → Exits) : Exits :=
  finalizer environment

def categoryPreservationSensitivity : Bool :=
  routeFinally (.categoryOnly .continue mappingOnly) reimportSequence !=
    brokenFlattenCategory mappingOnly reimportSequence

def brokenRetainIncomingCategory
    (category : ExitCategory) (environment : Env)
    (finalizer : Env → Exits) : Exits :=
  match (finalizer environment).states with
  | [] => .empty
  | first :: _ => .categoryOnly category first

def abruptReplacementSensitivity : Bool :=
  routeFinally (.categoryOnly .break knownBoth) terminatingFinalizer !=
    brokenRetainIncomingCategory .break knownBoth terminatingFinalizer

def brokenSequentialMerge
    (first : Exits) (next : Env → Exits) (unreachableEntry : Env) : Exits :=
  first.merge (next unreachableEntry)

def unreachableJoinSensitivity : Bool :=
  let first := Exits.categoryOnly .terminate mappingOnly
  let next := fun environment => .fallthroughOnly
    (environment.set .source (.known .typing))
  first.andThen next != brokenSequentialMerge first next mappingOnly

def omittedAbruptSensitivity : Bool :=
  let reachable := [knownBoth, mappingOnly]
  meetAll? reachable != meetAll? [knownBoth]

def brokenCleanupName (name : Name) (environment : Env) : Env :=
  if environment.get name == .absent then
    environment.set name (.known .builtin)
  else
    cleanupName name environment

def brokenCleanupExits (name : Name) (exits : Exits) : Exits :=
  mapExits (brokenCleanupName name) exits

def cleanupIdempotencySensitivity : Bool :=
  let exits := Exits.fallthroughOnly knownBoth
  cleanupExits .source (cleanupExits .source exits) == cleanupExits .source exits &&
    brokenCleanupExits .source (brokenCleanupExits .source exits) !=
      brokenCleanupExits .source exits

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
      item.mode == "strict" && item.expected == Exits.empty &&
        item.marker == "value: Sequence[int]" && item.candidate == .absent
  | "post_finally_uses_only_fallthrough" =>
      item.mode == "strict" &&
        item.expected == Exits.fallthroughOnly sequenceOnly &&
        item.marker == "value: Sequence[int]" && item.candidate == .present
  | "falling_finally_preserves_break" =>
      item.expected == routeFinally (.categoryOnly .break mappingOnly) reimportSequence
  | "falling_finally_preserves_continue" =>
      item.expected == routeFinally (.categoryOnly .continue mappingOnly) reimportSequence
  | "falling_finally_preserves_return_terminate" |
      "falling_finally_preserves_raise_terminate" =>
      item.expected == routeFinally (.categoryOnly .terminate mappingOnly) reimportSequence
  | "abrupt_finally_replaces_fallthrough" =>
      item.expected == routeFinally
        (.categoryOnly .fallthrough knownBoth) terminatingFinalizer
  | "abrupt_finally_replaces_break" =>
      item.expected == routeFinally (.categoryOnly .break knownBoth) terminatingFinalizer
  | "unreachable_post_return_excluded" =>
      item.expected == Exits.categoryOnly .terminate mappingOnly
  | "nonselected_handler_meet" =>
      item.mode == "model-only" &&
        item.expected == Exits.fallthroughOnly unknownSequence
  | _ => false

def fixedCasesPass : Bool :=
  nestedTryFlowCases.length == 10 &&
    nestedTryFlowCases.all fun item =>
      item.schema == 1 && validMode item.mode && !item.id.isEmpty &&
        !item.family.isEmpty && !item.source.isEmpty && !item.marker.isEmpty &&
        fixedExpectationSafe item

example : sensitivityPasses = true := by native_decide
example : fixedCasesPass = true := by native_decide

end HoiminOracle.NestedTryFlow
