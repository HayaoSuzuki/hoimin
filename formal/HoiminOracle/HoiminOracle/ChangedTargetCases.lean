import HoiminOracle.ChangedTargetProofs

namespace HoiminOracle.ChangedTarget

def range (start stop : Nat) : LineRange := ⟨start, stop⟩

def modified (path : String) (ranges : List LineRange) : ChangeFact where
  kind := .modified
  destinationPath := some path
  destinationRanges := ranges

def renamed (source destination : String) (ranges : List LineRange) : ChangeFact where
  kind := .renamed
  sourcePath := some source
  destinationPath := some destination
  destinationRanges := ranges

def deleted (path : String) : ChangeFact where
  kind := .deleted
  sourcePath := some path

def binary (path : String) : ChangeFact where
  kind := .binary
  destinationPath := some path

def untracked (path : String) (lines : Nat) : ChangeFact where
  kind := .untracked
  destinationPath := some path
  currentLineCount := lines

def observation (path : String) (line : Nat) (symbol : Option String := none) : Observation :=
  ⟨path, line, symbol⟩

def strictOverlap : Bool := decide <|
  ChangedEligible [modified "pkg/a.py" [range 2 4, range 4 6, range 7 7]] "pkg/a.py" 7

def strictIntersection : Bool := decide <|
  CombinedEligible [modified "pkg/a.py" [range 2 8]]
    [{ path := "pkg/a.py", ranges := [range 4 5] }]
    (observation "pkg/a.py" 5)

def strictSymbol : Bool := decide <|
  CombinedEligible [modified "pkg/a.py" [range 2 8]]
    [{ path := "pkg/a.py", symbols := ["Widget.run"] }]
    (observation "pkg/a.py" 5 (some "Widget.run"))

def strictRenameDestination : Bool := decide <|
  ChangedEligible [renamed "pkg/old.py" "pkg/new.py" [range 2 2]] "pkg/new.py" 2

def strictRenameSourceRejected : Bool := decide <|
  ¬ ChangedEligible [renamed "pkg/old.py" "pkg/new.py" [range 2 2]] "pkg/old.py" 2

def strictDeletedBinaryRejected : Bool := decide <|
  ¬ ChangedEligible [modified "pkg/a.py" [range 1 3], deleted "pkg/a.py"] "pkg/a.py" 2 ∧
  ¬ ChangedEligible [modified "pkg/b.py" [range 1 3], binary "pkg/b.py"] "pkg/b.py" 2

def strictUntrackedLastLine : Bool := decide <|
  ChangedEligible [untracked "pkg/new.py" 2] "pkg/new.py" 2 ∧
  ¬ ChangedEligible [untracked "pkg/new.py" 2] "pkg/new.py" 3

def strictDiffBaseComposition : Bool := decide <|
  ChangedEligible [modified "pkg/a.py" [range 2 3]] "pkg/a.py" 2 ∧
  ChangedEligible [modified "pkg/a.py" [range 2 3]] "pkg/a.py" 3

structure AuditCase where
  id : String
  mode : String
  scenario : String
  path : Option String
  facts : List ChangeFact := []
  selectors : List Selector := []
  symbol : Option String := none
  maximum : Nat := 0
  deriving Inhabited

def AuditCase.eligibleLines (case : AuditCase) : List Nat :=
  match case.path with
  | none => []
  | some path => (List.range (case.maximum + 1)).filter fun line =>
      decide (CombinedEligible case.facts case.selectors (observation path line case.symbol))

