import HoiminOracle.TimeoutLimitProofs

namespace HoiminOracle.TimeoutLimit

def oneSecond : Nat := 1000000000
def maximum : Nat := 3153600000000000000
def minimumAuto : Nat := 5000000000
def maximumAutoBaseline : Nat := 1576799999500000000

structure Expected where
  accepted : Bool
  invalidField : Option String
  effectiveMutantTimeout : Nat
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat
  id : String
  mode : String
  input : Input
  expected : Expected
  deriving Repr, DecidableEq, BEq

def input
    (analyzer baseline total : Nat)
    (mutant : MutantTimeout := .auto) : Input := {
  analyzer
  baseline
  mutant
  total
  maximum
  minimumAuto
  oneSecond
}

def expected (item : Input) : Expected := {
  accepted := accepts item
  invalidField := invalidField item
  effectiveMutantTimeout := effectiveMutantTimeout item
}

def case (id : String) (item : Input) : OracleCase := {
  schema := 1
  id
  mode := "strict"
  input := item
  expected := expected item
}

def cases : List OracleCase := [
  case "defaults_auto" (input (30 * oneSecond) (60 * oneSecond) (300 * oneSecond)),
  case "analyzer_zero" (input 0 (60 * oneSecond) (300 * oneSecond) (.fixed minimumAuto)),
  case "analyzer_maximum" (input maximum (60 * oneSecond) (300 * oneSecond) (.fixed minimumAuto)),
  case "analyzer_over_maximum" (input (maximum + 1) (60 * oneSecond) (300 * oneSecond) (.fixed minimumAuto)),
  case "baseline_zero_fixed" (input (30 * oneSecond) 0 (300 * oneSecond) (.fixed minimumAuto)),
  case "baseline_maximum_fixed" (input (30 * oneSecond) maximum (300 * oneSecond) (.fixed minimumAuto)),
  case "baseline_over_maximum_fixed" (input (30 * oneSecond) (maximum + 1) (300 * oneSecond) (.fixed minimumAuto)),
  case "auto_baseline_maximum" (input (30 * oneSecond) maximumAutoBaseline (300 * oneSecond)),
  case "auto_baseline_derived_over" (input (30 * oneSecond) (maximumAutoBaseline + 1) (300 * oneSecond)),
  case "fixed_mutant_zero" (input (30 * oneSecond) (60 * oneSecond) (300 * oneSecond) (.fixed 0)),
  case "fixed_mutant_maximum" (input (30 * oneSecond) (60 * oneSecond) (300 * oneSecond) (.fixed maximum)),
  case "fixed_mutant_over_maximum" (input (30 * oneSecond) (60 * oneSecond) (300 * oneSecond) (.fixed (maximum + 1))),
  case "total_zero" (input (30 * oneSecond) (60 * oneSecond) 0 (.fixed minimumAuto)),
  case "total_maximum" (input (30 * oneSecond) (60 * oneSecond) maximum (.fixed minimumAuto)),
  case "total_over_maximum" (input (30 * oneSecond) (60 * oneSecond) (maximum + 1) (.fixed minimumAuto))
]

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 &&
    item.mode == "strict" &&
    item.input.maximum == maximum &&
    item.input.minimumAuto == minimumAuto &&
    item.input.oneSecond == oneSecond &&
    expected item.input == item.expected

def brokenInputOnlyAutoAccepts (item : Input) : Bool :=
  durationValid item.maximum item.analyzer &&
    durationValid item.maximum item.baseline &&
    durationValid item.maximum item.total

def zeroSensitivity : Bool :=
  !durationValid maximum 0 && durationValid maximum 1

def inclusiveSensitivity : Bool :=
  durationValid maximum maximum && !durationValid maximum (maximum + 1)

def derivedAutoSensitivity : Bool :=
  let item := input (30 * oneSecond) (maximumAutoBaseline + 1) (300 * oneSecond)
  brokenInputOnlyAutoAccepts item && !accepts item &&
    invalidField item == some "baseline_timeout"

def attributionSensitivity : Bool :=
  invalidField (input 0 (60 * oneSecond) (300 * oneSecond) (.fixed minimumAuto)) ==
      some "analyzer_timeout" &&
    invalidField (input (30 * oneSecond) (60 * oneSecond) (300 * oneSecond) (.fixed (maximum + 1))) ==
      some "mutant_timeout" &&
    invalidField (input (30 * oneSecond) (60 * oneSecond) (maximum + 1) (.fixed minimumAuto)) ==
      some "total_timeout"

def sensitivityPasses : Bool :=
  zeroSensitivity && inclusiveSensitivity && derivedAutoSensitivity && attributionSensitivity

def fixedCasesPass : Bool := cases.all caseSafe

end HoiminOracle.TimeoutLimit
