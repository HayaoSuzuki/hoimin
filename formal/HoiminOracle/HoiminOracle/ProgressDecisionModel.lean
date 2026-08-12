import Std

namespace HoiminOracle.ProgressDecision

inductive Status
  | killed
  | survived
  | inconclusive
  deriving Repr, DecidableEq, BEq, Inhabited

structure Mutant where
  candidateId : Nat
  contentKey : Nat
  status : Status
  deriving Repr, DecidableEq, BEq

inductive Report
  | unusable
  | usable (mutants : List Mutant)
  deriving Repr, DecidableEq, BEq

inductive Eligibility
  | matching
  | different
  | duplicate
  deriving Repr, DecidableEq, BEq

inductive JoinMode
  | candidateId
  | contentKey
  deriving Repr, DecidableEq, BEq

inductive PairState
  | improving
  | regressing
  | stalled
  | indeterminate
  deriving Repr, DecidableEq, BEq

inductive LatestState
  | improving
  | regressing
  | stalled
  | saturated
  | indeterminate
  deriving Repr, DecidableEq, BEq

structure Counts where
  common : Nat := 0
  added : Nat := 0
  removed : Nat := 0
  ambiguous : Nat := 0
  inconclusive : Nat := 0
  improvements : Nat := 0
  regressions : Nat := 0
  carriedSurvivors : Nat := 0
  previousKilled : Nat := 0
  previousSurvived : Nat := 0
  currentKilled : Nat := 0
  currentSurvived : Nat := 0
  deriving Repr, DecidableEq, BEq

structure PairObservation where
  eligibility : Eligibility
  counts : Counts
  state : PairState
  deriving Repr, DecidableEq, BEq

inductive PairStep
  | unusableAdjacency
  | compared (observation : PairObservation)
  deriving Repr, DecidableEq, BEq

structure HistoryObservation where
  comparisons : List PairObservation
  latest : LatestState
  consecutiveStalls : Nat
  deriving Repr, DecidableEq, BEq

def Counts.add (left right : Counts) : Counts :=
  { common := left.common + right.common
    added := left.added + right.added
    removed := left.removed + right.removed
    ambiguous := left.ambiguous + right.ambiguous
    inconclusive := left.inconclusive + right.inconclusive
    improvements := left.improvements + right.improvements
    regressions := left.regressions + right.regressions
    carriedSurvivors := left.carriedSurvivors + right.carriedSurvivors
    previousKilled := left.previousKilled + right.previousKilled
    previousSurvived := left.previousSurvived + right.previousSurvived
    currentKilled := left.currentKilled + right.currentKilled
    currentSurvived := left.currentSurvived + right.currentSurvived }

def uniqueNats : List Nat → Bool
  | [] => true
  | value :: rest => !(rest.contains value) && uniqueNats rest

def sameNatSet (left right : List Nat) : Bool :=
  uniqueNats left && uniqueNats right &&
    left.length == right.length && left.all right.contains

def candidateIds (mutants : List Mutant) : List Nat :=
  mutants.map (·.candidateId)

def eligibility (previous current : List Mutant) : Eligibility :=
  let previousIds := candidateIds previous
  let currentIds := candidateIds current
  if !uniqueNats previousIds || !uniqueNats currentIds then .duplicate
  else if sameNatSet previousIds currentIds then .matching
  else .different

def joinMode : Eligibility → JoinMode
  | .matching => .candidateId
  | .different | .duplicate => .contentKey

def keyOf : JoinMode → Mutant → Nat
  | .candidateId, mutant => mutant.candidateId
  | .contentKey, mutant => mutant.contentKey

def keys (mode : JoinMode) (mutants : List Mutant) : List Nat :=
  (mutants.map (keyOf mode)).eraseDups

def duplicateKeys (mode : JoinMode) (mutants : List Mutant) : List Nat :=
  (keys mode mutants).filter fun candidate =>
    1 < (mutants.filter fun mutant => keyOf mode mutant == candidate).length

def ambiguousKeys
    (mode : JoinMode) (previous current : List Mutant) : List Nat :=
  (duplicateKeys mode previous ++ duplicateKeys mode current).eraseDups

