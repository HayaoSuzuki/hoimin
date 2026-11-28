import HoiminOracle.ProcessOutputModel

namespace HoiminOracle.ProcessOutput

theorem closed_decision_table_is_safe : allSafe = true := by native_decide

theorem mutant_close_timeout_is_nonfatal_error (termination : Termination) :
    decideOutcome .mutant (.known termination) .closeTimedOut =
      { status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true } := by
  rfl

theorem non_mutant_close_timeout_returns_output_failure (execution : ExecutionKind)
    (termination : Termination) (notMutant : execution ≠ .mutant) :
    (decideOutcome execution (.known termination) .closeTimedOut).failure = some .output := by
  cases execution <;> first | contradiction | rfl

theorem process_failure_remains_primary (execution : ExecutionKind)
    (output : OutputOutcome) :
    (decideOutcome execution .failed output).failure = some .process := by
  cases execution <;> cases output <;> rfl

theorem sensitivity_detects_broken_families : sensitivityPasses = true := by native_decide

theorem definite_drain_failure_precedes_close_timeout (stdout stderr collector : OutputOutcome)
    (failed : stdout = .failed ∨ stderr = .failed ∨ collector = .failed) :
    (selectDrainError stdout stderr collector).map Prod.snd = some .failed := by
  cases stdout <;> cases stderr <;> cases collector <;> simp_all [selectDrainError, drainEvidence]

theorem drain_timeout_requires_no_definite_failure (stdout stderr collector : OutputOutcome)
    (timeout : (selectDrainError stdout stderr collector).map Prod.snd = some .closeTimedOut) :
    stdout ≠ .failed ∧ stderr ≠ .failed ∧ collector ≠ .failed := by
  cases stdout <;> cases stderr <;> cases collector <;> simp_all [selectDrainError, drainEvidence]

end HoiminOracle.ProcessOutput
