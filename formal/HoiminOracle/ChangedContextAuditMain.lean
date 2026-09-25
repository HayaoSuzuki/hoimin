import HoiminOracle.ChangedContextCases

open HoiminOracle.ChangedContext

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
      IO.println s!"cases={fixtures.length * contexts.length}\nmaximum_file_lines=8\nmaximum_hunks=2\nsensitivity_families={sensitivity.length}"
      pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_context [--check PATH|--output PATH|--sensitivity|--stats]" *> pure 2
