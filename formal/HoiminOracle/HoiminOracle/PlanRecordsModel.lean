import HoiminOracle.PathOrderModel
import HoiminOracle.RelativePathModel

namespace HoiminOracle.PlanRecords

structure Record where
  path : String
  hash : String
  deriving DecidableEq

structure Inventory where
  sources : List Record := []
  inputs : List Record := []

inductive Kind | source | fingerprintInput
  deriving DecidableEq

inductive Invalid | path | hash | duplicate
  deriving DecidableEq

structure Failure where
  kind : Kind
  issue : Invalid
  path : String
  deriving DecidableEq

def validHash (hash : String) : Bool :=
  hash.utf8ByteSize == 64 && hash.toList.all (fun c =>
    ('0' ≤ c && c ≤ '9') || ('a' ≤ c && c ≤ 'f'))

def validateFrom (kind : Kind) (seen : List String) : List Record → Option Failure
  | [] => none
  | record :: rest =>
    if !RelativePath.lexical .portable record.path then some ⟨kind, .path, record.path⟩
    else if !validHash record.hash then some ⟨kind, .hash, record.path⟩
    else if seen.contains record.path then some ⟨kind, .duplicate, record.path⟩
    else validateFrom kind (record.path :: seen) rest

def validateSaved (saved : Inventory) : Option Failure :=
  (validateFrom .source [] saved.sources).orElse (fun _ =>
    validateFrom .fingerprintInput [] saved.inputs)

inductive ChangeKind | added | removed | modified
  deriving DecidableEq

structure Change where
  path : String
  kind : ChangeKind
  deriving DecidableEq

def digest (records : List Record) (path : String) : Option String :=
  (records.find? (fun entry => entry.path == path)).map Record.hash

def kindAt (saved current : Option String) : Option ChangeKind :=
  match saved, current with
  | none, none => none
  | none, some _ => some .added
  | some _, none => some .removed
  | some before, some after => if before = after then none else some .modified

def paths (saved current : List Record) : List String :=
  (((saved ++ current).map Record.path).eraseDups).mergeSort PathOrder.le

def changedAt (saved current : List Record) (path : String) : Option Change :=
  (kindAt (digest saved path) (digest current path)).map (fun kind => ⟨path, kind⟩)

def changes (saved current : List Record) : List Change :=
  (paths saved current).filterMap (changedAt saved current)

structure Summary where
  displayed : List Change
  omitted : Nat
  deriving DecidableEq

def displayLimit : Nat := 10

def summarize (rows : List Change) : Summary :=
  ⟨rows.take displayLimit, rows.length - displayLimit⟩

inductive Outcome
  | ok
  | invalid (failure : Failure)
  | stale (kind : Kind) (summary : Summary)
  deriving DecidableEq

def compare (saved current : Inventory) : Outcome :=
  match changes saved.sources current.sources with
  | first :: rest => .stale .source (summarize (first :: rest))
  | [] =>
    match changes saved.inputs current.inputs with
    | first :: rest => .stale .fingerprintInput (summarize (first :: rest))
    | [] => .ok

def verify (saved current : Inventory) : Outcome :=
  match validateSaved saved with
  | some failure => .invalid failure
  | none => compare saved current

theorem invalid_path_precedes_hash_and_duplicate (kind : Kind) (seen : List String)
    (record : Record) (rest : List Record) (bad : RelativePath.lexical .portable record.path = false) :
    validateFrom kind seen (record :: rest) = some ⟨kind, .path, record.path⟩ := by
  simp [validateFrom, bad]

theorem repeated_checked_path_is_rejected (kind : Kind) (seen : List String)
    (record : Record) (rest : List Record)
    (pathOk : RelativePath.lexical .portable record.path = true)
    (hashOk : validHash record.hash = true) (repeated : record.path ∈ seen) :
    validateFrom kind seen (record :: rest) = some ⟨kind, .duplicate, record.path⟩ := by
  simp [validateFrom, pathOk, hashOk, repeated]

theorem saved_source_validation_precedes_input_validation (saved : Inventory) (failure : Failure)
    (invalid : validateFrom .source [] saved.sources = some failure) :
    validateSaved saved = some failure := by
  simp [validateSaved, invalid]

theorem validation_precedes_comparison (saved current : Inventory) (failure : Failure)
    (invalid : validateSaved saved = some failure) :
    verify saved current = .invalid failure := by
  simp [verify, invalid]

theorem unchanged_iff_equal_digest (saved current : Option String) :
    kindAt saved current = none ↔ saved = current := by
  cases saved with
  | none => cases current <;> simp [kindAt]
  | some before =>
    cases current with
    | none => simp [kindAt]
    | some after => by_cases equal : before = after <;> simp [kindAt, equal]

private theorem digest_absent (records : List Record) (path : String)
    (absent : path ∉ records.map Record.path) : digest records path = none := by
  have noMatch : records.find? (fun entry => entry.path == path) = none := by
    apply List.find?_eq_none.mpr
    intro entry member same
    exact absent (List.mem_map.mpr ⟨entry, member, by simpa using same⟩)
  simp [digest, noMatch]

theorem no_changes_iff_every_digest_matches (saved current : List Record) :
    changes saved current = [] ↔ ∀ path, digest saved path = digest current path := by
  have atPath (path : String) :
      changedAt saved current path = none ↔ digest saved path = digest current path := by
    rw [← unchanged_iff_equal_digest]
    cases checked : kindAt (digest saved path) (digest current path) <;> simp [changedAt, checked]
  rw [changes, List.filterMap_eq_nil_iff]
  constructor
  · intro unchanged path
    by_cases member : path ∈ paths saved current
    · exact (atPath path).mp (unchanged path member)
    · have absentSaved : path ∉ saved.map Record.path := fun present =>
        member (by simp [paths, List.mem_eraseDups, present])
      have absentCurrent : path ∉ current.map Record.path := fun present =>
        member (by simp [paths, List.mem_eraseDups, present])
      rw [digest_absent _ _ absentSaved, digest_absent _ _ absentCurrent]
  · intro equal path _
    exact (atPath path).mpr (equal path)

theorem source_changes_precede_input_changes (saved current : Inventory) (first : Change)
    (rest : List Change) (source : changes saved.sources current.sources = first :: rest) :
    compare saved current = .stale .source (summarize (first :: rest)) := by
  simp [compare, source]

theorem display_bound (rows : List Change) : (summarize rows).displayed.length ≤ displayLimit := by
  simpa [summarize] using Nat.min_le_left displayLimit rows.length

theorem display_accounts_for_every_change (rows : List Change) :
    (summarize rows).displayed.length + (summarize rows).omitted = rows.length := by
  simp only [summarize, List.length_take]
  omega

end HoiminOracle.PlanRecords
