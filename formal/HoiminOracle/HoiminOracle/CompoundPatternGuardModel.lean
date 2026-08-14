import HoiminOracle.BindingFlowModel

namespace HoiminOracle.CompoundPatternGuard

open HoiminOracle.BindingFlow

structure Attempt where
  matched : List Env := []
  failed : List Env := []
  deriving Repr, DecidableEq, BEq

structure AttemptSummary where
  matched : Option Env
  failed : Option Env
  deriving Repr, DecidableEq, BEq

def Attempt.summary (attempt : Attempt) : AttemptSummary where
  matched := meetAll? attempt.matched
  failed := meetAll? attempt.failed

def pureTest (incoming : Env) (canSucceed canFail : Bool) : Attempt :=
  { matched := if canSucceed then [incoming] else []
    failed := if canFail then [incoming] else [] }

def capture (name : Name) (incoming : Env) : Attempt :=
  { matched := [incoming.set name .shadowed] }

def thenAttempt (first : Attempt) (next : Env → Attempt) : Attempt :=
  { matched := first.matched.flatMap fun environment => (next environment).matched
    failed := first.failed ++
      first.matched.flatMap fun environment => (next environment).failed }

def asPattern (child : Attempt) (name : Name) : Attempt :=
  thenAttempt child (capture name)

def sequencePatterns (first : Attempt) (children : List (Env → Attempt)) : Attempt :=
  children.foldl (fun current child => thenAttempt current child) first

def mappingPattern (structural : Attempt) (children : List (Env → Attempt))
    (rest : Option Name) : Attempt :=
  let completed := sequencePatterns structural children
  match rest with
  | none => completed
  | some name => thenAttempt completed (capture name)

def classPattern (structural : Attempt) (children : List (Env → Attempt)) : Attempt :=
  sequencePatterns structural children

def orPattern (arms : List Attempt) : Attempt :=
  { matched := arms.flatMap Attempt.matched
    failed := arms.flatMap Attempt.failed }

structure GuardResult where
  body : Option Env
  nextCase : Option Env
  deriving Repr, DecidableEq, BEq

def applyGuard (attempt : Attempt)
    (guard : Option (Env → Env × Bool)) : GuardResult :=
  let summary := attempt.summary
  match summary.matched, guard with
  | none, _ => { body := none, nextCase := summary.failed }
  | some matched, none => { body := some matched, nextCase := summary.failed }
  | some matched, some evaluate =>
      let result := evaluate matched
      if result.2 then
        { body := some result.1, nextCase := summary.failed }
      else
        { body := none
          nextCase := meetOption summary.failed (some result.1) }

end HoiminOracle.CompoundPatternGuard
