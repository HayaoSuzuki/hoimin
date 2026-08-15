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

def sensitivity : List (String × Bool) :=
  [ ("adjacent_merge", strictOverlap)
  , ("gap_not_merged", decide (¬ InRanges [range 2 2, range 4 4] 3))
  , ("destination_coordinates", decide (InRanges [range 7 7] 7 ∧ ¬ InRanges [range 2 2] 7))
  , ("deleted_binary_exclusion", strictDeletedBinaryRejected)
  , ("rename_destination", decide (strictRenameDestination && strictRenameSourceRejected))
  , ("untracked_last_line", strictUntrackedLastLine)
  , ("intersection_not_union", decide (¬ CombinedEligible [modified "pkg/a.py" [range 2 2]]
      [{ path := "pkg/a.py", ranges := [range 4 4] }] (observation "pkg/a.py" 2)))
  , ("symbol_retained", decide (¬ CombinedEligible [modified "pkg/a.py" [range 2 8]]
      [{ path := "pkg/a.py", symbols := ["Widget.run"] }]
      (observation "pkg/a.py" 5 (some "Other.run"))))
  , ("normalized_path_membership", decide ("pkg/a.py" = "pkg/./a.py".replace "/./" "/"))
  , ("later_section_independent", decide (ChangedEligible
      [modified "bad.txt" [range 0 0], modified "pkg/good.py" [range 3 3]] "pkg/good.py" 3)) ]

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