def auditCases : List AuditCase :=
  [ { id := "modified-overlap", mode := "strict", scenario := "modified_overlap",
      path := some "pkg/a.py", facts := [modified "pkg/a.py" [range 2 4, range 4 6, range 7 7]], maximum := 8 }
  , { id := "explicit-line", mode := "strict", scenario := "explicit_intersection",
      path := some "pkg/a.py", facts := [modified "pkg/a.py" [range 2 8]],
      selectors := [{ path := "pkg/a.py", ranges := [range 4 5] }], maximum := 8 }
  , { id := "symbol-line", mode := "strict", scenario := "symbol_intersection",
      path := some "pkg/a.py", facts := [modified "pkg/a.py" [range 3 3]],
      selectors := [{ path := "pkg/a.py", symbols := ["Widget.run"] }],
      symbol := some "Widget.run", maximum := 6 }
  , { id := "rename-destination", mode := "strict", scenario := "rename",
      path := some "pkg/new.py", facts := [renamed "pkg/old.py" "pkg/new.py" [range 2 2]], maximum := 3 }
  , { id := "deleted-binary", mode := "strict", scenario := "excluded",
      path := some "pkg/deleted.py",
      facts := [modified "pkg/deleted.py" [range 1 1], deleted "pkg/deleted.py",
        binary "pkg/deleted.py"], maximum := 1 }
  , { id := "untracked-unterminated", mode := "strict", scenario := "untracked",
      path := some "pkg/new.py", facts := [untracked "pkg/new.py" 2], maximum := 3 }
  , { id := "diff-base-worktree", mode := "strict", scenario := "diff_base",
      path := some "pkg/a.py", facts := [modified "pkg/a.py" [range 2 2], modified "pkg/a.py" [range 3 3]], maximum := 4 }
  , { id := "hostile-parser", mode := "internal-fixture", scenario := "parser_isolation",
      path := some "pkg/good.py", facts := [modified "bad.txt" [range 0 0], modified "pkg/good.py" [range 3 3]], maximum := 4 }
  , { id := "non-utf8-path", mode := "model-only", scenario := "non_utf8_path", path := none }
  , { id := "git-failure", mode := "infrastructure-error", scenario := "git_failure", path := none } ]

-- Explicit faulty transports.  Each family is checked against the canonical
-- observation, rather than merely re-evaluating the correct predicate.
def adjacentInput : List LineRange := [range 2 3, range 4 4]
def gapInput : List LineRange := [range 2 2, range 4 4]
def brokenNoAdjacentMerge : List LineRange := normalizedRangesWithGap 0 4 adjacentInput
def brokenMergeGap : List LineRange := normalizedRangesWithGap 2 4 gapInput

def destinationCoordinates (destination _source : List LineRange) (line : Nat) : Bool :=
  decide (InRanges destination line)
def brokenOldCoordinates (_destination source : List LineRange) (line : Nat) : Bool :=
  decide (InRanges source line)

def eligibleObservation (facts : List ChangeFact) (path : String) (line : Nat) : Bool :=
  decide (ChangedEligible facts path line)
def brokenRetainExcluded (facts : List ChangeFact) (path : String) (line : Nat) : Bool :=
  decide (∃ fact ∈ facts, selectedBy fact path line)

def renamePath (fact : ChangeFact) : Option String := fact.destinationPath
def brokenRenameSourcePath (fact : ChangeFact) : Option String := fact.sourcePath

def untrackedBoundary (count line : Nat) : Bool := decide (0 < line ∧ line ≤ count)
def brokenDropUnterminatedLast (count line : Nat) : Bool := decide (0 < line ∧ line < count)

def combinedObservation (facts : List ChangeFact) (selectors : List Selector)
    (item : Observation) : Bool := decide (CombinedEligible facts selectors item)
def brokenUnion (facts : List ChangeFact) (selectors : List Selector)
    (item : Observation) : Bool :=
  decide (ChangedEligible facts item.path item.line ∨ ExplicitEligible selectors item)

def brokenDropSymbols (facts : List ChangeFact) (selectors : List Selector)
    (item : Observation) : Bool :=
  let stripped := selectors.map fun selector => { selector with symbols := [] }
  decide (CombinedEligible facts stripped item)

def normalizedTransportPath (path : String) : String := path.replace "/./" "/"
def brokenRawTransportPath (path : String) : String := path

def laterSection (sections : List AuditCase) : List Nat :=
  sections.getLast?.map AuditCase.eligibleLines |>.getD []
