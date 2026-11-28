namespace HoiminOracle.GitInventory

inductive Status
  | added | modified | deleted | typeChanged | renamed | copied | unmerged | unknown | broken
  deriving DecidableEq, BEq

abbrev Record := Status × String × Option String

def destination (record : Record) : String := record.2.2.getD record.2.1

def requiresLegacy : Status → Bool
  | .renamed | .copied | .unmerged | .unknown | .broken => true
  | _ => false

structure Limits where
  paths : Nat
  bytes : Nat
  measure : String → Nat := String.utf8ByteSize

def gitLimits : Limits := ⟨128, 8192, String.utf8ByteSize⟩

def Fits (limits : Limits) (paths : List String) : Prop :=
  paths.length ≤ limits.paths ∧ (paths.map limits.measure).sum ≤ limits.bytes

instance (limits : Limits) (paths : List String) : Decidable (Fits limits paths) := by
  unfold Fits
  infer_instance

private def prefixLengths (limits : Limits) (paths : List String) : List Nat :=
  (List.range (paths.length + 1)).filter fun count => decide (Fits limits (paths.take count))

/-- A batch is the longest prefix satisfying both independent budgets. -/
def prefixSize (limits : Limits) (paths : List String) : Nat :=
  (prefixLengths limits paths).max?.getD 0

theorem prefixSize_spec (limits : Limits) (paths : List String) :
    let count := prefixSize limits paths
    count ≤ paths.length ∧ Fits limits (paths.take count) ∧
      ∀ other, other ≤ paths.length → Fits limits (paths.take other) → other ≤ count := by
  have zero : 0 ∈ prefixLengths limits paths := by simp [prefixLengths, Fits]
  cases found : (prefixLengths limits paths).max? with
  | none => simp [List.max?_eq_none_iff.mp found] at zero
  | some count =>
    have ⟨member, greatest⟩ := List.max?_eq_some_iff.mp found
    have conditions : count ≤ paths.length ∧ Fits limits (paths.take count) := by
      simpa [prefixLengths, Nat.lt_succ_iff] using member
    simpa [prefixSize, found] using And.intro conditions.1 (And.intro conditions.2
      (fun other within fits => greatest other (by simp [prefixLengths, Nat.lt_succ_iff, within, fits])))

def packAux (limits : Limits) : Nat → List String → List (List String)
  | 0, _ | _, [] => []
  | fuel + 1, path :: rest =>
    let paths := path :: rest
    let count := prefixSize limits paths
    paths.take count :: packAux limits fuel (paths.drop count)

def pack (limits : Limits) (paths : List String) : List (List String) :=
  packAux limits paths.length paths

private theorem packAux_spec (limits : Limits) (fuel : Nat) (paths : List String)
    (enough : paths.length ≤ fuel) (individual : ∀ path ∈ paths, Fits limits [path]) :
    (packAux limits fuel paths).flatten = paths ∧
      ∀ batch ∈ packAux limits fuel paths, batch ≠ [] ∧ Fits limits batch := by
  induction fuel generalizing paths with
  | zero => simp at enough; subst paths; simp [packAux]
  | succ fuel ih =>
    cases paths with
    | nil => simp [packAux]
    | cons path rest =>
      let count := prefixSize limits (path :: rest)
      have ⟨within, fits, greatest⟩ := prefixSize_spec limits (path :: rest)
      have positive : 1 ≤ count := greatest 1 (by simp) (by simpa using individual path (by simp))
      let remainder := (path :: rest).drop count
      have remainingFuel : remainder.length ≤ fuel := by
        simp [remainder, count] at *
        omega
      have remainingFits : ∀ item ∈ remainder, Fits limits [item] :=
        fun item member => individual item (List.mem_of_mem_drop member)
      have ⟨flattened, bounded⟩ := ih remainder remainingFuel remainingFits
      have nonempty : (List.take count (path :: rest)) ≠ [] := by
        intro empty
        have length := congrArg List.length empty
        simp at length
        change count ≤ (path :: rest).length at within
        omega
      change ((path :: rest).take count ++ (packAux limits fuel remainder).flatten = path :: rest) ∧ _
      refine ⟨by rw [flattened, List.take_append_drop], ?_⟩
      change ∀ batch ∈ (path :: rest).take count :: packAux limits fuel remainder, _
      intro batch member
      rcases List.mem_cons.mp member with rfl | tail
      · exact ⟨nonempty, fits⟩
      · exact bounded batch tail

