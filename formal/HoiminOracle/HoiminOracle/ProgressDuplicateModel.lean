namespace HoiminOracle.ProgressDuplicate

structure Input where
  path : Nat
  fingerprint : Nat
  deriving DecidableEq

inductive Reason | samePath | identicalBytes
  deriving DecidableEq

structure Duplicate where
  previous : Nat
  reason : Reason
  deriving DecidableEq

abbrev Files := List (Option Nat)

def contents (files : Files) (path : Nat) : Option Nat :=
  files[path]?.join

def sameBytes (files : Files) (left right : Nat) : Bool :=
  match contents files left, contents files right with
  | some a, some b => a == b
  | _, _ => false

def samePath (current previous : Input) : Bool := previous.path == current.path

def confirmed (files : Files) (current previous : Input) : Bool :=
  previous.fingerprint == current.fingerprint && sameBytes files previous.path current.path

def classifyWith (confirm : Files → Input → Input → Bool)
    (files : Files) (history : List Input) (current : Input) : Option Duplicate :=
  match history.findIdx? (samePath current) with
  | some previous => some ⟨previous, .samePath⟩
  | none => (history.findIdx? (confirm files current)).map (⟨·, .identicalBytes⟩)

def classify := classifyWith confirmed

structure Edit where
  path : Nat
  bytes : Option Nat

structure Action where
  input : Input
  edit : Option Edit := none

def applyEdit (files : Files) : Option Edit → Files
  | none => files
  | some edit => files.set edit.path edit.bytes

def annotateWith (choose : Files → List Input → Input → Option Duplicate)
    (files : Files) (history : List Input) : List Action → List (Input × Option Duplicate)
  | [] => []
  | action :: rest =>
    let updated := applyEdit files action.edit
    (action.input, choose updated history action.input) ::
      annotateWith choose updated (history ++ [action.input]) rest

theorem earliest_same_path_takes_precedence (files : Files) (history : List Input)
    (current : Input) (index : Nat) :
    classify files history current = some ⟨index, .samePath⟩ ↔
      ∃ bound : index < history.length,
        samePath current history[index] ∧
          ∀ earlier (before : earlier < index),
            ¬samePath current (history[earlier]'(Nat.lt_trans before bound)) := by
  rw [← List.findIdx?_eq_some_iff_getElem]
  cases found : history.findIdx? (samePath current) with
  | some first => simp [classify, classifyWith, found]
  | none =>
    simp only [classify, classifyWith, found]
    cases history.findIdx? (confirmed files current) <;> simp

theorem exact_match_is_earliest (files : Files) (history : List Input) (current : Input)
    (index : Nat) (noPath : history.findIdx? (samePath current) = none) :
    classify files history current = some ⟨index, .identicalBytes⟩ ↔
      ∃ bound : index < history.length,
        confirmed files current history[index] ∧
          ∀ earlier (before : earlier < index),
            ¬confirmed files current (history[earlier]'(Nat.lt_trans before bound)) := by
  simp only [classify, classifyWith, noPath]
  rw [← List.findIdx?_eq_some_iff_getElem]
  cases history.findIdx? (confirmed files current) <;> simp

theorem unavailable_file_cannot_confirm (files : Files) (current previous : Input)
    (missing : contents files previous.path = none ∨ contents files current.path = none) :
    confirmed files current previous = false := by
  rcases missing with h | h <;> simp [confirmed, sameBytes, h]

theorem annotation_preserves_input_order (choose : Files → List Input → Input → Option Duplicate)
    (files : Files) (history : List Input) (actions : List Action) :
    (annotateWith choose files history actions).map Prod.fst = actions.map Action.input := by
  induction actions generalizing files history with
  | nil => rfl
  | cons action rest ih => simp [annotateWith, ih]

end HoiminOracle.ProgressDuplicate
