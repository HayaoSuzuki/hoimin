import HoiminOracle.MutationScoreExitPolicyModel
import HoiminOracle.ProgressDecisionModel

namespace HoiminOracle.ProgressInput

open MutationScoreExitPolicy

inductive Baseline
  | passed | failed | missing
  deriving Repr, BEq, DecidableEq

inductive Termination
  | exitZero | exitNonzero | timeout | outOfMemory | processLimit | cancelled
  deriving Repr, BEq, DecidableEq

inductive OutputState
  | complete | closeTimedOut
  deriving Repr, BEq, DecidableEq

structure ResultFields where
  termination : Option Termination
  outputState : OutputState
  deriving Repr, BEq

def terminationStatus : Termination → Status
  | .exitZero => .survived
  | .exitNonzero => .killed
  | .timeout => .timeout
  | .outOfMemory => .outOfMemory
  | .processLimit => .processLimit
  | .cancelled => .notRun

-- Spool and diagnostic payloads are canonical in the generated result cases.
def validResult (status : Status) (fields : ResultFields) : Bool :=
  match fields.outputState with
  | .closeTimedOut => fields.termination.isSome && status == .error
  | .complete => match fields.termination with
    | none => true
    | some termination => status == terminationStatus termination

structure Input where
  statuses : List Status
  baseline : Baseline := .passed
  reportedComplete : Bool
  reportedExit : Int
  resultFields : Option ResultFields := none
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

def resultsValid (input : Input) : Bool :=
  match input.resultFields with
  | none => true -- Existing summary cases use canonical results in the adapter.
  | some fields => input.statuses.all fun status => validResult status fields

def classify (input : Input) : Disposition :=
  if !resultsValid input then .invalid
  else if !coherent (summarize input.statuses) input.reportedComplete input.reportedExit then .invalid
  else match input.baseline with
    | .missing => .missingBaseline
    | .failed => .baselineFailed
    | .passed => if input.reportedComplete then .usable else .incomplete

def brokenSkipResultValidation (input : Input) : Disposition :=
  classify { input with resultFields := none }

def brokenIgnoreOutputState (input : Input) : Disposition :=
  classify { input with resultFields := input.resultFields.map fun fields =>
    { fields with outputState := .complete } }

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
