import HoiminOracle.SessionAdmissionModel

namespace HoiminOracle.SessionWalAdmission

open HoiminOracle.SessionAdmission

structure State where
  checkpointed : Image
  committed : Image

def initial (image : Image) : State := ⟨image, image⟩

inductive Change where
  | version (value : Int)
  | applicationId (value : Int)
  | createForeign
  | dropForeign
  | initialize

def commit (state : State) (change : Change) : State :=
  let image := state.committed
  { state with committed := match change with
    | .version value => { image with version := value }
    | .applicationId value => { image with applicationId := value }
    | .createForeign => { image with objects := .foreign }
    | .dropForeign => { image with objects := .empty }
    | .initialize => ⟨currentVersion, 0, .canonical⟩ }

def checkpoint (state : State) : State :=
  { state with checkpointed := state.committed }

def advisoryCheck (state : State) : Option Failure := admitImage state.checkpointed

def committedCheck (state : State) : Option Failure := admitImage state.committed

def openCheck (state : State) : Option Failure :=
  (advisoryCheck state).orElse fun _ => committedCheck state

theorem commit_preserves_advisory_image (state : State) (change : Change) :
    (commit state change).checkpointed = state.checkpointed := rfl

theorem checkpoint_preserves_committed_image (state : State) :
    (checkpoint state).committed = state.committed := rfl

theorem checkpoint_aligns_opener_with_committed_check (state : State) :
    openCheck (checkpoint state) = committedCheck state := by
  unfold openCheck advisoryCheck committedCheck checkpoint
  cases admitImage state.committed <;> rfl

theorem opener_requires_admitted_committed_image (state : State) (accepted : openCheck state = none) :
    admitted state.committed := by
  apply (admitted_image_iff _).mp
  cases advisory : advisoryCheck state with
  | none => simpa [openCheck, advisory, committedCheck] using accepted
  | some failure => simp [openCheck, advisory] at accepted

theorem admissible_commits_can_be_rejected_before_checkpoint (state : State) (failure : Failure)
    (advisory : advisoryCheck state = some failure)
    (ready : admitted state.committed) :
    openCheck state = some failure ∧ openCheck (checkpoint state) = none := by
  constructor
  · simp [openCheck, advisory]
  · rw [checkpoint_aligns_opener_with_committed_check]
    exact (admitted_image_iff _).mpr ready

end HoiminOracle.SessionWalAdmission
