import HoiminOracle.AnnotationScopeProofs

namespace HoiminOracle.AnnotationScope

open BindingFlow

inductive ObservationKind
  | annotation
  | resolution
  | publicCandidate
  deriving Repr, DecidableEq, BEq

inductive Scenario
  | globalBefore
  | globalAfterWrite
  | globalRestored
  | globalUnaffected
  | nonlocalBefore
  | nonlocalAfterWrite
  | nonlocalRestored
  | nonlocalUnaffected
  | classGlobal
  | classNonlocal
  | classGlobalUnaffected
  | classNonlocalUnaffected
  | listOuterBefore
  | listOuterAfter
  | listAfter
  | listFirstIterable
  | listBody
  | setBody
  | dictBody
  | generatorBody
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : Scenario
  observationKind : ObservationKind
  source : String
  marker : String
  expectedFacts : List String := []
  expectedSymbol : Option String := none
  expectedScope : Option String := none
  expectedResolution : Option String := none
  expectedOperator : Option String := none
  expectedOriginal : Option String := none
  expectedReplacement : Option String := none
  expectedPresent : Bool
  deriving Repr, DecidableEq, BEq

def Scenario.label : Scenario → String
  | .globalBefore => "global-before"
  | .globalAfterWrite => "global-after-write"
  | .globalRestored => "global-restored"
  | .globalUnaffected => "global-unaffected"
  | .nonlocalBefore => "nonlocal-before"
  | .nonlocalAfterWrite => "nonlocal-after-write"
  | .nonlocalRestored => "nonlocal-restored"
  | .nonlocalUnaffected => "nonlocal-unaffected"
  | .classGlobal => "class-global"
  | .classNonlocal => "class-nonlocal"
  | .classGlobalUnaffected => "class-global-unaffected"
  | .classNonlocalUnaffected => "class-nonlocal-unaffected"
  | .listOuterBefore => "list-outer-before"
  | .listOuterAfter => "list-outer-after"
  | .listAfter => "list-after"
  | .listFirstIterable => "list-first-iterable"
  | .listBody => "list-body"
  | .setBody => "set-body"
  | .dictBody => "dict-body"
  | .generatorBody => "generator-body"

def ObservationKind.label : ObservationKind → String
  | .annotation => "annotation"
  | .resolution => "resolution"
  | .publicCandidate => "public-candidate"

def Resolution.label : Resolution → String
  | .definitelyBuiltin => "definitely-builtin"
  | .shadowed => "shadowed"
  | .unknown => "unknown"

def normalizedFacts (environment : Env) : List String :=
  match environment.destination with
  | .known .typing => ["direct:Sequence=typing.Sequence"]
  | _ => []

def initialEnv : Env where
  source := .known .builtin
  destination := .known .typing

def initialDirected : DirectedState where
  moduleEnv := initialEnv
  nearestFunctionEnv := initialEnv
  currentEnv := initialEnv

def globalWritten : DirectedState :=
  writeDirected .global .destination .shadowed initialDirected

def globalRestored : DirectedState :=
  writeDirected .global .destination (.known .typing) globalWritten

def nonlocalWritten : DirectedState :=
  writeDirected .nonlocal .destination .shadowed initialDirected

def nonlocalRestored : DirectedState :=
  writeDirected .nonlocal .destination (.known .typing) nonlocalWritten

def annotationCase
    (id mode : String)
    (scenario : Scenario)
    (source : String)
    (facts : List String)
    (symbol : String)
    (present : Bool) : OracleCase where
  id
  mode
  scenario
  observationKind := .annotation
  source
  marker := "list[str]"
  expectedFacts := facts
  expectedSymbol := some symbol
  expectedScope := some "function"
  expectedPresent := present

def annotationCaseAt
    (id : String)
    (scenario : Scenario)
    (source : String)
    (facts : List String)
    (symbol : Option String)
    (scope : String)
    (present : Bool) : OracleCase where
  id
  mode := "internal-fixture"
  scenario
  observationKind := .annotation
  source
  marker := "list[str]"
  expectedFacts := facts
  expectedSymbol := symbol
  expectedScope := some scope
  expectedPresent := present

