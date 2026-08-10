import Std

namespace HoiminOracle.CandidateRanking

inductive Path
  | alpha
  | beta
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

def pathKey : Path → Nat
  | .alpha => 0
  | .beta => 1

inductive OperatorClass
  | highValueControl
  | exceptionHandling
  | behavioral
  | arithmetic
  | typeAnnotation
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

inductive Reason
  | explicitLine
  | explicitSymbol
  | changedLine
  | operator (kind : OperatorClass)
  deriving Repr, DecidableEq, BEq, ReflBEq, LawfulBEq

def reasonScore : Reason → Nat
  | .explicitLine => 300
  | .explicitSymbol => 250
  | .changedLine => 200
  | .operator .highValueControl => 100
  | .operator .exceptionHandling => 90
  | .operator .behavioral => 80
  | .operator .arithmetic => 70
  | .operator .typeAnnotation => 50

structure Candidate where
  id : String
  path : Path
  line : Nat
  column : Nat
  operatorKey : String
  operatorClass : OperatorClass
  explicitLine : Bool := false
  explicitSymbol : Bool := false
  changedLine : Bool := false
  deriving Repr, DecidableEq, BEq

def reasons (candidate : Candidate) : List Reason :=
  (if candidate.explicitLine then [.explicitLine] else []) ++
  (if candidate.explicitSymbol then [.explicitSymbol] else []) ++
  (if candidate.changedLine then [.changedLine] else []) ++
  [.operator candidate.operatorClass]

def scoreReasons (items : List Reason) : Nat :=
  items.foldl (fun total item => total + reasonScore item) 0

structure RankedCandidate where
  candidate : Candidate
  rank : Nat
  score : Nat
  rankingReasons : List Reason
  deriving Repr, DecidableEq, BEq

def rankOne (candidate : Candidate) : RankedCandidate where
  candidate := candidate
  rank := 0
  score := scoreReasons (reasons candidate)
  rankingReasons := reasons candidate

def stableBefore (left right : RankedCandidate) : Bool :=
  left.score > right.score ||
    (left.score == right.score &&
      (pathKey left.candidate.path < pathKey right.candidate.path ||
       (left.candidate.path == right.candidate.path &&
        (left.candidate.line < right.candidate.line ||
         (left.candidate.line == right.candidate.line &&
          (left.candidate.column < right.candidate.column ||
           (left.candidate.column == right.candidate.column &&
            (left.candidate.operatorKey < right.candidate.operatorKey ||
             (left.candidate.operatorKey == right.candidate.operatorKey &&
              left.candidate.id < right.candidate.id)))))))))

def insertRanked (candidate : RankedCandidate) : List RankedCandidate → List RankedCandidate
  | [] => [candidate]
  | head :: tail =>
      if stableBefore candidate head then candidate :: head :: tail
      else head :: insertRanked candidate tail

def assignRanksFrom : Nat → List RankedCandidate → List RankedCandidate
  | _, [] => []
  | rank, head :: tail =>
      { head with rank := rank } :: assignRanksFrom (rank + 1) tail

def rankCandidates (candidates : List Candidate) : List RankedCandidate :=
  assignRanksFrom 1 (candidates.foldr (fun candidate => insertRanked (rankOne candidate)) [])

def validateRanking (saved : List RankedCandidate) : Bool :=
  rankCandidates (saved.map RankedCandidate.candidate) == saved

def strictSelect (ranked : List RankedCandidate) (limit : Nat) : List String :=
  (ranked.take limit).map fun candidate => candidate.candidate.id

def pathOrder (candidates : List RankedCandidate) : List Path :=
  candidates.foldl (fun paths candidate =>
    if candidate.candidate.path ∈ paths then paths
    else paths ++ [candidate.candidate.path]) []

def firstForPath? (path : Path) (candidates : List RankedCandidate) : Option RankedCandidate :=
  candidates.find? fun candidate => candidate.candidate.path == path

def selectRound (paths : List Path) (candidates : List RankedCandidate) : List RankedCandidate :=
  paths.filterMap fun path => firstForPath? path candidates

def eraseSelected (selected candidates : List RankedCandidate) : List RankedCandidate :=
  candidates.filter fun candidate =>
    !(selected.any fun picked => picked.candidate.id == candidate.candidate.id)

def roundRobinWithFuel : Nat → List Path → List RankedCandidate → List RankedCandidate
  | 0, _, _ => []
  | fuel + 1, paths, candidates =>
      let selected := selectRound paths candidates
      if selected.isEmpty then []
      else selected ++ roundRobinWithFuel fuel paths (eraseSelected selected candidates)

def roundRobinTier (candidates : List RankedCandidate) : List RankedCandidate :=
  roundRobinWithFuel candidates.length (pathOrder candidates) candidates

def diverseOrderWithFuel : Nat → List RankedCandidate → List RankedCandidate
  | 0, _ => []
  | _, [] => []
  | fuel + 1, head :: tail =>
      let tierTail := tail.takeWhile fun candidate => candidate.score == head.score
      let tier := head :: tierTail
      let rest := tail.dropWhile fun candidate => candidate.score == head.score
      roundRobinTier tier ++ diverseOrderWithFuel fuel rest

def diverseOrder (ranked : List RankedCandidate) : List RankedCandidate :=
  diverseOrderWithFuel ranked.length ranked

def diverseSelect (ranked : List RankedCandidate) (limit : Nat) : List String :=
  ((diverseOrder ranked).take limit).map fun candidate => candidate.candidate.id

end HoiminOracle.CandidateRanking
