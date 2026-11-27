import HoiminOracle.GitWorktreeModel

namespace HoiminOracle.GitTrackedRows

open HoiminOracle.GitWorktree (Files Scope currentText)
open HoiminOracle.GitPhysicalRows (Span translate)

abbrev Rows := List (String × List Span)

inductive Failure
  | source (reason : GitWorktree.Diagnostic)
  | range (reason : GitPhysicalRows.Diagnostic)

def selected (scope : Scope) (rows : Rows) : Rows :=
  rows.filter fun (path, _) => scope.all fun targets => targets.any (·.path == path)

def projectFile (files : Files) (project : List UInt8 → List Span → Except GitPhysicalRows.Diagnostic (List Span))
    (entry : String × List Span) : Except Failure (Option (String × List Span)) := do
  match (← (currentText files entry.1).mapError .source) with
  | none => return none
  | some bytes =>
    let spans ← (project bytes entry.2).mapError .range
    return some (entry.1, spans)

def collect (read : String × List Span → Except Failure (Option (String × List Span)))
    (rows : Rows) : Except Failure Rows :=
  (rows.mapM read).map (·.filterMap id)

def scanWith (read : String × List Span → Except Failure (Option (String × List Span)))
    (rootAvailable : Bool) (scope : Scope) (rows : Rows) : Except Failure Rows :=
  match (selected scope rows).mergeSort (fun left right => left.1 ≤ right.1) with
  | [] => .ok []
  | kept => if rootAvailable then collect read kept else .error (.source .rootUnavailable)

def scan (rootAvailable : Bool) (scope : Scope) (rows : Rows) (files : Files) : Except Failure Rows :=
  scanWith (projectFile files translate) rootAvailable scope rows

theorem selected_subset (scope : Scope) (rows : Rows) (entry : String × List Span)
    (kept : entry ∈ selected scope rows) : entry ∈ rows := (List.mem_filter.mp kept).1

theorem empty_scope_skips_root (rootAvailable : Bool) (rows : Rows) (files : Files) :
    scan rootAvailable (some []) rows files = .ok [] := by
  have empty : selected (some []) rows = [] := by simp [selected, List.filter_eq_nil_iff]
  simp [scan, scanWith, empty]

theorem skipped_file_skips_projection (files : Files)
    (project : List UInt8 → List Span → Except GitPhysicalRows.Diagnostic (List Span))
    (path : String) (spans : List Span) (skipped : currentText files path = .ok none) :
    projectFile files project (path, spans) = .ok none := by
  simp [projectFile, skipped, Except.mapError, bind, Except.bind, pure, Except.pure]

theorem late_failure_returns_only_error
    (read : String × List Span → Except Failure (Option (String × List Span)))
    (before after : Rows) (entry : String × List Span) (outputs : List (Option (String × List Span)))
    (error : Failure) (successful : before.mapM read = .ok outputs) (failed : read entry = .error error) :
    collect read (before ++ entry :: after) = .error error := by
  simp [collect, List.mapM_append, successful, failed, bind, Except.bind, Except.map]

end HoiminOracle.GitTrackedRows
