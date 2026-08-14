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
  path : String := "pkg/sample.py"
  spanStart : Nat := 0
  spanLength : Nat := 1
  original : String := "original"
  replacement : String := "replacement"
  operatorKey : String := "operator"
  line : Nat := 1
  column : Nat := 0
  deriving Repr, DecidableEq, BEq

def productionBefore (left right : Candidate) : Bool :=
  left.orderKey < right.orderKey ||
    (left.orderKey == right.orderKey && left.emissionIndex < right.emissionIndex)

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

def boundReference (complete : List Candidate) (limit : Nat) : Discovery :=
  { candidates := complete.take limit
    truncated := limit < complete.length }

def bounded (items : List Candidate) (limit : Nat) : Discovery :=
  boundReference (reference items) limit

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
    let complete := state.candidates ++ reference items
    let current := boundReference complete limit
    { candidates := current.candidates
      truncated := current.truncated
      targetsRead := state.targetsRead + 1
      spoolFinished := current.truncated }

def discoverTargetsFrom
    (initial : TargetState) (targets : List (List Candidate)) (limit : Nat) : TargetState :=
  match targets with
  | [] => initial
  | items :: rest => discoverTargetsFrom (targetStep initial items limit) rest limit

def discoverTargets (targets : List (List Candidate)) (limit : Nat) : TargetState :=
  let result := discoverTargetsFrom {} targets limit
  { result with spoolFinished := result.spoolFinished || !targets.isEmpty }

def targetReference (targets : List (List Candidate)) : List Candidate :=
  targets.flatMap reference

def retainedByIdentity (retained : List Candidate) (candidate : Candidate) : Bool :=
  retained.any fun item => item.identity == candidate.identity

end HoiminOracle.BoundedCandidateDiscovery
