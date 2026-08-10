import HoiminOracle.BindingFlowModel

namespace HoiminOracle.BindingFlow

set_option maxHeartbeats 100000 in
theorem fact_meet_idem (fact : Fact) : fact.meet fact = fact := by
  simp [Fact.meet]

set_option maxHeartbeats 100000 in
theorem fact_meet_comm (left right : Fact) : left.meet right = right.meet left := by
  by_cases equal : left = right
  · subst right
    simp [Fact.meet]
  · have reverse : right ≠ left := Ne.symm equal
    simp [Fact.meet, equal, reverse]

private theorem fact_meet_absorb_left (left right : Fact) :
    left.meet (left.meet right) = left.meet right := by
  by_cases equal : left = right
  · subst right
    simp [fact_meet_idem]
  · by_cases unknown : left = .unknown
    · subst left
      cases right <;> simp [Fact.meet]
    · have joined : left.meet right = .unknown := by
        simp [Fact.meet, equal]
      rw [joined]
      simp [Fact.meet, unknown]

private theorem fact_meet_absorb_right (left right : Fact) :
    (left.meet right).meet right = left.meet right := by
  calc
    (left.meet right).meet right = right.meet (left.meet right) :=
      fact_meet_comm (left.meet right) right
    _ = right.meet (right.meet left) := by rw [fact_meet_comm left right]
    _ = right.meet left := fact_meet_absorb_left right left
    _ = left.meet right := fact_meet_comm right left

set_option maxHeartbeats 100000 in
theorem fact_meet_assoc (left middle right : Fact) :
    (left.meet middle).meet right = left.meet (middle.meet right) := by
  by_cases leftMiddle : left = middle
  · subst middle
    rw [fact_meet_idem, fact_meet_absorb_left]
  · by_cases middleRight : middle = right
    · subst right
      rw [fact_meet_idem, fact_meet_absorb_right]
    · have firstUnknown : left.meet middle = .unknown := by
        simp [Fact.meet, leftMiddle]
      have secondUnknown : middle.meet right = .unknown := by
        simp [Fact.meet, middleRight]
      rw [firstUnknown, secondUnknown]
      cases left <;> cases right <;> simp [Fact.meet]

set_option maxHeartbeats 100000 in
theorem env_meet_idem (environment : Env) :
    environment.meet environment = environment := by
  cases environment
  simp [Env.meet, fact_meet_idem]

set_option maxHeartbeats 100000 in
theorem env_meet_comm (left right : Env) :
    left.meet right = right.meet left := by
  cases left
  cases right
  simp [Env.meet, fact_meet_comm]

set_option maxHeartbeats 100000 in
theorem env_meet_assoc (left middle right : Env) :
    (left.meet middle).meet right = left.meet (middle.meet right) := by
  cases left
  cases middle
  cases right
  simp [Env.meet, fact_meet_assoc]

set_option maxHeartbeats 100000 in
theorem meet_does_not_invent_knowledge
    (left right : Fact)
    (target : Target)
    (retained : left.meet right = .known target) :
    left = .known target ∧ right = .known target := by
  by_cases equal : left = right
  · subst right
    simpa [fact_meet_idem] using And.intro retained retained
  · simp [Fact.meet, equal] at retained

private theorem env_meet_retains_known
    (left right : Env)
    (name : Name)
    (target : Target)
    (retained : (left.meet right).get name = .known target) :
    left.get name = .known target ∧ right.get name = .known target := by
  cases name <;> simp only [Env.get, Env.meet] at retained ⊢
  · exact meet_does_not_invent_knowledge left.source right.source target retained
  · exact meet_does_not_invent_knowledge
      left.destination right.destination target retained

