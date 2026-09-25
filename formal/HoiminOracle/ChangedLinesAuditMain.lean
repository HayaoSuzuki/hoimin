import HoiminOracle.ChangedLinesModel
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
  | ["--sensitivity"] =>
      for (name, passed) in sensitivity do IO.println s!"{name}={passed}"
      pure <| if sensitivity.all (·.2) then 0 else 1
  | ["--stats"] =>
      IO.println s!"cases={rows.length}\nseparator_pairs=9\ngit_states=4\nmaximum_source_bytes=37\nsensitivity_families={sensitivity.length}\ntransitions=not-applicable"
      pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_lines [--check PATH|--output PATH|--sensitivity|--stats]" *> pure 2
