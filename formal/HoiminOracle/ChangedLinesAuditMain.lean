import HoiminOracle.ChangedLinesModel
import HoiminOracle.GitPhysicalRowsCases
import HoiminOracle.GitWorktreeCases
import HoiminOracle.GitResolutionCases
import HoiminOracle.GitCaptureCases
import HoiminOracle.GitLifecycleCases
import HoiminOracle.GitAdmissionCases
import Lean.Data.Json

open HoiminOracle.ChangedLines Lean

namespace ChangedLinesAudit

def states : List String := ["untracked", "unborn-indexed", "staged-new", "tracked-modified"]
def eols : List Eol := [.lf, .crlf, .cr]

def row (a b : Eol) (state : String) : Json := Id.run do
  let before := "# header" ++ a.text ++ "def f():" ++ b.text ++ "    return 1 "
  let source := before ++ "+ 2\n"
  let pyLine := pythonLine before
  let gitRow := gitLine before
  let first := if state == "tracked-modified" then gitRow else 1
  return Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson s!"{a.label}-{b.label}-{state}"),
    ("mode", toJson "strict"), ("eol1", toJson a.label), ("eol2", toJson b.label),
    ("git_state", toJson state), ("source", toJson source),
    ("original_source", toJson (before ++ "- 2\n")),
    ("candidate_line", toJson pyLine), ("candidate_offset", toJson before.utf8ByteSize),
    ("changed_first_git_line", toJson first), ("changed_last_git_line", toJson gitRow),
    ("eligible", toJson (inside first gitRow gitRow)),
    ("broken_eligible", toJson (inside first gitRow pyLine))]

def rows : List Json := eols.flatMap fun a => eols.flatMap fun b => states.map (row a b)
def corpus : String := String.join (rows.map fun value => value.compress ++ "\n")

end ChangedLinesAudit

open ChangedLinesAudit

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--check", path] =>
      if (← IO.FS.readFile path) = corpus then pure 0
      else IO.eprintln "corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | ["--check-physical-rows", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitPhysicalRows.corpus then pure 0
      else IO.eprintln "Git physical rows corpus is stale" *> pure 1
  | ["--output-physical-rows", path] =>
      IO.FS.writeFile path HoiminOracle.GitPhysicalRows.corpus *> pure 0
  | ["--check-git-worktree", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitWorktree.corpus then pure 0
      else IO.eprintln "Git worktree corpus is stale" *> pure 1
  | ["--output-git-worktree", path] =>
      IO.FS.writeFile path HoiminOracle.GitWorktree.corpus *> pure 0
  | ["--check-git-resolution", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitResolution.corpus then pure 0
      else IO.eprintln "Git resolution corpus is stale" *> pure 1
  | ["--output-git-resolution", path] =>
      IO.FS.writeFile path HoiminOracle.GitResolution.corpus *> pure 0
  | ["--check-git-capture", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitCapture.corpus then pure 0
      else IO.eprintln "Git capture corpus is stale" *> pure 1
  | ["--output-git-capture", path] =>
      IO.FS.writeFile path HoiminOracle.GitCapture.corpus *> pure 0
  | ["--check-git-lifecycle", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitLifecycle.corpus then pure 0
      else IO.eprintln "Git lifecycle corpus is stale" *> pure 1
  | ["--output-git-lifecycle", path] =>
      IO.FS.writeFile path HoiminOracle.GitLifecycle.corpus *> pure 0
  | ["--check-git-admission", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.GitAdmission.corpus then pure 0
      else IO.eprintln "Git admission corpus is stale" *> pure 1
  | ["--output-git-admission", path] =>
      IO.FS.writeFile path HoiminOracle.GitAdmission.corpus *> pure 0
  | ["--sensitivity"] =>
      for (name, passed) in sensitivity do IO.println s!"{name}={passed}"
      unless sensitivity.all (·.2) do return 1
      let status ← HoiminOracle.GitPhysicalRows.sensitivity
      if status != 0 then return status
      let status ← HoiminOracle.GitWorktree.sensitivity
      if status != 0 then return status
      let status ← HoiminOracle.GitResolution.sensitivity
      if status != 0 then return status
      let status ← HoiminOracle.GitCapture.sensitivity
      if status != 0 then return status
      let status ← HoiminOracle.GitLifecycle.sensitivity
      if status != 0 then return status
      HoiminOracle.GitAdmission.sensitivity
  | ["--stats"] =>
      IO.println s!"cases={rows.length}\nseparator_pairs=9\ngit_states=4\nmaximum_source_bytes=37\nsensitivity_families={sensitivity.length}\ntransitions=not-applicable"
      pure 0
  | ["--physical-rows-stats"] =>
      HoiminOracle.GitPhysicalRows.stats
      pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_lines [--check PATH|--output PATH|--check-physical-rows PATH|--output-physical-rows PATH|--check-git-worktree PATH|--output-git-worktree PATH|--check-git-resolution PATH|--output-git-resolution PATH|--check-git-capture PATH|--output-git-capture PATH|--check-git-lifecycle PATH|--output-git-lifecycle PATH|--check-git-admission PATH|--output-git-admission PATH|--sensitivity|--stats|--physical-rows-stats]" *> pure 2
