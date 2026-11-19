import HoiminOracle.ReduceSpecModel

namespace HoiminOracle.ReduceSpec

set_option maxHeartbeats 20000 in
theorem pairPass_strict (num den original reduced : Nat)
    (h : pairPass num den original reduced = true) : reduced < original := by
  simp only [pairPass, validRate, Bool.and_eq_true, decide_eq_true_eq] at h
  obtain ⟨⟨hn, hd⟩, ho, _, hb⟩ := h
  have hdpos : 0 < den := Nat.lt_trans hn hd
  have hs : den - num < den := Nat.sub_lt hdpos hn
  have hm : (den - num) * original < den * original := Nat.mul_lt_mul_of_pos_right hs ho
  have hlt : den * reduced < den * original := Nat.lt_of_le_of_lt hb hm
  exact (Nat.mul_lt_mul_left hdpos).mp hlt

set_option maxHeartbeats 20000 in
theorem certified_protected (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    ctx.keptSpans.all (fun k => p.retained.contains k.id &&
      p.removed.all (fun r => !overlaps r k)) = true := by
  have hs := h.2.1
  simp only [scopeValid, Bool.and_eq_true] at hs
  exact hs.2

set_option maxHeartbeats 20000 in
theorem certified_exact (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    exactResults ctx.selected ctx.before p.after = true := h.2.2.2.2.1

set_option maxHeartbeats 20000 in
theorem certified_identity (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    sameEvidence ctx p = true := h.2.2.1

set_option maxHeartbeats 20000 in
theorem certified_three_pairs (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    p.pairCount = 3 := h.2.2.2.2.2.2.1

set_option maxHeartbeats 20000 in
theorem chooseBest_safe (ctx : Context) (s : State) (p : Proposal)
    (hs : Safe ctx s) (hp : Certified ctx p) (ht : p.completedAt < s.deadline) :
    Safe ctx { s with best := chooseBest s.best p } := by
  intro q hq
  cases hb : s.best with
  | none =>
    simp [chooseBest, hb] at hq
    subst q
    exact ⟨hp, ht⟩
  | some old =>
    by_cases hlt : median p.exploration < median old.exploration
    · simp [chooseBest, hb, hlt] at hq
      subst q
      exact ⟨hp, ht⟩
    · simp [chooseBest, hb, hlt] at hq
      subst q
      exact hs old hb

set_option maxHeartbeats 20000 in
theorem chooseBest_idempotent (best : Option Proposal) (p : Proposal) :
    chooseBest (chooseBest best p) p = chooseBest best p := by
  cases best with
  | none => simp [chooseBest]
  | some old =>
    by_cases h : median p.exploration < median old.exploration <;> simp [chooseBest, h]

set_option maxHeartbeats 20000 in
theorem step_preserves (ctx : Context) (s : State) (e : Event) (hs : Safe ctx s) :
    Safe ctx (step ctx s e) := by
  unfold step
  split
  · exact hs
  · split
    · exact hs
    · rename_i htime
      cases he : e.proposal with
      | none => exact hs
      | some p =>
        dsimp only
        split
        · exact hs
        · split
          · rename_i hp
            have ht : p.completedAt < s.deadline := by omega
            exact chooseBest_safe ctx s p hs hp.1 ht
          · exact hs

set_option maxHeartbeats 20000 in
theorem run_preserves (ctx : Context) (s : State) (events : List Event) (hs : Safe ctx s) :
    Safe ctx (run ctx s events) := by
  induction events generalizing s with
  | nil => exact hs
  | cons e rest ih => exact ih (step ctx s e) (step_preserves ctx s e hs)

set_option maxHeartbeats 20000 in
theorem all_trace_artifact_safety (ctx : Context) (deadline : Nat) (events : List Event) :
    Safe ctx (run ctx { deadline := deadline } events) := by
  apply run_preserves
  intro p hp
  cases hp

set_option maxHeartbeats 20000 in
theorem stopped_absorbing (ctx : Context) (s : State) (e : Event)
    (h : s.stopped = true) : step ctx s e = s := by simp [step, h]

set_option maxHeartbeats 20000 in
theorem deadline_no_publish (ctx : Context) (s : State) (e : Event)
    (h : s.deadline ≤ e.tick) : (step ctx s e).best = s.best := by
  unfold step
  split <;> simp_all

set_option maxHeartbeats 20000 in
theorem chooseBest_minimum (best : Option Proposal) (p : Proposal) :
    ∃ q, chooseBest best p = some q ∧
      median q.exploration ≤ median p.exploration ∧
      ∀ old, best = some old → median q.exploration ≤ median old.exploration := by
  cases best with
  | none =>
    refine ⟨p, rfl, Nat.le_refl _, ?_⟩
    intro old h
    cases h
  | some old =>
    by_cases h : median p.exploration < median old.exploration
    · refine ⟨p, ?_, Nat.le_refl _, ?_⟩
      · simp [chooseBest, h]
      · intro q hq
        cases hq
        exact Nat.le_of_lt h
    · refine ⟨old, ?_, ?_, ?_⟩
      · simp [chooseBest, h]
      · omega
      · intro q hq
        cases hq
        exact Nat.le_refl _

set_option maxHeartbeats 20000 in
theorem finalPass_strict (num den : Nat) (original reduced : Triple)
    (h : finalPass num den original reduced = true) :
    reduced.a < original.a ∧ reduced.b < original.b ∧ reduced.c < original.c := by
  simp only [finalPass, Bool.and_eq_true] at h
  exact ⟨pairPass_strict _ _ _ _ h.1.1,
    pairPass_strict _ _ _ _ h.1.2, pairPass_strict _ _ _ _ h.2⟩

set_option maxHeartbeats 20000 in
theorem empty_span_no_overlap (id point : Nat) (b : Span) :
    overlaps ⟨id,point,point⟩ b = false := by simp [overlaps]

set_option maxHeartbeats 20000 in
theorem overlap_symmetric (a b : Span) : overlaps a b = overlaps b a := by
  simp [overlaps, and_comm, and_assoc]

set_option maxHeartbeats 20000 in
theorem exactResults_lookup (selected : List Nat) (before after : List Result)
    (h : exactResults selected before after = true) (id : Nat) (hi : id ∈ selected) :
    lookup before id = lookup after id := by
  have hs : selected.all (fun i => decide (lookup before i = lookup after i)) = true := by
    simp only [exactResults, Bool.and_eq_true] at h
    exact h.2
  have hv := List.all_eq_true.mp hs id hi
  simpa only [decide_eq_true_eq] using hv

set_option maxHeartbeats 20000 in
theorem certified_frozen_timeout (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    p.evidence.timeout = ctx.identity.timeout := by
  have he : p.evidence = { ctx.identity with variant := p.id } := by
    simpa only [sameEvidence, decide_eq_true_eq] using certified_identity ctx p h
  have ht := congrArg Identity.timeout he
  exact ht

set_option maxHeartbeats 20000 in
theorem certified_original_kill (ctx : Context) (p : Proposal) (h : Certified ctx p) :
    ctx.before.any (fun r => r.outcome == .killed) = true := by
  have hc := h.1
  simp only [configValid, Bool.and_eq_true] at hc
  exact hc.2

set_option maxHeartbeats 20000 in
theorem kept_interval_not_removed (ctx : Context) (p : Proposal) (k r : Span)
    (h : Certified ctx p) (hk : k ∈ ctx.keptSpans) (hr : r ∈ p.removed) :
    overlaps r k = false := by
  have hp := List.all_eq_true.mp (certified_protected ctx p h) k hk
  simp only [Bool.and_eq_true] at hp
  have hn := List.all_eq_true.mp hp.2 r hr
  simpa using hn

set_option maxHeartbeats 20000 in
theorem invalid_config_no_publish (ctx : Context) (s : State) (e : Event)
    (h : configValid ctx = false) : (step ctx s e).best = s.best := by
  unfold step
  split <;> simp_all

set_option maxHeartbeats 20000 in
theorem measurement_error_stops (ctx : Context) (s : State) (e : Event) (p : Proposal)
    (active : s.stopped = false) (hp : e.proposal = some p)
    (hm : measurementsValid p = false) :
    (step ctx s e).stopped = true ∧ (step ctx s e).best = s.best := by
  simp only [step, active, Bool.false_eq_true, ↓reduceIte]
  split <;> simp_all

end HoiminOracle.ReduceSpec
