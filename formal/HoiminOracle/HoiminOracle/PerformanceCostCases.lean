import HoiminOracle.PerformanceCostProofs

namespace HoiminOracle.PerformanceCost

structure Case where
  id : String
  family : String
  events : List Event := []
  queries : List Nat := []
  aliases : Nat := 0
  annotations : Nat := 0
  selected : Bool := false
  source : String := ""
  operator : String := "binary_add_sub"
  limit : Nat := 1000
  candidates : Nat := 0
  truncated : Bool := false
  cloneCalls : Nat := 0
  cloneEntries : Nat := 0
  replacementBuilds : Nat := 0
  replacementBytes : Nat := 0

-- 8 is the boundary and N; adjacent values and 2N/4N are independent rows.
def sizes : List Nat := [0, 1, 7, 8, 9, 16, 32]
def effectAt (index : Nat) : Effect :=
  match index % 3 with
  | 0 => .maybeBind
  | 1 => .bind
  | _ => .unknown

def bindingCases : List Case := sizes.flatMap fun n =>
  ["ordered", "duplicate", "nonmonotone"].map fun shape =>
    let events := (List.range n).map fun index =>
      { offset := if shape == "duplicate" then index / 2 + 1
          else if shape == "nonmonotone" then n - index else index + 1
        effect := effectAt index : Event }
    { id := s!"binding-{shape}-{n}", family := "binding", events
      queries := List.range (n + 2) }

def annotationSource (n : Nat) : String :=
  String.join ((List.range n).map fun i => s!"from typing import List as Alias{i}\n") ++
  "from typing import Sequence\n" ++
  String.join ((List.range n).map fun i => s!"value{i}: list[int]\n")

def annotationCases : List Case := sizes.flatMap fun n =>
  [false, true].map fun selected =>
    { id := s!"annotation-{selected}-{n}", family := "annotation"
      aliases := n, annotations := n, selected, source := annotationSource n
      operator := if selected then "type_list_sequence" else "binary_add_sub"
      candidates := if selected then n else 0 }

def flatSource (n : Nat) : String :=
  "value = [" ++ String.join (List.replicate n "0, ") ++ "]\n"

def replacementCases : List Case := sizes.flatMap fun n =>
  ([false, true].map fun selected =>
    { id := s!"replacement-flat-{selected}-{n}", family := "replacement", selected
      source := flatSource n
      operator := if selected then "collection_list_tuple" else "binary_add_sub"
      limit := 1, candidates := if selected then 1 else 0
      replacementBuilds := if selected then 1 else 0
      replacementBytes := filteredReplacementBytes selected [3 * n + 2] }) ++
  [{ id := s!"replacement-nested-false-{n}", family := "replacement", limit := 1
     source := "value = " ++ String.join (List.replicate (max 1 n) "[") ++ "0" ++
       String.join (List.replicate (max 1 n) "]") ++ "\n" }]

def cases : List Case := bindingCases ++ annotationCases ++ replacementCases

def uniqueOffsets (events : List Event) : Nat :=
  ((events.map Event.offset).eraseDups).length

def linearWitness : Option Nat := (List.range 33).find? fun n =>
  queryBound n n < linearVisits n n

def cloneWitness : Option Nat := (List.range 33).find? fun n => 0 < cloneEntries n n

def replacementWitness : Option Nat := (List.range 33).find? fun n =>
  filteredReplacementBytes false [n] < eagerReplacementBytes [n]

def sensitivity : Bool :=
  linearWitness == some 3 && cloneWitness == some 1 && replacementWitness == some 1 &&
  cases.any (fun c => c.family == "binding" &&
    queryBound (uniqueOffsets c.events) c.queries.length < linearVisits c.events.length c.queries.length) &&
  cases.any (fun c => c.family == "annotation" && c.selected && 0 < c.annotations) &&
  cases.any (fun c => c.family == "replacement" && !c.selected && !c.source.isEmpty)

theorem smallest_broken_cost_witnesses : sensitivity = true := by native_decide

theorem boundary_build_costs :
    (sizes.map buildUpdates) = [0, 1, 28, 32, 45, 80, 192] := by native_decide

theorem boundary_lookup_costs :
    (sizes.map halfSteps) = [0, 1, 3, 4, 4, 5, 6] := by native_decide

end HoiminOracle.PerformanceCost
