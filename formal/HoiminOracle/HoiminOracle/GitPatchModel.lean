namespace HoiminOracle.GitPatch

inductive Diagnostic
  | quotedPath | pathEscape | octalEscape | pathUtf8 | backslash
  | hunkHeader | hunkStart | hunkCount | hunkOverflow
  deriving BEq

inductive Header
  | file (path : String)
  | timestamped (path : String)
  | quotedCafe
  | absent
  | backslash
  | badQuote | badEscape | badOctal | badUtf8

inductive Number
  | decimal (value : Nat)
  | invalid

inductive Hunk
  | coordinates (oldStart : Nat) (newStart : Number) (count : Option Number)
  | missingDestination

inductive Line
  | old (header : Header)
  | new (header : Header)
  | hunk (hunk : Hunk)
  | payload

structure Span where
  start : Nat
  finish : Nat
  deriving BEq

inductive Effect
  | changed (path : String) (span : Span)
  | excluded (path : String)
  deriving BEq

abbrev Scope := Option (List String)

def headerPath (scope : Scope) (header : Header) : Except Diagnostic (Option String) :=
  let selected (path : String) :=
    if scope.all (·.contains path) then some path else none
  match header with
  | .file path | .timestamped path => .ok (selected path)
  | .quotedCafe => .ok (selected "pkg/café.py")
  | .absent => .ok none
  | .backslash => if scope.isSome then .ok none else .error .backslash
  | .badQuote => .error .quotedPath
  | .badEscape => .error .pathEscape
  | .badOctal => .error .octalEscape
  | .badUtf8 => .error .pathUtf8

def maximumLine : Nat := 4294967295

def bounded (kind : Diagnostic) : Number → Except Diagnostic Nat
  | .decimal value => if value ≤ maximumLine then .ok value else .error kind
  | .invalid => .error kind

def checkedRange (start count : Nat) : Except Diagnostic (Option Span) :=
  if count = 0 then .ok none
  else if start + count - 1 ≤ maximumLine then .ok (some ⟨start, start + count - 1⟩)
  else .error .hunkOverflow

theorem checked_range_iff (start count : Nat) (range : Span) :
    checkedRange start count = .ok (some range) ↔
      count > 0 ∧ range = ⟨start, start + count - 1⟩ ∧ range.finish ≤ maximumLine := by
  unfold checkedRange
  split
  · simp_all
  · rename_i nonzero
    have positive : 0 < count := by omega
    split
    · simp_all [eq_comm]
    · constructor
      · intro impossible
        cases impossible
      · rintro ⟨_, rfl, bound⟩
        contradiction

def hunkRange : Hunk → Except Diagnostic (Option Span)
  | .missingDestination => .error .hunkHeader
  | .coordinates _ start count => do
    let start ← bounded .hunkStart start
    let count ← bounded .hunkCount (count.getD (.decimal 1))
    checkedRange start count

theorem old_coordinates_irrelevant (oldFirst oldSecond : Nat) (start : Number) (count : Option Number) :
    hunkRange (.coordinates oldFirst start count) = hunkRange (.coordinates oldSecond start count) := rfl

private def oldHeader : Line → Option Header
  | .old header => some header
  | _ => none

private def newHeader : Line → Option Header
  | .new header => some header
  | _ => none

private def findHeader (test : Line → Option Header) : List Line → Option (Header × List Line)
  | [] => none
  | line :: suffix =>
    match test line with
    | some header => some (header, suffix)
    | none => findHeader test suffix

def isPython (path : String) : Bool :=
  let name := (path.splitOn "/").getLast?.getD ""
  name.length > 3 && name.toLower.endsWith ".py"

private def bodyRanges (body : List Line) : Except Diagnostic (List Span) := do
  let hunks := body.filterMap fun
    | .hunk hunk => some hunk
    | _ => none
  return (← hunks.mapM hunkRange).filterMap id

private def bodyEffects (source target : Option String) (body : List Line) :
    Except Diagnostic (List Effect) := do
  let ranges ← bodyRanges body
  return match target, source with
    | some path, _ => if isPython path then ranges.map (.changed path) else []
    | none, some path => if isPython path then [.excluded path] else []
    | none, none => []

/-- Sections have their own header search, with an optional body after the new header. -/
def observeSection (scope : Scope) (lines : List Line) : Except Diagnostic (List Effect) := do
  let some (old, rest) := findHeader oldHeader lines | return []
  let source ← headerPath scope old
  let some (new, body) := findHeader newHeader rest | return []
  let target ← headerPath scope new
  bodyEffects source target body

def observe (scope : Scope) (sections : List (List Line)) : Except Diagnostic (List Effect) := do
  return (← sections.mapM (observeSection scope)).flatten

theorem header_lookalikes_in_body (scope : Scope) (old new extraOld extraNew : Header)
    (before after : List Line) :
    observeSection scope (.old old :: .new new :: before ++ [.old extraOld, .new extraNew] ++ after) =
      observeSection scope (.old old :: .new new :: before ++ after) := by
  simp [observeSection, findHeader, oldHeader, newHeader, bodyEffects, bodyRanges]

theorem sections_compose (scope : Scope) (first second : List (List Line)) :
    observe scope (first ++ second) = (do
      let left ← observe scope first
      let right ← observe scope second
      return left ++ right) := by
  unfold observe
  rw [List.mapM_append]
  cases hFirst : first.mapM (observeSection scope) <;> cases hSecond : second.mapM (observeSection scope) <;>
    simp [bind, pure, Except.bind, Except.pure]

end HoiminOracle.GitPatch
