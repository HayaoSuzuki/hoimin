namespace HoiminOracle.ReviewStoreDestination

inductive Slot | database | journal | wal | shm
  deriving DecidableEq

def order : List Slot := [.database, .journal, .wal, .shm]

def Slot.suffix : Slot → String
  | .database => ""
  | .journal => "-journal"
  | .wal => "-wal"
  | .shm => "-shm"

/-- Outcomes of inspecting an entry. File numbers denote descriptor identity. -/
inductive Entry | missing | nonregular | file (identity : Nat) | ioError
  deriving DecidableEq

inductive Failure | destinationKind | aliasesInput | ioError
  deriving DecidableEq

def checkInput (destination : Nat) : Entry → Option Failure
  | .missing | .nonregular => none
  | .file identity => if destination = identity then some .aliasesInput else none
  | .ioError => some .ioError

def checkEntry (inputs : List Entry) : Entry → Option Failure
  | .missing => none
  | .nonregular => some .destinationKind
  | .ioError => some .ioError
  | .file identity => inputs.findSome? (checkInput identity)

def validateInOrder (destinations : List (Slot × Entry)) (inputs : List Entry) :
    Option (Slot × Failure) :=
  destinations.findSome? fun (slot, entry) => (checkEntry inputs entry).map (slot, ·)

def validate (destinations : Slot → Entry) (inputs : List Entry) : Option (Slot × Failure) :=
  validateInOrder (order.map fun slot => (slot, destinations slot)) inputs

private theorem admitted_list_has_no_failures {α β : Type} (observe : α → Option β) (items : List α) :
    items.findSome? observe = none ↔ ∀ item ∈ items, observe item = none := by
  induction items with
  | nil => simp
  | cons head tail ih =>
    cases found : observe head <;> simp [found, ih]

theorem missing_destination_does_not_inspect_protected_inputs (inputs : List Entry) :
    checkEntry inputs .missing = none := rfl

theorem alias_cannot_pass (inputs : List Entry) (identity : Nat)
    (sameFile : Entry.file identity ∈ inputs) :
    checkEntry inputs (.file identity) ≠ none := by
  intro accepted
  have matched := (admitted_list_has_no_failures _ _).mp accepted _ sameFile
  simp [checkInput] at matched

theorem accepted_destination_has_no_protected_alias (destinations : Slot → Entry)
    (inputs : List Entry) (identity : Nat) (slot : Slot)
    (regular : destinations slot = .file identity)
    (accepted : validate destinations inputs = none) :
    Entry.file identity ∉ inputs := by
  have visits : slot ∈ order := by cases slot <;> simp [order]
  have matched := (admitted_list_has_no_failures _ _).mp accepted
    (slot, destinations slot) (List.mem_map.mpr ⟨slot, visits, rfl⟩)
  simp only [regular, Option.map_eq_none_iff] at matched
  intro sameFile
  exact alias_cannot_pass inputs identity sameFile matched

theorem first_failed_destination_wins (earlier later : List (Slot × Entry)) (inputs : List Entry)
    (failure : Slot × Failure) (found : validateInOrder earlier inputs = some failure) :
    validateInOrder (earlier ++ later) inputs = some failure := by
  simp [validateInOrder, List.findSome?_append, show earlier.findSome? _ = some failure from found]

theorem first_failed_protected_input_wins (earlier later : List Entry) (identity : Nat)
    (failure : Failure) (found : checkEntry earlier (.file identity) = some failure) :
    checkEntry (earlier ++ later) (.file identity) = some failure := by
  simp [checkEntry, List.findSome?_append, show earlier.findSome? _ = some failure from found]

end HoiminOracle.ReviewStoreDestination
