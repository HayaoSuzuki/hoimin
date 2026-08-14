import Std

namespace HoiminOracle.BoundedCandidateDiscovery

inductive Producer
  | token
  | ast
  | annotation
  deriving Repr, DecidableEq, BEq

structure Candidate where
  identity : Nat
  orderKey : Nat
  producer : Producer
  eligible : Bool := true
  emissionIndex : Nat := 0
  deriving Repr, DecidableEq, BEq

def productionBefore (left right : Candidate) : Bool :=
  left.orderKey < right.orderKey ||
    (left.orderKey == right.orderKey && left.emissionIndex ≤ right.emissionIndex)

def insertCandidate (candidate : Candidate) : List Candidate → List Candidate
  | [] => [candidate]
  | head :: tail =>
      if productionBefore candidate head then candidate :: head :: tail
      else head :: insertCandidate candidate tail

def productionSort (items : List Candidate) : List Candidate :=
  items.foldl (fun sorted candidate => insertCandidate candidate sorted) []

def deduplicate (items : List Candidate) : List Candidate :=
  items.foldl (fun retained candidate =>
    if retained.any (fun prior => prior.identity == candidate.identity) then retained
    else retained ++ [candidate]) []

def reference (items : List Candidate) : List Candidate :=
  deduplicate (productionSort (items.filter Candidate.eligible))

structure Discovery where
  candidates : List Candidate
  truncated : Bool
  deriving Repr, DecidableEq, BEq

def bounded (items : List Candidate) (limit : Nat) : Discovery :=
  let complete := reference items
  { candidates := complete.take limit
    truncated := limit < complete.length }

def producerWindow (items : List Candidate) (limit : Nat) : List Candidate :=
  (reference items).take (limit + 1)

def mergeProducerWindows
    (token ast annotation : List Candidate) (limit : Nat) : Discovery :=
  let retained :=
    producerWindow token limit ++ producerWindow ast limit ++
      producerWindow annotation limit
  let result := bounded retained limit
  { result with
    truncated := result.truncated ||
      limit < (reference token).length ||
      limit < (reference ast).length ||
      limit < (reference annotation).length }

def sequences (count : Nat) : List Nat :=
  (List.range count).map fun index => index + 1

structure TargetState where
  candidates : List Candidate := []
  truncated : Bool := false
  targetsRead : Nat := 0
  spoolFinished : Bool := false
  deriving Repr, DecidableEq, BEq

def targetStep (state : TargetState) (items : List Candidate) (limit : Nat) : TargetState :=
  if state.truncated then state
  else
    let remaining := limit - state.candidates.length
    let current := bounded items remaining
    { candidates := (state.candidates ++ current.candidates).take limit
      truncated := current.truncated
      targetsRead := state.targetsRead + 1
      spoolFinished := current.truncated }

def discoverTargetsFrom
    (initial : TargetState) (targets : List (List Candidate)) (limit : Nat) : TargetState :=
  targets.foldl (fun state items => targetStep state items limit) initial

def discoverTargets (targets : List (List Candidate)) (limit : Nat) : TargetState :=
  let result := discoverTargetsFrom {} targets limit
  { result with spoolFinished := result.spoolFinished || !targets.isEmpty }

end HoiminOracle.BoundedCandidateDiscovery