def resolutionCase
    (id : String)
    (scenario : Scenario)
    (source marker : String)
    (resolution : Resolution) : OracleCase where
  id
  mode := "internal-fixture"
  scenario
  observationKind := .resolution
  source
  marker
  expectedResolution := some resolution.label
  expectedPresent := resolution == .definitelyBuiltin

def publicCase
    (id : String)
    (scenario : Scenario)
    (source marker : String)
    (symbol : Option String)
    (operator original replacement : String)
    (present : Bool) : OracleCase where
  id
  mode := "strict"
  scenario
  observationKind := .publicCandidate
  source
  marker
  expectedSymbol := symbol
  expectedOperator := some operator
  expectedOriginal := some original
  expectedReplacement := some replacement
  expectedPresent := present

def publicTypeCase
    (id : String)
    (scenario : Scenario)
    (source : String)
    (environment : Env)
    (symbol : String) : OracleCase :=
  let present := !(normalizedFacts environment).isEmpty
  publicCase id scenario source "list[str]" (if present then some symbol else none)
    "type_list_sequence" "list[str]" "Sequence[str]" present

def publicResolutionCase
    (id : String)
    (scenario : Scenario)
    (source marker : String)
    (resolution : Resolution)
    (symbol : String) : OracleCase :=
  let present := resolution == .definitelyBuiltin
  publicCase id scenario source marker (if present then some symbol else none)
    "collection_list_tuple" "list" "tuple" present

def globalBeforeSource : String :=
  "from typing import Sequence\ndef global_scope():\n    global Sequence\n    global_before: list[str]\n"

def globalAfterSource : String :=
  "from typing import Sequence\ndef global_scope():\n    global Sequence\n    Sequence = object\n    global_after: list[str]\n"

def globalRestoredSource : String :=
  "from typing import Sequence\ndef global_scope():\n    global Sequence\n    Sequence = object\n    from typing import Sequence\n    global_restored: list[str]\n"

def globalUnaffectedSource : String :=
  "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    def inner():\n        global Sequence\n        Sequence = object\n    unaffected: list[str]\n"

def nonlocalBeforeSource : String :=
  "def outer():\n    from typing import Sequence\n    def inner():\n        nonlocal Sequence\n        nonlocal_before: list[str]\n"

def nonlocalAfterSource : String :=
  "def outer():\n    from typing import Sequence\n    def inner():\n        nonlocal Sequence\n        Sequence = object\n        nonlocal_after: list[str]\n"

def nonlocalRestoredSource : String :=
  "def outer():\n    from typing import Sequence\n    def inner():\n        nonlocal Sequence\n        Sequence = object\n        from typing import Sequence\n        nonlocal_restored: list[str]\n"

def nonlocalUnaffectedSource : String :=
  "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    def inner():\n        nonlocal Sequence\n        Sequence = object\nunaffected: list[str]\n"

def classGlobalSource : String :=
  "from typing import Sequence\nclass GlobalWrite:\n    global Sequence\n    Sequence = object\n    def method(self):\n        hidden: list[str]\n"

def classNonlocalSource : String :=
  "def outer():\n    from typing import Sequence\n    class NonlocalWrite:\n        nonlocal Sequence\n        Sequence = object\n        def method(self):\n            hidden: list[str]\n"

def classGlobalUnaffectedSource : String :=
  "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    class GlobalWrite:\n        global Sequence\n        Sequence = object\n    unaffected: list[str]\n"

def classNonlocalUnaffectedSource : String :=
  "from typing import Sequence\ndef outer():\n    from typing import Sequence\n    class NonlocalWrite:\n        nonlocal Sequence\n        Sequence = object\nunaffected: list[str]\n"

def listOuterBeforeSource : String :=
  "from typing import Sequence\ndef collect(values):\n    before: list[str]\n    result = [item for item in values]\n"

def listOuterAfterSource : String :=
  "from typing import Sequence\ndef collect(values):\n    result = [item for item in values]\n    after: list[str]\n"

def listAfterSource : String :=
  "def collect(values):\n    result = [item for list in values]\n    after = list(values)\n"

def listFirstIterableSource : String :=
  "def collect(values):\n    result = [item for list in list(values)]\n"

def listBodySource : String :=
  "def collect(values):\n    result = [list(item) for list in values]\n"

def setBodySource : String :=
  "def collect(values):\n    result = {list(item) for list in values}\n"

