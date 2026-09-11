import HoiminOracle.MutationScoreExitPolicyModel
import HoiminOracle.ProgressDecisionModel

namespace HoiminOracle.ProgressInput

open MutationScoreExitPolicy

inductive Baseline
  | passed | failed | missing
  deriving Repr, BEq, DecidableEq

structure Input where
  statuses : List Status
  baseline : Baseline := .passed
  reportedComplete : Bool
  reportedExit : Int
  deriving Repr, BEq

inductive Disposition
  | invalid | usable | incomplete | baselineFailed | missingBaseline
  deriving Repr, BEq, DecidableEq

def exitCanResultFromRunFailure (counts : Counts) (reportedExit : Int) : Bool :=
  let possibleFailures : List RunFlags := [
    { interrupted := true }, { infrastructureError := true },
    { baselineFailed := true }, { incomplete := true } ]
  possibleFailures.any fun flags =>
    reportedExit == (exitCode (composePolicy counts flags) : Int)

def coherent (counts : Counts) (reportedComplete : Bool) (reportedExit : Int) : Bool :=
  if reportedComplete then
    complete (policyFromCounts counts) && reportedExit == (exitCode (policyFromCounts counts) : Int)
  else
    exitCanResultFromRunFailure counts reportedExit

def classify (input : Input) : Disposition :=
  if !coherent (summarize input.statuses) input.reportedComplete input.reportedExit then .invalid
  else match input.baseline with
    | .missing => .missingBaseline
    | .failed => .baselineFailed
    | .passed => if input.reportedComplete then .usable else .incomplete

def toProgressReport (input : Input) : Option ProgressDecision.Report :=
  match classify input with
  | .invalid => none
  | .usable => some (.usable (input.statuses.zipIdx.map fun (status, index) =>
      { candidateId := index, contentKey := index, status := match status with
          | .killed => .killed
          | .survived => .survived
          | _ => .inconclusive }))
  | _ => some .unusable

end HoiminOracle.ProgressInput