theorem pack_spec (limits : Limits) (paths : List String)
    (individual : ∀ path ∈ paths, Fits limits [path]) :
    (pack limits paths).flatten = paths ∧ ∀ batch ∈ pack limits paths, batch ≠ [] ∧ Fits limits batch :=
  packAux_spec limits paths.length paths (Nat.le_refl _) individual

inductive Plan
  | legacy
  | scoped (batches : List (List String))
  deriving BEq

def planWith (limits : Limits) (scope : List String) (inventory : List Record) : Plan :=
  let selected := inventory.filter fun record => scope.contains (destination record)
  if selected.any fun record => requiresLegacy record.1 || limits.measure (destination record) > limits.bytes then
    .legacy
  else
    .scoped (pack limits ((selected.map destination).eraseDups.mergeSort (· ≤ ·)))

def plan := planWith gitLimits

theorem legacy_iff (scope : List String) (inventory : List Record) :
    plan scope inventory = .legacy ↔ ∃ record ∈ inventory, destination record ∈ scope ∧
      (requiresLegacy record.1 = true ∨ (destination record).utf8ByteSize > gitLimits.bytes) := by
  simp only [plan, planWith, gitLimits]
  split
  · simp_all [List.any_eq_true, Bool.or_eq_true]
  · rename_i noLegacy
    constructor
    · intro impossible
      cases impossible
    · intro witness
      exact False.elim (noLegacy (by simpa [List.any_eq_true, Bool.or_eq_true] using witness))

theorem scoped_spec (scope : List String) (inventory : List Record) (batches : List (List String))
    (planned : plan scope inventory = .scoped batches) :
    (∀ path, path ∈ batches.flatten ↔
      ∃ record ∈ inventory, destination record = path ∧ path ∈ scope) ∧
    ∀ batch ∈ batches, batch ≠ [] ∧ Fits gitLimits batch := by
  let selected := inventory.filter fun record => scope.contains (destination record)
  let paths := (selected.map destination).eraseDups.mergeSort (· ≤ ·)
  have member_iff (path : String) : path ∈ paths ↔
      ∃ record ∈ inventory, destination record = path ∧ path ∈ scope := by
    simp only [paths, selected, List.mem_mergeSort, List.mem_eraseDups, List.mem_map,
      List.mem_filter, List.contains_iff_mem]
    constructor
    · rintro ⟨record, ⟨fromInventory, inScope⟩, rfl⟩
      exact ⟨record, fromInventory, rfl, inScope⟩
    · rintro ⟨record, fromInventory, rfl, inScope⟩
      exact ⟨record, ⟨fromInventory, inScope⟩, rfl⟩
  have packed : pack gitLimits paths = batches := by
    change (if selected.any fun record =>
      requiresLegacy record.1 || gitLimits.measure (destination record) > gitLimits.bytes then
      Plan.legacy else Plan.scoped (pack gitLimits paths)) = .scoped batches at planned
    split at planned
    · cases planned
    · exact Plan.scoped.inj planned
  have individual : ∀ path ∈ paths, Fits gitLimits [path] := by
    intro path member
    obtain ⟨record, fromInventory, rfl, inScope⟩ := (member_iff path).mp member
    by_cases byteBound : (destination record).utf8ByteSize ≤ gitLimits.bytes
    · simpa [Fits, gitLimits] using byteBound
    · have fallback := (legacy_iff scope inventory).mpr
        ⟨record, fromInventory, inScope, .inr (by omega)⟩
      rw [planned] at fallback
      cases fallback
  have ⟨allPaths, fit⟩ := pack_spec gitLimits paths individual
  rw [packed] at allPaths fit
  exact ⟨fun path => by rw [allPaths]; exact member_iff path, fit⟩

end HoiminOracle.GitInventory
