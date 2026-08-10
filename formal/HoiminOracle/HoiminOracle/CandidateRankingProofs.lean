import HoiminOracle.CandidateRankingModel

namespace HoiminOracle.CandidateRanking

set_option maxHeartbeats 100000 in
theorem rankOne_score_is_reason_sum (candidate : Candidate) :
    (rankOne candidate).score = scoreReasons (reasons candidate) := by
  rfl

set_option maxHeartbeats 100000 in
theorem rankOne_reasons_are_constructed (candidate : Candidate) :
    (rankOne candidate).rankingReasons = reasons candidate := by
  rfl

set_option maxHeartbeats 100000 in
theorem strictSelect_is_saved_prefix (ranked : List RankedCandidate) (limit : Nat) :
    strictSelect ranked limit =
      (ranked.take limit).map fun candidate => candidate.candidate.id := by
  rfl

set_option maxHeartbeats 100000 in
theorem strictSelect_length (ranked : List RankedCandidate) (limit : Nat) :
    (strictSelect ranked limit).length = min limit ranked.length := by
  simp [strictSelect]

set_option maxHeartbeats 100000 in
theorem strictSelect_member_of_saved
    (ranked : List RankedCandidate) (limit : Nat) (id : String)
    (selected : id ∈ strictSelect ranked limit) :
    ∃ candidate ∈ ranked, candidate.candidate.id = id := by
  simp only [strictSelect, List.mem_map] at selected
  obtain ⟨candidate, candidateInPrefix, candidateId⟩ := selected
  exact ⟨candidate, List.mem_of_mem_take candidateInPrefix, candidateId⟩

set_option maxHeartbeats 100000 in
theorem validation_recomputes_complete_ranking (saved : List RankedCandidate) :
    validateRanking saved =
      (rankCandidates (saved.map RankedCandidate.candidate) == saved) := by
  rfl

end HoiminOracle.CandidateRanking
