import HoiminOracle.ProcessOutputModel

namespace HoiminOracle.ProcessOutput

theorem closed_decision_table_is_safe : allSafe = true := by native_decide

theorem mutant_close_timeout_is_nonfatal_error (termination : Termination) :
    decideOutcome .mutant (.known termination) .closeTimedOut =
      { fatal := false, status := some .error, termination := some termination,
        outputIncomplete := true, diagnostic := true, continueRun := true } := by
  rfl

theorem baseline_close_timeout_remains_fatal (termination : Termination) :
    (decideOutcome .baseline (.known termination) .closeTimedOut).fatal = true := by
  rfl

theorem process_failure_remains_fatal (execution : ExecutionKind)
    (output : OutputOutcome) :
    (decideOutcome execution .failed output).fatal = true := by
  cases execution <;> cases output <;> rfl

theorem sensitivity_detects_broken_families : sensitivityPasses = true := by native_decide

end HoiminOracle.ProcessOutput
