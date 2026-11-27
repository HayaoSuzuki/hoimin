namespace HoiminOracle.WorkspaceCapacity

def maximumBytes : Nat := 2^64 - 1

structure Limits where
  copy : Option Nat := none
  owned : Option Nat := none

inductive Failure where
  | zeroWorkers
  | overflow
  | copyLimit (requested limit : Nat)
  | ownedLimit (requested limit : Nat)
  | grew
  | shrank
  | originalChanged
  deriving DecidableEq

inductive Admission where
  | admit (aggregate : Nat)
  | reject (failure : Failure)
  deriving DecidableEq

def check (limits : Limits) (logical workers : Nat) : Admission :=
  let copies := logical * workers
  let owned := logical * (workers + 1)
  let copyLimit := limits.copy.getD maximumBytes
  if copies > maximumBytes then .reject .overflow
  else if copies > copyLimit then .reject (.copyLimit copies copyLimit)
  else if owned > maximumBytes then .reject .overflow
  else if limits.owned.any (fun limit => decide (limit ≤ owned)) then
    .reject (.ownedLimit owned (limits.owned.getD maximumBytes))
  else .admit copies

theorem admitted_respects_limits (limits : Limits) (logical workers aggregate : Nat)
    (admitted : check limits logical workers = .admit aggregate) :
    aggregate = logical * workers ∧ aggregate ≤ maximumBytes ∧
      aggregate ≤ limits.copy.getD maximumBytes ∧ logical * (workers + 1) ≤ maximumBytes ∧
      (∀ limit, limits.owned = some limit → logical * (workers + 1) < limit) := by
  simp only [check] at admitted
  split at admitted
  · contradiction
  split at admitted
  · contradiction
  split at admitted
  · contradiction
  split at admitted
  · contradiction
  · rename_i fitsCopy fitsAllowance fitsOwned belowOwned
    cases admitted
    refine ⟨rfl, Nat.le_of_not_gt fitsCopy, Nat.le_of_not_gt fitsAllowance,
      Nat.le_of_not_gt fitsOwned, ?_⟩
    intro limit present
    simpa [present, Nat.not_le] using belowOwned

theorem copy_error_precedes_owned (limits : Limits) (logical workers : Nat)
    (noOverflow : logical * workers ≤ maximumBytes)
    (overCopy : limits.copy.getD maximumBytes < logical * workers) :
    check limits logical workers = .reject (.copyLimit (logical * workers) (limits.copy.getD maximumBytes)) := by
  simp [check, Nat.not_lt_of_ge noOverflow, overCopy]

theorem copy_equality_admitted (logical workers : Nat) (owned : Option Nat)
    (fits : logical * (workers + 1) ≤ maximumBytes)
    (below : ∀ limit, owned = some limit → logical * (workers + 1) < limit) :
    check { copy := some (logical * workers), owned } logical workers = .admit (logical * workers) := by
  have copyFits : logical * workers ≤ maximumBytes :=
    Nat.le_trans (Nat.mul_le_mul_left logical (Nat.le_succ workers)) fits
  cases owned with
  | none => simp [check, Nat.not_lt_of_ge fits, Nat.not_lt_of_ge copyFits]
  | some limit => simp [check, Nat.not_lt_of_ge fits, Nat.not_lt_of_ge copyFits, Nat.not_le_of_gt (below limit rfl)]

theorem owned_equality_rejected (logical workers : Nat) (fits : logical * (workers + 1) ≤ maximumBytes) :
    check { copy := some (logical * workers), owned := some (logical * (workers + 1)) } logical workers =
      .reject (.ownedLimit (logical * (workers + 1)) (logical * (workers + 1))) := by
  have copyFits : logical * workers ≤ maximumBytes :=
    Nat.le_trans (Nat.mul_le_mul_left logical (Nat.le_succ workers)) fits
  simp [check, Nat.not_lt_of_ge fits, Nat.not_lt_of_ge copyFits]

structure Writes where
  count : Nat := 0
  bytes : Nat := 0
  failure : Option Failure := none
  deriving DecidableEq

def writeChunks (admit : Nat → Admission) (previous : Nat) : List Nat → Writes
  | [] => {}
  | chunk :: rest =>
    match admit (previous + chunk) with
    | .reject failure => { failure := some failure }
    | .admit _ =>
      let later := writeChunks admit (previous + chunk) rest
      { later with count := 1 + later.count, bytes := chunk + later.bytes }

theorem written_bytes_are_a_prefix (admit : Nat → Admission) (previous : Nat) (chunks : List Nat) :
    let written := writeChunks admit previous chunks
    written.bytes = (chunks.take written.count).sum := by
  induction chunks generalizing previous with
  | nil => rfl
  | cons chunk rest ih =>
    cases admission : admit (previous + chunk) with
    | reject failure => simp [writeChunks, admission]
    | admit aggregate => simp [writeChunks, admission, Nat.one_add, ih (previous + chunk)]

