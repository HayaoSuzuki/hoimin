import HoiminOracle.ProgressInputModel

namespace HoiminOracle.ProgressInput

open MutationScoreExitPolicy

theorem timeout_report_with_complete_flipped_is_invalid :
    classify ⟨[.timeout], .passed, true, 4⟩ = .invalid := by decide

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

end HoiminOracle.ProgressInput