def dictBodySource : String :=
  "def collect(values):\n    result = {list(item): item for list in values}\n"

def generatorBodySource : String :=
  "def collect(values):\n    result = (list(item) for list in values)\n"

def comprehensionObservation : ComprehensionObservation :=
  observeComprehension initialEnv .source

def afterComprehensionResolution (observation : ComprehensionObservation) : Resolution :=
  resolutionOfFact (observation.after.get .source)

def internalCases : List OracleCase := [
  annotationCase "global_before_site" "internal-fixture" .globalBefore
    globalBeforeSource (normalizedFacts initialDirected.moduleEnv) "global_scope" true,
  annotationCase "global_after_write_site" "internal-fixture" .globalAfterWrite
    globalAfterSource (normalizedFacts globalWritten.moduleEnv) "global_scope" false,
  annotationCase "global_restored_site" "internal-fixture" .globalRestored
    globalRestoredSource (normalizedFacts globalRestored.moduleEnv) "global_scope" true,
  annotationCaseAt "global_unaffected_function_site" .globalUnaffected
    globalUnaffectedSource (normalizedFacts globalWritten.nearestFunctionEnv)
      (some "outer") "function" true,
  annotationCase "nonlocal_before_site" "internal-fixture" .nonlocalBefore
    nonlocalBeforeSource (normalizedFacts initialDirected.nearestFunctionEnv) "outer.inner" true,
  annotationCase "nonlocal_after_write_site" "internal-fixture" .nonlocalAfterWrite
    nonlocalAfterSource (normalizedFacts nonlocalWritten.nearestFunctionEnv) "outer.inner" false,
  annotationCase "nonlocal_restored_site" "internal-fixture" .nonlocalRestored
    nonlocalRestoredSource (normalizedFacts nonlocalRestored.nearestFunctionEnv) "outer.inner" true,
  annotationCaseAt "nonlocal_unaffected_module_site" .nonlocalUnaffected
    nonlocalUnaffectedSource (normalizedFacts nonlocalWritten.moduleEnv) none "module" true,
  annotationCase "class_global_site" "internal-fixture" .classGlobal
    classGlobalSource (normalizedFacts globalWritten.moduleEnv) "GlobalWrite.method" false,
  annotationCase "class_nonlocal_site" "internal-fixture" .classNonlocal
    classNonlocalSource (normalizedFacts nonlocalWritten.nearestFunctionEnv)
      "outer.NonlocalWrite.method" false,
  annotationCaseAt "class_global_unaffected_function_site" .classGlobalUnaffected
    classGlobalUnaffectedSource (normalizedFacts globalWritten.nearestFunctionEnv)
      (some "outer") "function" true,
  annotationCaseAt "class_nonlocal_unaffected_module_site" .classNonlocalUnaffected
    classNonlocalUnaffectedSource (normalizedFacts nonlocalWritten.moduleEnv) none "module" true,
  annotationCase "list_outer_before_site" "internal-fixture" .listOuterBefore
    listOuterBeforeSource (normalizedFacts comprehensionObservation.after) "collect" true,
  annotationCase "list_outer_after_site" "internal-fixture" .listOuterAfter
    listOuterAfterSource (normalizedFacts comprehensionObservation.after) "collect" true,
  resolutionCase "list_after_resolution" .listAfter
    listAfterSource "list(values)" (afterComprehensionResolution comprehensionObservation),
  resolutionCase "list_first_iterable_resolution" .listFirstIterable
    listFirstIterableSource "list(values)" comprehensionObservation.firstIterable,
  resolutionCase "list_body_resolution" .listBody
    listBodySource "list(item)" comprehensionObservation.inside,
  resolutionCase "set_body_resolution" .setBody
    setBodySource "list(item)" comprehensionObservation.inside,
  resolutionCase "dict_body_resolution" .dictBody
    dictBodySource "list(item)" comprehensionObservation.inside,
  resolutionCase "generator_body_resolution" .generatorBody
    generatorBodySource "list(item)" comprehensionObservation.inside
]

