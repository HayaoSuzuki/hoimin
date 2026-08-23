import HoiminOracle.TopBudgetProjectionProofs

namespace HoiminOracle.TopBudgetProjection

def ticksPerSecond : Nat := 1000000000

def durationMax : Nat := 18446744073709551615999999999

structure Expected where
  effectiveMutantTimeout : Nat
  waves : Nat
  projectedCapacity : Nat
  shortfall : Bool
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat
  id : String
  mode : String
  input : Input
  expected : Expected
  deriving Repr, DecidableEq, BEq

def input
    (selected jobs baseline remaining : Nat)
    (timeoutMode : TimeoutMode)
    (plannedTotalTimeout : Nat := 10000000000) : Input := {
  selected
  jobs
  plannedTotalTimeout
  baseline
  timeoutMode
  remaining
  durationMax
  ticksPerSecond
}

def cases : List OracleCase := [
  { schema := 1
    id := "zero_selection"
    mode := "strict"
    input := input 0 4 1 0 (.fixed 1)
    expected := {
      effectiveMutantTimeout := 1
      waves := 0
      projectedCapacity := 0
      shortfall := false } },
  { schema := 1
    id := "divisible_parallel"
    mode := "strict"
    input := input 8 4 1 4000000000 (.fixed 2000000000)
    expected := {
      effectiveMutantTimeout := 2000000000
      waves := 2
      projectedCapacity := 4000000000
      shortfall := false } },
  { schema := 1
    id := "ceiling_parallel"
    mode := "strict"
    input := input 9 4 1 5000000000 (.fixed 2000000000)
    expected := {
      effectiveMutantTimeout := 2000000000
      waves := 3
      projectedCapacity := 6000000000
      shortfall := true } },
  { schema := 1
    id := "equal_capacity"
    mode := "strict"
    input := input 2 1 1 6000000000 (.fixed 3000000000)
    expected := {
      effectiveMutantTimeout := 3000000000
      waves := 2
      projectedCapacity := 6000000000
      shortfall := false } },
  { schema := 1
    id := "auto_minimum"
    mode := "strict"
    input := input 1 1 1000000000 5000000000 .auto
    expected := {
      effectiveMutantTimeout := 5000000000
      waves := 1
      projectedCapacity := 5000000000
      shortfall := false } },
  { schema := 1
    id := "auto_scaled"
    mode := "strict"
    input := input 2 1 8000000000 33000000000 .auto
    expected := {
      effectiveMutantTimeout := 17000000000
      waves := 2
      projectedCapacity := 34000000000
      shortfall := true } },
  { schema := 1
    id := "large_wave_exact"
    mode := "strict"
    input := input 4294967296 1 1 4294967295 (.fixed 1)
    expected := {
      effectiveMutantTimeout := 1
      waves := 4294967296
      projectedCapacity := 4294967296
      shortfall := true } },
  { schema := 1
    id := "duration_saturation"
    mode := "strict"
    input := input 10000000000 1 1 durationMax (.fixed 3153600000000000000)
    expected := {
      effectiveMutantTimeout := 3153600000000000000
      waves := 10000000000
      projectedCapacity := durationMax
      shortfall := false } }
]

def observed (item : Input) : Expected :=
  let projection := project item
  { effectiveMutantTimeout := projection.effectiveMutantTimeout
    waves := projection.waves
    projectedCapacity := projection.projectedCapacity
    shortfall := projection.shortfall }

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 &&
    item.mode == "strict" &&
    item.input.jobs > 0 &&
    item.input.durationMax == durationMax &&
    observed item.input == item.expected

def brokenFloorWaves (selected jobs : Nat) : Nat := selected / jobs

def brokenInclusiveShortfall (capacity remaining : Nat) : Bool :=
  capacity >= remaining

def brokenU32LimitedCapacity (maximum timeout waveCount : Nat) : Nat :=
  if waveCount > 4294967295 then maximum
  else saturatingMul maximum timeout waveCount

def brokenAutoWithoutIncrement (maximum ticksPerSecond baseline : Nat) : Nat :=
  max (min maximum (5 * ticksPerSecond))
    (saturatingMul maximum baseline 2)

def brokenAutoWithoutMinimum (maximum ticksPerSecond baseline : Nat) : Nat :=
  saturatingAdd maximum
    (saturatingMul maximum baseline 2) ticksPerSecond

def floorSensitivity : Bool :=
  waves 9 4 == 3 && brokenFloorWaves 9 4 == 2

def equalitySensitivity : Bool :=
  !(6000000000 > 6000000000) &&
    brokenInclusiveShortfall 6000000000 6000000000

def u32CapacitySensitivity : Bool :=
  saturatingMul durationMax 1 4294967296 == 4294967296 &&
    brokenU32LimitedCapacity durationMax 1 4294967296 == durationMax

def autoSensitivity : Bool :=
  autoTimeout durationMax ticksPerSecond 8000000000 == 17000000000 &&
    brokenAutoWithoutIncrement durationMax ticksPerSecond 8000000000 == 16000000000 &&
    autoTimeout durationMax ticksPerSecond 1000000000 == 5000000000 &&
    brokenAutoWithoutMinimum durationMax ticksPerSecond 1000000000 == 3000000000

def sensitivityPasses : Bool :=
  floorSensitivity && equalitySensitivity && u32CapacitySensitivity && autoSensitivity

def fixedCasesPass : Bool := cases.all caseSafe

end HoiminOracle.TopBudgetProjection
