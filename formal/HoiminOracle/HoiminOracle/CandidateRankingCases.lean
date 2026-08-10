import HoiminOracle.CandidateRankingProofs

namespace HoiminOracle.CandidateRanking

def candidate
    (id : String) (path : Path) (line column : Nat)
    (operatorKey : String) (operatorClass : OperatorClass)
    (explicitLine explicitSymbol changedLine : Bool := false) : Candidate := {
  id, path, line, column, operatorKey, operatorClass,
  explicitLine, explicitSymbol, changedLine
}

def selectorCandidates : List Candidate := [
  candidate "selector" .beta 7 4 "compare_eq_ne" .highValueControl true true true,
  candidate "plain" .alpha 2 2 "binary_add_sub" .arithmetic
]

def diverseCandidates : List Candidate := [
  candidate "alpha_2" .alpha 2 2 "compare_eq_ne" .highValueControl,
  candidate "beta_1" .beta 1 2 "compare_eq_ne" .highValueControl,
  candidate "alpha_1" .alpha 1 2 "compare_eq_ne" .highValueControl,
  candidate "low" .alpha 3 2 "binary_add_sub" .arithmetic
]

def stableKeyCandidates : List Candidate := [
  candidate "f" .beta 1 0 "binary_add_sub" .arithmetic,
  candidate "e" .alpha 2 0 "binary_add_sub" .arithmetic,
  candidate "d" .alpha 1 2 "binary_add_sub" .arithmetic,
  candidate "c" .alpha 1 1 "unary_sign" .arithmetic,
  candidate "b" .alpha 1 1 "binary_add_sub" .arithmetic,
  candidate "a" .alpha 1 1 "binary_add_sub" .arithmetic
]

def truncatedCandidates : List Candidate := diverseCandidates.take 3

structure OracleCase where
  schema : Nat
  id : String
  mode : String
  scenario : String
  candidates : List Candidate
  limit : Nat
  deriving Repr, DecidableEq, BEq

def cases : List OracleCase := [
  { schema := 1, id := "selector_reason_scores", mode := "model-only",
    scenario := "selector_scores", candidates := selectorCandidates, limit := 2 },
  { schema := 1, id := "stable_tie_break", mode := "model-only",
    scenario := "stable_order", candidates := stableKeyCandidates, limit := 6 },
  { schema := 1, id := "diverse_equal_tier", mode := "strict",
    scenario := "diverse_tier", candidates := diverseCandidates, limit := 3 },
  { schema := 1, id := "truncated_limit_saturates", mode := "model-only",
    scenario := "truncated_boundary", candidates := truncatedCandidates, limit := 10 }
]

def idsUnique (ids : List String) : Bool :=
  decide ids.Nodup

def scoresNonIncreasing : List RankedCandidate → Bool
  | [] | [_] => true
  | left :: right :: tail => left.score >= right.score && scoresNonIncreasing (right :: tail)

def caseSafe (item : OracleCase) : Bool :=
  let ranked := rankCandidates item.candidates
  validateRanking ranked &&
    idsUnique (ranked.map fun candidate => candidate.candidate.id) &&
    idsUnique (strictSelect ranked item.limit) &&
    idsUnique (diverseSelect ranked item.limit) &&
    scoresNonIncreasing (diverseOrder ranked) &&
    strictSelect ranked item.limit ==
      (ranked.take item.limit).map fun candidate => candidate.candidate.id

def brokenValidation (_ : List RankedCandidate) : Bool := true

def tamperedRanking : List RankedCandidate :=
  match rankCandidates [candidate "tampered" .alpha 1 0 "binary_add_sub" .arithmetic] with
  | [] => []
  | head :: tail => { head with score := head.score + 1 } :: tail

def brokenDuplicateSelect (ranked : List RankedCandidate) (limit : Nat) : List String :=
  match diverseSelect ranked limit with
  | [] => []
  | head :: tail => (head :: head :: tail).take limit

def brokenTierSelect (ranked : List RankedCandidate) (limit : Nat) : List String :=
  match ranked with
  | first :: second :: third :: fourth :: rest =>
      (first :: fourth :: second :: third :: rest).take limit |>.map fun item => item.candidate.id
  | _ => strictSelect ranked limit

def validationSensitivity : Bool :=
  !validateRanking tamperedRanking && brokenValidation tamperedRanking

def duplicateSensitivity : Bool :=
  let ranked := rankCandidates diverseCandidates
  idsUnique (diverseSelect ranked 3) && !idsUnique (brokenDuplicateSelect ranked 3)

def tierSensitivity : Bool :=
  let ranked := rankCandidates diverseCandidates
  diverseSelect ranked 3 == ["alpha_1", "beta_1", "alpha_2"] &&
    brokenTierSelect ranked 3 != diverseSelect ranked 3

def sensitivityPasses : Bool :=
  validationSensitivity && duplicateSensitivity && tierSensitivity

def subsets : List α → List (List α)
  | [] => [[]]
  | head :: tail =>
      let rest := subsets tail
      rest ++ rest.map fun items => head :: items

def finiteManifests : List (List Candidate) := subsets diverseCandidates

def finiteAuditPasses : Bool :=
  finiteManifests.all fun candidates =>
    let ranked := rankCandidates candidates
    validateRanking ranked &&
      idsUnique (ranked.map fun item => item.candidate.id) &&
      (List.range (candidates.length + 2)).all fun limit =>
        idsUnique (strictSelect ranked limit) &&
        idsUnique (diverseSelect ranked limit) &&
        (strictSelect ranked limit).length == min limit candidates.length &&
        (diverseSelect ranked limit).length == min limit candidates.length &&
        scoresNonIncreasing (diverseOrder ranked)

end HoiminOracle.CandidateRanking
