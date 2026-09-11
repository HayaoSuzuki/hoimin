import HoiminOracle.ProgressInputModel

namespace HoiminOracle.ProgressInput

open MutationScoreExitPolicy

theorem timeout_report_with_complete_flipped_is_invalid :
    classify ⟨[.timeout], .passed, true, 4, none⟩ = .invalid := by decide

theorem invalid_has_no_comparison_input (input : Input)
    (h : classify input = .invalid) : toProgressReport input = none := by
  simp [toProgressReport, h]

theorem incoherent_is_invalid (input : Input)
    (h : coherent (summarize input.statuses) input.reportedComplete input.reportedExit = false) :
    classify input = .invalid := by
  simp [classify, h]

theorem complete_requires_no_inconclusive (counts : Counts) (code : Int)
    (h : coherent counts true code = true) : inconclusive counts = 0 := by
  simp [coherent, complete, policyFromCounts] at h
  simp [inconclusive]
  omega

theorem producer_is_coherent (counts : Counts) (flags : RunFlags) :
    coherent counts (complete (composePolicy counts flags))
      (exitCode (composePolicy counts flags) : Int) = true := by
  simp [coherent, exitCanResultFromRunFailure, complete, composePolicy, policyFromCounts, exitCode]
  split <;> simp_all
  all_goals split <;> simp_all
  all_goals split <;> simp_all
  all_goals split <;> simp_all
  all_goals split <;> simp_all
  all_goals split <;> simp_all
  all_goals omega

theorem invalid_result_is_invalid (input : Input) (h : resultsValid input = false) :
    classify input = .invalid := by simp [classify, h]

theorem legacy_absent_complete_accepts (status : Status) :
    validResult status ⟨none, .complete⟩ = true := by rfl

theorem close_timeout_requires_error (status : Status) (termination : Option Termination)
    (h : validResult status ⟨termination, .closeTimedOut⟩ = true) : status = .error := by
  simpa [validResult] using (Bool.and_eq_true_iff.mp h).2

theorem successful_exit_close_timeout_is_error :
    validResult .error ⟨some .exitZero, .closeTimedOut⟩ = true := by decide

theorem killed_successful_exit_is_invalid :
    classify ⟨[.killed], .passed, true, 0, some ⟨some .exitZero, .complete⟩⟩ = .invalid := by decide

theorem skip_validation_witness :
    let input : Input := ⟨[.killed], .passed, true, 0, some ⟨some .exitZero, .complete⟩⟩
    brokenSkipResultValidation input = .usable ∧ classify input = .invalid := by decide

theorem ignore_output_state_witness :
    let input : Input := ⟨[.error], .passed, false, 2, some ⟨some .exitZero, .closeTimedOut⟩⟩
    brokenIgnoreOutputState input = .invalid ∧ classify input = .incomplete := by decide

end HoiminOracle.ProgressInput