private theorem foldl_meet_retains_known
    (initial : Env)
    (rest : List Env)
    (name : Name)
    (target : Target)
    (retained : (rest.foldl Env.meet initial).get name = .known target) :
    initial.get name = .known target ∧
      ∀ environment ∈ rest, environment.get name = .known target := by
  induction rest generalizing initial with
  | nil =>
      exact ⟨retained, by simp⟩
  | cons next rest inductionHypothesis =>
      simp only [List.foldl_cons] at retained
      have tail := inductionHypothesis (initial := initial.meet next) retained
      have head := env_meet_retains_known initial next name target tail.1
      refine ⟨head.1, ?_⟩
      intro environment membership
      simp only [List.mem_cons] at membership
      rcases membership with rfl | membership
      · exact head.2
      · exact tail.2 environment membership

set_option maxHeartbeats 100000 in
theorem meetAll_retained_on_every_path
    (paths : List Env)
    (joined : Env)
    (name : Name)
    (target : Target)
    (joinedPaths : meetAll? paths = some joined)
    (retained : joined.get name = .known target) :
    ∀ environment ∈ paths, environment.get name = .known target := by
  cases paths with
  | nil => simp [meetAll?] at joinedPaths
  | cons first rest =>
      simp only [meetAll?] at joinedPaths
      have equality : rest.foldl Env.meet first = joined :=
        Option.some.inj joinedPaths
      have retainedFold :
          (rest.foldl Env.meet first).get name = .known target := by
        simpa [equality] using retained
      have allRetained := foldl_meet_retains_known first rest name target retainedFold
      intro environment membership
      simp only [List.mem_cons] at membership
      rcases membership with rfl | membership
      · exact allRetained.1
      · exact allRetained.2 environment membership

set_option maxHeartbeats 100000 in
theorem allowed_candidate_is_sound
    (environment : Env)
    (target : Target)
    (allowed : allowsCandidate environment target = true) :
    environment.source = .known target ∧
      environment.destination = .known target := by
  simpa [allowsCandidate] using allowed

set_option maxHeartbeats 100000 in
theorem unrelated_sibling_isolated
    (name : Name)
    (candidate : Candidate)
    (siblings : List Frame) :
    resolve name { candidate with unrelatedSiblings := siblings } =
      resolve name candidate := by
  rfl

set_option maxHeartbeats 100000 in
theorem method_skips_class_scope (name : Name) :
    resolve name {
      path := [
        functionFrame 2 emptyEnv,
        classFrame 1 shadowedEnv shadowedEnv,
        moduleFrame emptyEnv emptyEnv
      ]
    } = .known .builtin := by
  cases name <;> rfl

set_option maxHeartbeats 100000 in
theorem fallthrough_finally_preserves_category
    (category : ExitCategory)
    (environment : Env) :
    routeCategory category (Exits.fallthroughOnly environment) =
      Exits.categoryOnly category environment := by
  cases category <;> rfl

private theorem fact_meet_rank_le_left (left right : Fact) :
    (left.meet right).rank ≤ left.rank := by
  unfold Fact.meet
  split <;> simp [Fact.rank]

set_option maxHeartbeats 100000 in
theorem loop_iteration_descends (head backEdge : Env) :
    (loopIteration head backEdge).rank ≤ head.rank := by
  exact Nat.add_le_add
    (fact_meet_rank_le_left head.source backEdge.source)
    (fact_meet_rank_le_left head.destination backEdge.destination)

set_option maxHeartbeats 100000 in
theorem loop_iteration_stabilizes (head backEdge : Env) :
    loopIteration (loopIteration head backEdge) backEdge =
      loopIteration head backEdge := by
  calc
    loopIteration (loopIteration head backEdge) backEdge =
        head.meet (backEdge.meet backEdge) :=
      env_meet_assoc head backEdge backEdge
    _ = head.meet backEdge := by rw [env_meet_idem]
    _ = loopIteration head backEdge := rfl

set_option maxHeartbeats 100000 in
theorem eval_preserves_safety
    (fuel : Nat)
    (statement : Stmt)
    (initial result : Env)
    (target : Target)
    (_reachable : result ∈ (eval fuel statement initial).states)
    (allowed : allowsCandidate result target = true) :
    result.source = .known target ∧ result.destination = .known target := by
  exact allowed_candidate_is_sound result target allowed

end HoiminOracle.BindingFlow
