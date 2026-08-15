import HoiminOracle.CandidateSpanModel

namespace HoiminOracle.CandidateSpan

theorem accepted_span_within_source (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    candidate.start + candidate.length ≤ environment.source.length :=
  accepted.2.1

theorem accepted_span_within_counter (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    candidate.start + candidate.length ≤ environment.maximumOffset :=
  accepted.1

theorem accepted_boundaries (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    boundaryAt environment candidate.start = true ∧
      boundaryAt environment (candidate.start + candidate.length) = true :=
  ⟨accepted.2.2.1, accepted.2.2.2.1⟩

theorem accepted_original_matches (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    candidate.original.length = candidate.length ∧
      sourceSlice environment candidate = candidate.original :=
  ⟨accepted.2.2.2.2.1, accepted.2.2.2.2.2.1⟩

theorem accepted_location_matches (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    locationAt environment candidate.start =
      { line := candidate.line, column := candidate.column } :=
  accepted.2.2.2.2.2.2.1

theorem accepted_identity_matches (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    candidate.identity = identityOf candidate :=
  by
    rcases accepted with ⟨_, _, _, _, _, _, _, _, _, _, identity⟩
    exact identity

theorem complete_transport_preserves_candidate (candidate : Candidate) :
    completeTransport candidate = candidate := by
  rfl

theorem complete_transport_preserves_identity (candidate : Candidate) :
    identityOf (completeTransport candidate) = identityOf candidate := by
  rfl

theorem session_projection_preserves_identity (candidate : Candidate) :
    (sessionProjection (completeTransport candidate)).identity = identityOf candidate := by
  rfl

theorem identity_schema_sensitive (left right : Candidate)
    (different : left.schema ≠ right.schema) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.schema equal)

theorem identity_start_sensitive (left right : Candidate)
    (different : left.start ≠ right.start) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.start equal)

theorem identity_length_sensitive (left right : Candidate)
    (different : left.length ≠ right.length) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.length equal)

theorem identity_path_sensitive (left right : Candidate)
    (different : left.path ≠ right.path) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.path equal)

theorem identity_hash_sensitive (left right : Candidate)
    (different : left.sourceHash ≠ right.sourceHash) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.sourceHash equal)

theorem identity_operator_sensitive (left right : Candidate)
    (different : left.operator ≠ right.operator) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.operator equal)

theorem identity_replacement_sensitive (left right : Candidate)
    (different : left.replacement ≠ right.replacement) :
    identityOf left ≠ identityOf right := by
  intro equal
  exact different (congrArg Identity.replacement equal)

theorem application_is_exact_reference (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    applyChecked environment candidate = some
      (environment.source.take candidate.start ++ candidate.replacement ++
        environment.source.drop (candidate.start + candidate.length)) := by
  simp [applyChecked, valid, accepted, replaceBytes]

theorem application_preserves_prefix (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    (replaceBytes environment.source candidate).take candidate.start =
      environment.source.take candidate.start := by
  have startBound : candidate.start ≤ environment.source.length := by
    have endBound := accepted_span_within_source environment candidate accepted
    omega
  have prefixLength : (environment.source.take candidate.start).length = candidate.start := by
    simp [startBound]
  simp [replaceBytes, prefixLength]

theorem application_preserves_suffix (environment : Environment) (candidate : Candidate)
    (accepted : Valid environment candidate) :
    (replaceBytes environment.source candidate).drop
        (candidate.start + candidate.replacement.length) =
      environment.source.drop (candidate.start + candidate.length) := by
  have startBound : candidate.start ≤ environment.source.length := by
    have endBound := accepted_span_within_source environment candidate accepted
    omega
  have prefixLength : (environment.source.take candidate.start).length = candidate.start := by
    simp [startBound]
  rw [replaceBytes, List.append_assoc]
  rw [show candidate.start + candidate.replacement.length =
      (environment.source.take candidate.start).length + candidate.replacement.length by
        omega]
  rw [List.drop_length_add_append]
  simp

theorem rejected_application_preserves_source (environment : Environment) (candidate : Candidate)
    (rejected : ¬ Valid environment candidate) :
    applyOrOriginal environment candidate = environment.source := by
  simp [applyOrOriginal, applyChecked, valid, rejected]

theorem reset_makes_second_application_independent (environment : Environment)
    (first second : Candidate) :
    applyAfterReset environment first second = applyChecked environment second := by
  rfl

end HoiminOracle.CandidateSpan
