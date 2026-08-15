import Std

namespace HoiminOracle.ChangedTarget

structure LineRange where
  start : Nat
  stop : Nat
  deriving Repr, DecidableEq, BEq

def ValidRange (range : LineRange) : Prop :=
  0 < range.start ∧ range.start ≤ range.stop

def InRanges (ranges : List LineRange) (line : Nat) : Prop :=
  0 < line ∧ ∃ range ∈ ranges, ValidRange range ∧ range.start ≤ line ∧ line ≤ range.stop

instance (ranges : List LineRange) (line : Nat) : Decidable (InRanges ranges line) := by
  unfold InRanges ValidRange
  infer_instance

-- Extensionally, normalization is the positive one-based line set.  The bounded
-- executable representation below orders and deduplicates that set; overlapping
-- and adjacent input ranges therefore have one canonical representation.
def normalize (ranges : List LineRange) : Nat → Prop := InRanges ranges

instance (ranges : List LineRange) (line : Nat) : Decidable (normalize ranges line) := by
  unfold normalize
  infer_instance

def normalizedLines (maximum : Nat) (ranges : List LineRange) : List Nat :=
  (List.range (maximum + 1)).filter fun line => decide (normalize ranges line)

def normalizeLineSet (maximum : Nat) (lines : List Nat) : List Nat :=
  (List.range (maximum + 1)).filter fun line => line ∈ lines

def pushNormalizedLine (gap : Nat) (reversed : List LineRange) (line : Nat) : List LineRange :=
  match reversed with
  | [] => [⟨line, line⟩]
  | current :: rest =>
      if line ≤ current.stop + gap then
        { current with stop := max current.stop line } :: rest
      else
        ⟨line, line⟩ :: reversed

def normalizedRangesWithGap (gap maximum : Nat) (ranges : List LineRange) : List LineRange :=
  ((normalizedLines maximum ranges).foldl (pushNormalizedLine gap) []).reverse

def normalizedRanges (maximum : Nat) (ranges : List LineRange) : List LineRange :=
  normalizedRangesWithGap 1 maximum ranges

inductive ChangeKind where
  | added | modified | deleted | renamed | binary | untracked
  deriving Repr, DecidableEq, BEq

structure ChangeFact where
  kind : ChangeKind
  sourcePath : Option String := none
  destinationPath : Option String := none
  destinationRanges : List LineRange := []
  currentLineCount : Nat := 0
  deriving Repr, DecidableEq, BEq

def effectivePath (fact : ChangeFact) : Option String :=
  match fact.kind with
  | .deleted => fact.sourcePath
  | _ => fact.destinationPath

def excludedBy (fact : ChangeFact) (path : String) : Prop :=
  match fact.kind with
  | .deleted => fact.sourcePath = some path
  | .binary => effectivePath fact = some path
  | _ => False

instance (fact : ChangeFact) (path : String) : Decidable (excludedBy fact path) := by
  unfold excludedBy effectivePath
  split <;> infer_instance

def Excluded (facts : List ChangeFact) (path : String) : Prop :=
  ∃ fact ∈ facts, excludedBy fact path

def selectedBy (fact : ChangeFact) (path : String) (line : Nat) : Prop :=
  match fact.kind with
  | .added | .modified | .renamed =>
      fact.destinationPath = some path ∧ normalize fact.destinationRanges line
  | .untracked => fact.destinationPath = some path ∧ 0 < line ∧ line ≤ fact.currentLineCount
  | .deleted | .binary => False

def ChangedEligible (facts : List ChangeFact) (path : String) (line : Nat) : Prop :=
  ¬ Excluded facts path ∧ ∃ fact ∈ facts, selectedBy fact path line

structure Observation where
  path : String
  line : Nat
  symbol : Option String := none
  deriving Repr, DecidableEq, BEq

structure Selector where
  path : String
  ranges : List LineRange := []
  symbols : List String := []
  deriving Repr, DecidableEq, BEq

def symbolAllowed (selector : Selector) (symbol : Option String) : Prop :=
  selector.symbols = [] ∨ ∃ name ∈ selector.symbols, symbol = some name

instance (selector : Selector) (symbol : Option String) :
    Decidable (symbolAllowed selector symbol) := by
  unfold symbolAllowed
  infer_instance

def selectedExplicitly (selector : Selector) (observation : Observation) : Prop :=
  selector.path = observation.path ∧
    (selector.ranges = [] ∨ normalize selector.ranges observation.line) ∧
    symbolAllowed selector observation.symbol

def ExplicitEligible (selectors : List Selector) (observation : Observation) : Prop :=
  selectors = [] ∨ ∃ selector ∈ selectors, selectedExplicitly selector observation

def CombinedEligible (facts : List ChangeFact) (selectors : List Selector)
    (observation : Observation) : Prop :=
  ChangedEligible facts observation.path observation.line ∧
    ExplicitEligible selectors observation

instance (facts : List ChangeFact) (path : String) : Decidable (Excluded facts path) := by
  unfold Excluded
  infer_instance

instance (fact : ChangeFact) (path : String) (line : Nat) :
    Decidable (selectedBy fact path line) := by
  unfold selectedBy
  split <;> infer_instance

instance (facts : List ChangeFact) (path : String) (line : Nat) :
    Decidable (ChangedEligible facts path line) := by
  unfold ChangedEligible
  infer_instance

instance (selector : Selector) (observation : Observation) :
    Decidable (selectedExplicitly selector observation) := by
  unfold selectedExplicitly symbolAllowed
  infer_instance

instance (selectors : List Selector) (observation : Observation) :
    Decidable (ExplicitEligible selectors observation) := by
  unfold ExplicitEligible
  infer_instance

instance (facts : List ChangeFact) (selectors : List Selector) (observation : Observation) :
    Decidable (CombinedEligible facts selectors observation) := by
  unfold CombinedEligible
  infer_instance

end HoiminOracle.ChangedTarget