def brokenContaminatedLaterSection (_sections : List AuditCase) : List Nat := []

def sensitivity : List (String × Bool) :=
  [ ("adjacent_merge", decide (brokenNoAdjacentMerge ≠ normalizedRanges 4 adjacentInput))
  , ("gap_not_merged", decide (brokenMergeGap ≠ normalizedRanges 4 gapInput))
  , ("destination_coordinates", decide
      (destinationCoordinates [range 7 7] [range 2 2] 7 !=
       brokenOldCoordinates [range 7 7] [range 2 2] 7))
  , ("deleted_binary_exclusion", decide
      (eligibleObservation [modified "pkg/a.py" [range 1 3], deleted "pkg/a.py"] "pkg/a.py" 2 !=
       brokenRetainExcluded [modified "pkg/a.py" [range 1 3], deleted "pkg/a.py"] "pkg/a.py" 2))
  , ("rename_destination", decide
      (renamePath (renamed "pkg/old.py" "pkg/new.py" [range 2 2]) !=
       brokenRenameSourcePath (renamed "pkg/old.py" "pkg/new.py" [range 2 2])))
  , ("untracked_last_line", decide
      (untrackedBoundary 2 2 != brokenDropUnterminatedLast 2 2))
  , ("intersection_not_union", decide
      (combinedObservation [modified "pkg/a.py" [range 2 2]]
        [{ path := "pkg/a.py", ranges := [range 4 4] }] (observation "pkg/a.py" 2) !=
       brokenUnion [modified "pkg/a.py" [range 2 2]]
        [{ path := "pkg/a.py", ranges := [range 4 4] }] (observation "pkg/a.py" 2)))
  , ("symbol_retained", decide
      (combinedObservation [modified "pkg/a.py" [range 2 8]]
        [{ path := "pkg/a.py", symbols := ["Widget.run"] }]
        (observation "pkg/a.py" 5 (some "Other.run")) !=
       brokenDropSymbols [modified "pkg/a.py" [range 2 8]]
        [{ path := "pkg/a.py", symbols := ["Widget.run"] }]
        (observation "pkg/a.py" 5 (some "Other.run"))))
  , ("normalized_path_membership", decide
      (normalizedTransportPath "pkg/./a.py" != brokenRawTransportPath "pkg/./a.py"))
  , ("later_section_independent", decide
      (laterSection [auditCases[0]!, auditCases[7]!] !=
       brokenContaminatedLaterSection [auditCases[0]!, auditCases[7]!])) ]

def boundedRanges : List LineRange :=
  (List.range 4).flatMap fun start => (List.range 4).map fun stop => range start stop

def boundedRangeSets : List (List LineRange) :=
  [[]] ++ boundedRanges.map (fun item => [item]) ++
    boundedRanges.flatMap fun left => boundedRanges.map fun right => [left, right]

def exploredMembershipStates : Nat := boundedRangeSets.length * 5

def boundedStatePass (ranges : List LineRange) (line : Nat) : Bool :=
  decide ((line ∈ normalizedLines 4 ranges) ↔ (line ≤ 4 ∧ normalize ranges line)) &&
    decide (normalizeLineSet 4 (normalizedLines 4 ranges) = normalizedLines 4 ranges)

def boundedExplorationPass : Bool :=
  boundedRangeSets.all fun ranges => (List.range 5).all fun line => boundedStatePass ranges line

def fixedCases : List (String × Bool) :=
  [ ("overlap_adjacent", strictOverlap)
  , ("explicit_intersection", strictIntersection)
  , ("symbol_intersection", strictSymbol)
  , ("rename_destination", strictRenameDestination)
  , ("rename_source_rejected", strictRenameSourceRejected)
  , ("deleted_binary_rejected", strictDeletedBinaryRejected)
  , ("untracked_last_line", strictUntrackedLastLine)
  , ("diff_base_composition", strictDiffBaseComposition) ]

end HoiminOracle.ChangedTarget
