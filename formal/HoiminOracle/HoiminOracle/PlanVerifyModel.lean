namespace HoiminOracle.PlanVerify

inductive Stored | missing | valid | hash | original | stable | symbol | sequence
  deriving DecidableEq

structure Entry where
  id : Nat
  path : Nat
  stored : Stored
  deriving DecidableEq

structure Target where
  path : Nat
  discovery : List Nat

structure Input where
  entries : List Entry
  requested : List Nat
  targets : List Target
  readable : List Nat

inductive Reason | unknown | unselected | unreadable | hash | original | stable | undiscovered | descriptor
  deriving DecidableEq

structure Failure where
  id : Nat
  reason : Reason
  deriving DecidableEq

structure Inspection where
  checkedPaths : List Nat
  failure : Option Failure := none

def lookup (entries : List Entry) (id : Nat) : Option Entry :=
  entries.find? fun entry => entry.id == id && entry.stored != .missing

def requestedIds (ids : List Nat) : List Nat :=
  ids.eraseDups.mergeSort (· ≤ ·)

def inspect (input : Input) (seen : List Nat) : List Nat → Inspection
  | [] => ⟨seen, none⟩
  | id :: rest =>
    match lookup input.entries id with
    | none => ⟨seen, some ⟨id, .unknown⟩⟩
    | some entry =>
      if !(input.targets.any (fun target => target.path == entry.path)) then
        ⟨seen, some ⟨id, .unselected⟩⟩
      else if !input.readable.contains entry.path then
        ⟨seen, some ⟨id, .unreadable⟩⟩
      else
        let used := if seen.contains entry.path then seen else entry.path :: seen
        match entry.stored with
        | .hash => ⟨used, some ⟨id, .hash⟩⟩
        | .original => ⟨used, some ⟨id, .original⟩⟩
        | .stable => ⟨used, some ⟨id, .stable⟩⟩
        | _ => inspect input used rest

def projectedTargets (input : Input) (paths : List Nat) : List Target :=
  input.targets.filter fun target => paths.contains target.path

def compareFound (entries : List Entry) (found : List Nat) (id : Nat) : Option Failure :=
  if !found.contains id then some ⟨id, .undiscovered⟩
  else if (lookup entries id).any (fun entry => entry.stored == .symbol) then
    some ⟨id, .descriptor⟩
  else none

def confirm (entries : List Entry) (requested found : List Nat) : Except Failure (List Nat) :=
  match requested.findSome? (compareFound entries found) with
  | some failure => .error failure
  | none => .ok (found.filter requested.contains)

structure Observation where
  contexts : Nat
  result : Except Failure (List Nat)

def verify (input : Input) : Observation :=
  let ids := requestedIds input.requested
  let inspected := inspect input [] ids
  let result := match inspected.failure with
    | some failure => .error failure
    | none =>
      let found := (projectedTargets input inspected.checkedPaths).flatMap Target.discovery
      confirm input.entries ids found
  ⟨inspected.checkedPaths.length, result⟩

theorem unknown_precedes_any_file_work (input : Input) (seen : List Nat) (id : Nat)
    (rest : List Nat) (missing : lookup input.entries id = none) :
    inspect input seen (id :: rest) = ⟨seen, some ⟨id, .unknown⟩⟩ := by
  simp [inspect, missing]

theorem descriptor_failure_precedes_discovery (input : Input) (failure : Failure)
    (failed : (inspect input [] (requestedIds input.requested)).failure = some failure) :
    (verify input).result = .error failure := by
  simp [verify, failed]

theorem projected_targets_preserve_order (input : Input) (paths : List Nat) :
    (projectedTargets input paths).Sublist input.targets :=
  List.filter_sublist

theorem confirm_returns_exact_discovery_projection (entries : List Entry)
    (requested found selected : List Nat) (accepted : confirm entries requested found = .ok selected) :
    selected = found.filter requested.contains := by
  simp only [confirm] at accepted
  split at accepted <;> simp_all

theorem confirmation_checks_every_requested_descriptor (entries : List Entry)
    (requested found selected : List Nat) (accepted : confirm entries requested found = .ok selected)
    (id : Nat) (member : id ∈ requested) : compareFound entries found id = none := by
  have noErrors : requested.findSome? (compareFound entries found) = none := by
    cases outcome : requested.findSome? (compareFound entries found) with
    | none => rfl
    | some failure => simp [confirm, outcome] at accepted
  exact List.findSome?_eq_none_iff.mp noErrors id member

theorem confirmed_membership (entries : List Entry) (requested found selected : List Nat)
    (accepted : confirm entries requested found = .ok selected) (id : Nat) :
    id ∈ selected ↔ id ∈ requested := by
  rw [confirm_returns_exact_discovery_projection _ _ _ _ accepted]
  simp only [List.mem_filter, List.contains_iff_mem]
  refine ⟨And.right, fun member => ⟨?_, member⟩⟩
  by_cases present : id ∈ found
  · exact present
  · have noFailure := confirmation_checks_every_requested_descriptor _ _ _ _ accepted id member
    simp [compareFound, present] at noFailure

end HoiminOracle.PlanVerify
