import HoiminOracle.GitPatchModel

namespace HoiminOracle.GitNumstat

open HoiminOracle.GitPatch (Scope isPython)

inductive Counts
  | text | binary | dashAdded | dashDeleted
  deriving BEq

inductive Name
  | file (path : String)
  | backslash
  | invalidUtf8

inductive RenameProblem
  | missingBoth | missingNew | emptyOld | emptyNew

inductive Record
  | ordinary (counts : Counts) (name : Name)
  | renamed (counts : Counts) (source target : Name)
  | missingColumns
  | incompleteRename (counts : Counts) (problem : RenameProblem)
  | padding

inductive Diagnostic
  | record | rename | invalidUtf8 | backslash
  deriving BEq

private def choose (scope : Scope) (path : String) : List String :=
  if isPython path && scope.all (·.contains path) then [path] else []

def nameExclusions (scope : Scope) : Name → Except Diagnostic (List String)
  | .file path => .ok (choose scope path)
  | .backslash => if scope.isSome then .ok [] else .error .backslash
  | .invalidUtf8 => .error .invalidUtf8

def candidates : Record → Except Diagnostic (List Name)
  | .missingColumns => .error .record
  | .incompleteRename _ _ => .error .rename
  | .ordinary .binary name => .ok [name]
  | .renamed .binary source target => .ok [source, target]
  | _ => .ok []

def recordExclusions (scope : Scope) (record : Record) : Except Diagnostic (List String) := do
  let names ← candidates record
  return (← names.mapM (nameExclusions scope)).flatten

def exclusions (scope : Scope) (records : List Record) : Except Diagnostic (List String) := do
  return (← records.mapM (recordExclusions scope)).flatten

def observe (scope : Scope) (seed : List String) (records : List Record) :
    Except Diagnostic (List String) := do
  return seed ++ (← exclusions scope records)

theorem rename_membership (scope : Scope) (source target path : String) :
    ∃ selected, recordExclusions scope (.renamed .binary (.file source) (.file target)) = .ok selected ∧
      (path ∈ selected ↔ (path = source ∨ path = target) ∧
        isPython path = true ∧ scope.all (·.contains path) = true) := by
  refine ⟨choose scope source ++ choose scope target, ?_, ?_⟩
  · simp [recordExclusions, candidates, nameExclusions, bind, pure, Except.bind, Except.pure]
  · have member (name : String) : path ∈ choose scope name ↔ path = name ∧
         isPython path = true ∧ scope.all (·.contains path) = true := by
      unfold choose
      split
      · rename_i chosen
        simp only [Bool.and_eq_true] at chosen
        simp only [List.mem_singleton]
        constructor
        · rintro rfl
          exact ⟨rfl, chosen⟩
        · exact And.left
      · rename_i rejected
        simp only [List.not_mem_nil, false_iff]
        rintro ⟨rfl, python, inScope⟩
        exact rejected (Bool.and_eq_true_iff.mpr ⟨python, inScope⟩)
    simp only [List.mem_append, member, or_and_right]

theorem exclusions_append (scope : Scope) (first second : List Record) :
    exclusions scope (first ++ second) = (do
      let left ← exclusions scope first
      let right ← exclusions scope second
      return left ++ right) := by
  unfold exclusions
  rw [List.mapM_append]
  cases first.mapM (recordExclusions scope) <;> cases second.mapM (recordExclusions scope) <;>
    simp [bind, pure, Except.bind, Except.pure]

theorem replay_preserves_membership (scope : Scope) (seed : List String) (records : List Record)
    (paths : List String) (checked : exclusions scope records = .ok paths) (path : String) :
    ∃ first repeated, observe scope seed records = .ok first ∧
      observe scope seed (records ++ records) = .ok repeated ∧ (path ∈ repeated ↔ path ∈ first) := by
  refine ⟨seed ++ paths, seed ++ (paths ++ paths), ?_, ?_, by simp⟩
  · simp [observe, checked, bind, pure, Except.bind, Except.pure]
  · simp [observe, exclusions_append, checked, bind, pure, Except.bind, Except.pure]

def survivingPaths (changed excluded : List String) : List String :=
  changed.filter fun path => !excluded.contains path

theorem survives_iff (changed excluded : List String) (path : String) :
    path ∈ survivingPaths changed excluded ↔ path ∈ changed ∧ path ∉ excluded := by
  simp [survivingPaths]

end HoiminOracle.GitNumstat
