import HoiminOracle.CleanupCapabilityProofs

namespace HoiminOracle.CleanupCapability

inductive CorrespondenceMode where
  | strict
  | internalFixture
  | modelOnly
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

structure Observation where
  result : Phase
  outsideWritable : Bool
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

structure OracleCase where
  schema : Nat
  id : String
  mode : CorrespondenceMode
  scenario : String
  strategy : Strategy
  schedule : List Event
  expected : Observation
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

def observe (state : State) : Observation :=
  { result := state.phase, outsideWritable := state.outsideWritable }

def fixedCases : List OracleCase := [
  { schema := 1
    id := "stable_entry"
    mode := .strict
    scenario := "stable-entry"
    strategy := .postInspection
    schedule := [.inspect, .bind, .effect]
    expected := { result := .complete, outsideWritable := false } },
  { schema := 1
    id := "preexisting_outside_link"
    mode := .strict
    scenario := "preexisting-outside-link"
    strategy := .postInspection
    schedule := [.swap, .inspect, .bind, .effect]
    expected := { result := .complete, outsideWritable := false } },
  { schema := 1
    id := "entry_swap_after_inspection"
    mode := .internalFixture
    scenario := "entry-swap-after-inspection"
    strategy := .postInspection
    schedule := [.inspect, .swap, .bind, .effect]
    expected := { result := .rejected, outsideWritable := false } },
  { schema := 1
    id := "entry_swap_after_binding"
    mode := .modelOnly
    scenario := "entry-swap-after-binding"
    strategy := .postInspection
    schedule := [.inspect, .bind, .swap, .effect]
    expected := { result := .complete, outsideWritable := false } },
  { schema := 1
    id := "retained_wrapper_swap"
    mode := .internalFixture
    scenario := "retained-wrapper-swap"
    strategy := .retained
    schedule := [.inspect, .swap, .bind, .effect]
    expected := { result := .complete, outsideWritable := false } }
]

def casePasses (item : OracleCase) : Bool :=
  item.schema == 1 && observe (run item.strategy item.schedule) == item.expected

def casesPass : Bool := fixedCases.all casePasses

def caseIdsUnique : Bool := decide (fixedCases.map (·.id)).Nodup

def corpusContractValid : Bool :=
  fixedCases.length == 5 &&
    (fixedCases.filter fun item => item.mode == .strict).length == 2 &&
    (fixedCases.filter fun item => item.mode == .internalFixture).length == 2 &&
    (fixedCases.filter fun item => item.mode == .modelOnly).length == 1 &&
    fixedCases.all fun item => item.schema == 1 && !item.scenario.isEmpty

def retainedAtomicitySensitivity : Bool :=
  !(run .retained [.inspect, .swap, .bind, .effect]).outsideWritable &&
    (brokenRun .retained [.inspect, .swap, .bind, .effect]).outsideWritable

def postInspectionAtomicitySensitivity : Bool :=
  !(run .postInspection [.inspect, .swap, .bind, .effect]).outsideWritable &&
    (brokenRun .postInspection [.inspect, .swap, .bind, .effect]).outsideWritable

def sensitivityPasses : Bool :=
  retainedAtomicitySensitivity && postInspectionAtomicitySensitivity

end HoiminOracle.CleanupCapability