def findByKey? (mode : JoinMode) (key : Nat) (mutants : List Mutant) : Option Mutant :=
  mutants.find? fun mutant => keyOf mode mutant == key

def isConclusive : Status → Bool
  | .killed | .survived => true
  | .inconclusive => false

def transitionCounts (before after : Status) : Counts :=
  if !isConclusive before || !isConclusive after then
    { common := 1, inconclusive := 1 }
  else
    { common := 1
      improvements := if before == .survived && after == .killed then 1 else 0
      regressions := if before == .killed && after == .survived then 1 else 0
      carriedSurvivors := if before == .survived && after == .survived then 1 else 0
      previousKilled := if before == .killed then 1 else 0
      previousSurvived := if before == .survived then 1 else 0
      currentKilled := if after == .killed then 1 else 0
      currentSurvived := if after == .survived then 1 else 0 }

def keyCounts
    (mode : JoinMode) (previous current : List Mutant) (key : Nat) : Counts :=
  match findByKey? mode key previous, findByKey? mode key current with
  | some before, some after => transitionCounts before.status after.status
  | some before, none =>
      { removed := 1
        inconclusive := if before.status == .inconclusive then 1 else 0 }
  | none, some after =>
      { added := 1
        inconclusive := if after.status == .inconclusive then 1 else 0 }
  | none, none => {}

def classifyPair
    (pairEligibility : Eligibility)
    (comparableCommon regressions improvements : Nat) : PairState :=
  if pairEligibility != .matching || comparableCommon = 0 then .indeterminate
  else if regressions > 0 then .regressing
  else if improvements > 0 then .improving
  else .stalled

def comparePair (previous current : List Mutant) : PairObservation :=
  let pairEligibility := eligibility previous current
  let mode := joinMode pairEligibility
  let ambiguous := ambiguousKeys mode previous current
  let comparableKeys :=
    (keys mode previous ++ keys mode current).eraseDups.filter fun key =>
      !(ambiguous.contains key)
  let base := comparableKeys.foldl
    (fun counts key => Counts.add counts (keyCounts mode previous current key))
    ({} : Counts)
  let counts := { base with ambiguous := ambiguous.length }
  let comparableCommon := counts.previousKilled + counts.previousSurvived
  { eligibility := pairEligibility
    counts
    state := classifyPair pairEligibility comparableCommon
      counts.regressions counts.improvements }

def pairStep : Report → Report → PairStep
  | .usable previous, .usable current => .compared (comparePair previous current)
  | _, _ => .unusableAdjacency

def pairSteps : List Report → List PairStep
  | previous :: current :: rest =>
      pairStep previous current :: pairSteps (current :: rest)
  | _ => []

def isStalledStep : PairStep → Bool
  | .compared observation => observation.state == .stalled
  | .unusableAdjacency => false

def stallCountStep (count : Nat) (step : PairStep) : Nat :=
  if isStalledStep step then count + 1 else 0

def trailingStalls (steps : List PairStep) : Nat :=
  steps.foldl stallCountStep 0

def comparisons (steps : List PairStep) : List PairObservation :=
  steps.filterMap fun
    | .compared observation => some observation
    | .unusableAdjacency => none

def baseLatest : Option PairStep → LatestState
  | some (.compared observation) =>
      match observation.state with
      | .improving => .improving
      | .regressing => .regressing
      | .stalled => .stalled
      | .indeterminate => .indeterminate
  | _ => .indeterminate

def latestWithPatience (steps : List PairStep) (patience : Nat) : LatestState :=
  if 0 < patience && patience ≤ trailingStalls steps then .saturated
  else baseLatest steps.getLast?

def foldPairStepsWithPatience
    (steps : List PairStep) (patience : Nat) : HistoryObservation :=
  { comparisons := comparisons steps
    latest := latestWithPatience steps patience
    consecutiveStalls := trailingStalls steps }

def compareHistory (reports : List Report) (patience : Nat) : HistoryObservation :=
  foldPairStepsWithPatience (pairSteps reports) patience

end HoiminOracle.ProgressDecision
