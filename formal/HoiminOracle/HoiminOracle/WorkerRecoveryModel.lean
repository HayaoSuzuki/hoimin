namespace HoiminOracle.WorkerRecovery

inductive Owner where
  | absent
  | active (generation : Nat)
  | pending (generation : Nat)
  deriving DecidableEq

def Owner.generation : Owner → Option Nat
  | .absent => none
  | .active n | .pending n => some n

def Owner.slots (owner : Owner) : Nat := owner.generation.toList.length

def Owner.copyBytes (owner : Owner) (logicalBytes : Nat) : Nat := owner.slots * logicalBytes

structure State where
  owner : Owner
  next : Nat
  deriving DecidableEq

def initial : State := ⟨.active 0, 1⟩

inductive Operation where
  | reset
  | restoreFault
  | discardFault
  | depthFault
  | create
  | foreign
  | foreignWithCleanupFault
  | retryCleanupFault
  | copyFault
  | importFault
  deriving DecidableEq

inductive Outcome where
  | reset
  | created
  | workerMissing
  | restore
  | depth
  | foreign
  | io
  | importRoot
  deriving DecidableEq

inductive Diagnostic where
  | restore
  | discard
  | depth
  | cleanup
  | copy
  deriving DecidableEq

structure Result where
  state : State
  outcome : Outcome
  diagnostics : List Diagnostic := []
  admitted : Bool := true
  consumedFault : Option Bool := none
  deriving DecidableEq

private def allocate (state : State) (importFault : Bool) : Result :=
  if importFault then
    ⟨⟨.pending state.next, state.next + 1⟩, .importRoot, [], true, none⟩
  else
    ⟨⟨.active state.next, state.next + 1⟩, .created, [], true, none⟩

private def resetWith (state : State) (result : Nat → Result) (fault : Option Bool := none) : Result :=
  match state.owner with
  | .active generation => result generation
  | _ => ⟨state, .workerMissing, [], false, fault⟩

private def createWith (state : State) (result : Result) (fault : Option Bool := none) : Result :=
  match state.owner with
  | .active _ => ⟨state, .workerMissing, [], false, fault⟩
  | _ => result

def step (state : State) : Operation → Result
  | .reset => resetWith state (fun _ => ⟨state, .reset, [], true, none⟩)
  | .restoreFault => resetWith state (fun _ => ⟨{ state with owner := .absent }, .restore, [.restore], true, none⟩)
  | .discardFault => resetWith state
      (fun n => ⟨{ state with owner := .pending n }, .restore, [.restore, .discard], true, some true⟩) (some false)
  | .depthFault => resetWith state (fun n => ⟨{ state with owner := .pending n }, .depth, [.depth], true, none⟩)
  | .create => createWith state (allocate state false)
  | .foreign => createWith state ⟨state, .foreign, [], true, none⟩
  | .foreignWithCleanupFault => createWith state ⟨state, .foreign, [], true, some false⟩ (some false)
  | .retryCleanupFault => createWith state (match state.owner with
      | .pending _ => ⟨state, .io, [.cleanup], true, some true⟩
      | _ => { allocate state false with consumedFault := some false }) (some false)
  | .copyFault => createWith state ⟨{ state with owner := .absent }, .io, [.copy], true, some true⟩ (some false)
  | .importFault => createWith state (allocate state true)

def runWith (next : State → Operation → Result) (state : State) : List Operation → List Result
  | [] => []
  | op :: rest =>
    let result := next state op
    result :: runWith next result.state rest

def Valid (state : State) : Prop :=
  ∀ n, state.owner.generation = some n → n < state.next

-- A foreign grant plus an armed cleanup fault gives an observable precedence witness.
example : (step ⟨.pending 0, 1⟩ .foreignWithCleanupFault).outcome = .foreign := by decide

theorem step_preserves_generation_order (state : State) (op : Operation) (holds : Valid state) :
    Valid (step state op).state := by
  rcases state with ⟨owner, next⟩
  cases owner <;> cases op <;>
    simp [Valid, Owner.generation, step, resetWith, createWith, allocate] at holds ⊢ <;> assumption

theorem trace_preserves_generation_order (state : State) (trace : List Operation) (holds : Valid state) :
    ∀ result ∈ runWith step state trace, Valid result.state := by
  induction trace generalizing state with
  | nil => simp [runWith]
  | cons op rest ih =>
    intro result membership
    simp [runWith] at membership
    have next := step_preserves_generation_order state op holds
    rcases membership with rfl | later
    · exact next
    · exact ih (step state op).state next result later

theorem foreign_grant_preserves_pending (generation next : Nat) :
    step ⟨.pending generation, next⟩ .foreignWithCleanupFault =
      ⟨⟨.pending generation, next⟩, .foreign, [], true, some false⟩ := rfl

theorem discard_fault_keeps_owner_and_error_order (generation next : Nat) :
    step ⟨.active generation, next⟩ .discardFault =
      ⟨⟨.pending generation, next⟩, .restore, [.restore, .discard], true, some true⟩ := rfl

theorem depth_fault_keeps_owner_and_primary_diagnostic (generation next : Nat) :
    step ⟨.active generation, next⟩ .depthFault =
      ⟨⟨.pending generation, next⟩, .depth, [.depth], true, none⟩ := rfl

theorem failed_replacement_releases_pending (generation next : Nat) :
    (step ⟨.pending generation, next⟩ .copyFault).state.owner = .absent := rfl

theorem discard_fault_retains_charge (generation next logicalBytes : Nat) :
    (step ⟨.active generation, next⟩ .discardFault).state.owner.copyBytes logicalBytes = logicalBytes := by
  simp [step, resetWith, Owner.copyBytes, Owner.slots, Owner.generation]

theorem failed_replacement_releases_charge (generation next logicalBytes : Nat) :
    (step ⟨.pending generation, next⟩ .copyFault).state.owner.copyBytes logicalBytes = 0 := by
  simp [failed_replacement_releases_pending, Owner.copyBytes, Owner.slots, Owner.generation]

theorem successful_replacement_is_fresh (generation next : Nat) (holds : Valid ⟨.pending generation, next⟩) :
    (step ⟨.pending generation, next⟩ .create).state.owner.generation = some next ∧ generation ≠ next :=
  ⟨rfl, Nat.ne_of_lt (holds generation rfl)⟩

theorem one_slot_bounds_copy_charge (owner : Owner) (logicalBytes : Nat) :
    owner.copyBytes logicalBytes ≤ logicalBytes := by
  cases owner <;> simp [Owner.copyBytes, Owner.slots, Owner.generation]

end HoiminOracle.WorkerRecovery