theorem written_prefix_passes_admission (admit : Nat → Admission) (previous : Nat) (chunks : List Nat)
    (wrote : 0 < (writeChunks admit previous chunks).count) :
    ∃ aggregate, admit (previous + (writeChunks admit previous chunks).bytes) = .admit aggregate := by
  induction chunks generalizing previous with
  | nil => simp [writeChunks] at wrote
  | cons chunk rest ih =>
    cases admission : admit (previous + chunk) with
    | reject failure => simp [writeChunks, admission] at wrote
    | admit aggregate =>
      by_cases later : 0 < (writeChunks admit (previous + chunk) rest).count
      · simpa [writeChunks, admission, Nat.add_assoc] using ih (previous + chunk) later
      · have noBytes : (writeChunks admit (previous + chunk) rest).bytes = 0 := by
          rw [written_bytes_are_a_prefix]
          simp [Nat.eq_zero_of_not_pos later]
        exact ⟨aggregate, by simp [writeChunks, admission, noBytes]⟩

-- Each input file is opened once for the snapshot and read using its descriptor length.
structure FileObservation where
  metadataBytes : Nat
  descriptorBytes : Nat
  currentBytes : Nat

def blockBytes : Nat := 65536

private def blocks (bytes : Nat) : List Nat :=
  List.replicate (bytes / blockBytes) blockBytes ++
    (if bytes % blockBytes = 0 then [] else [bytes % blockBytes])

private def copyFiles (admit chunkAdmission : Nat → Admission) (written : Writes) : List FileObservation → Writes
  | [] => written
  | file :: rest =>
    match admit (written.bytes + file.metadataBytes) with
    | .reject failure => { written with failure := some failure }
    | .admit _ =>
      let readable := min file.currentBytes file.descriptorBytes
      let next := writeChunks chunkAdmission written.bytes (blocks readable)
      let combined : Writes := ⟨written.count + next.count, written.bytes + next.bytes, next.failure⟩
      if combined.failure.isSome then combined
      else if file.descriptorBytes < file.currentBytes then { combined with failure := some .grew }
      else if file.currentBytes < file.descriptorBytes then { combined with failure := some .shrank }
      else copyFiles admit chunkAdmission combined rest

inductive Edit where
  | beforeSnapshot (bytes : Nat)
  | addFile
  | beforeDescriptor (bytes : Nat)
  | afterDescriptor (bytes : Nat)
  | validation (bytes : Nat)

structure Run where
  workers : Nat := 1
  limits : Limits := {}
  initial : Nat
  edit : Option Edit := none
  duplicate : Bool := false

structure Observation where
  admission : Admission
  snapshotStarted : Bool := false
  count : Nat := 0
  writtenBytes : Nat := 0
  validation : Option Nat := none
  sourceBytes : Nat
  added : Bool := false
  deriving DecidableEq

def runWith (gate chunkGate : Limits → Nat → Nat → Admission) (input : Run)
    (checkInventory : Bool := true) : Observation :=
  let start result : Observation := { admission := result, sourceBytes := input.initial }
  if input.workers = 0 then start (.reject .zeroWorkers)
  else
    let admit bytes := gate input.limits bytes input.workers
    let inventory := if checkInventory && (input.limits.copy.isSome || input.limits.owned.isSome)
      then admit input.initial else .admit 0
    match inventory with
    | .reject failure => start (.reject failure)
    | .admit _ =>
      let metadata := match input.edit with | some (.beforeSnapshot bytes) => bytes | _ => input.initial
      let descriptor := match input.edit with | some (.beforeDescriptor bytes) => bytes | _ => metadata
      let current := match input.edit with | some (.afterDescriptor bytes) => bytes | _ => descriptor
      let added := match input.edit with | some .addFile => true | _ => false
      let file : FileObservation := ⟨metadata, descriptor, current⟩
      -- For the two-file cases the added file and the original have equal lengths.
      let written := copyFiles admit (fun bytes => chunkGate input.limits bytes input.workers)
        {} (if added then [file, file] else [file])
      match written.failure with
      | some failure => ⟨.reject failure, true, written.count, written.bytes, none, current, added⟩
      | none =>
        let final := match input.edit with | some (.validation bytes) => bytes | _ => current
        let result := if final = current then admit written.bytes else .reject .originalChanged
        ⟨result, true, written.count, written.bytes, some written.bytes, final, added⟩

def run (input : Run) := runWith check check input

theorem duplicate_visits_do_not_add_charge (input : Run) :
    run { input with duplicate := true } = run { input with duplicate := false } := rfl

end HoiminOracle.WorkspaceCapacity