def strictCases : List OracleCase := [
  publicTypeCase "global_before_public" .globalBefore globalBeforeSource
    initialDirected.moduleEnv "global_scope",
  publicTypeCase "global_after_write_public" .globalAfterWrite globalAfterSource
    globalWritten.moduleEnv "global_scope",
  publicTypeCase "global_restored_public" .globalRestored globalRestoredSource
    globalRestored.moduleEnv "global_scope",
  publicTypeCase "nonlocal_before_public" .nonlocalBefore nonlocalBeforeSource
    initialDirected.nearestFunctionEnv "outer.inner",
  publicTypeCase "nonlocal_after_write_public" .nonlocalAfterWrite nonlocalAfterSource
    nonlocalWritten.nearestFunctionEnv "outer.inner",
  publicTypeCase "nonlocal_restored_public" .nonlocalRestored nonlocalRestoredSource
    nonlocalRestored.nearestFunctionEnv "outer.inner",
  publicResolutionCase "list_after_public" .listAfter listAfterSource "list(values)"
    (afterComprehensionResolution comprehensionObservation) "collect",
  publicResolutionCase "list_first_iterable_public" .listFirstIterable
    listFirstIterableSource "list(values)" comprehensionObservation.firstIterable "collect",
  publicResolutionCase "list_body_public" .listBody listBodySource "list(item)"
    comprehensionObservation.inside "collect"
]

def cases : List OracleCase := internalCases ++ strictCases

def markerUnique (item : OracleCase) : Bool :=
  (item.source.splitOn item.marker).length == 2

def fieldsMatchKind (item : OracleCase) : Bool :=
  match item.observationKind with
  | .annotation =>
      item.mode == "internal-fixture" && item.expectedScope.isSome &&
        item.expectedResolution.isNone &&
        ((item.expectedScope == some "function" && item.expectedSymbol.isSome) ||
          (item.expectedScope == some "module" && item.expectedSymbol.isNone)) &&
        item.expectedOperator.isNone && item.expectedOriginal.isNone &&
        item.expectedReplacement.isNone
  | .resolution =>
      item.mode == "internal-fixture" && item.expectedScope.isNone &&
        item.expectedResolution.isSome && item.expectedSymbol.isNone &&
        item.expectedFacts.isEmpty && item.expectedOperator.isNone &&
        item.expectedOriginal.isNone && item.expectedReplacement.isNone
  | .publicCandidate =>
      item.mode == "strict" && item.expectedScope.isNone &&
        item.expectedResolution.isNone && item.expectedFacts.isEmpty &&
        item.expectedOperator.isSome && item.expectedOriginal.isSome &&
        item.expectedReplacement.isSome &&
        item.expectedPresent == item.expectedSymbol.isSome

def CaseValid (item : OracleCase) : Bool :=
  item.schema == 1 && !item.id.isEmpty && !item.source.isEmpty &&
    !item.marker.isEmpty && markerUnique item &&
    (item.expectedFacts == [] ||
      item.expectedFacts == ["direct:Sequence=typing.Sequence"]) &&
    fieldsMatchKind item

def fixedCasesPass : Bool :=
  cases.length == 29 && cases.all CaseValid

private def brokenComprehensionLeak : ComprehensionObservation :=
  { comprehensionObservation with after := initialEnv.set .source .shadowed }

private def brokenFirstIterableBinding : ComprehensionObservation :=
  { comprehensionObservation with firstIterable := .shadowed }

private def brokenGlobalCurrent : DirectedState :=
  writeDirected .normal .destination .shadowed initialDirected

private def brokenNonlocalModule : DirectedState :=
  writeDirected .global .destination .shadowed initialDirected

def comprehensionLeakSensitivity : Bool :=
  afterComprehensionResolution brokenComprehensionLeak !=
    afterComprehensionResolution comprehensionObservation

def firstIterableSensitivity : Bool :=
  brokenFirstIterableBinding.firstIterable != comprehensionObservation.firstIterable

def globalTargetSensitivity : Bool :=
  brokenGlobalCurrent != globalWritten

def nonlocalTargetSensitivity : Bool :=
  brokenNonlocalModule != nonlocalWritten

def siteEntrySensitivity : Bool :=
  initialEnv != initialEnv.set .destination .shadowed

def sensitivityPasses : Bool :=
  comprehensionLeakSensitivity && firstIterableSensitivity &&
    globalTargetSensitivity && nonlocalTargetSensitivity && siteEntrySensitivity

end HoiminOracle.AnnotationScope
