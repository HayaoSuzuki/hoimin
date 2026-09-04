import HoiminOracle.CleanupCapabilityModel

namespace HoiminOracle.CleanupCapability

theorem step_preserves_outside_writability (state : State) (event : Event) :
    (step state event).outsideWritable = state.outsideWritable := by
  rcases state with ⟨strategy, binding, phase, observed, handle, owned, outside⟩
  cases strategy <;> cases binding <;> cases phase <;> cases observed <;>
    cases handle <;> cases event <;> rfl

theorem runWith_step_preserves_outside_writability
    (state : State) (events : List Event) :
    (runWith step state events).outsideWritable = state.outsideWritable := by
  induction events generalizing state with
  | nil => rfl
  | cons event rest induction =>
      exact (induction (step state event)).trans
        (step_preserves_outside_writability state event)

theorem cleanup_never_changes_outside_writability
    (strategy : Strategy) (events : List Event) :
    (run strategy events).outsideWritable = false := by
  exact runWith_step_preserves_outside_writability (State.initial strategy) events

theorem retained_broken_witness_changes_outside :
    (brokenRun .retained [.inspect, .swap, .bind, .effect]).outsideWritable = true := by
  decide

theorem post_inspection_broken_witness_changes_outside :
    (brokenRun .postInspection [.inspect, .swap, .bind, .effect]).outsideWritable = true := by
  decide

end HoiminOracle.CleanupCapability
