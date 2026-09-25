import HoiminOracle.ResumeDiagnosticModel
open HoiminOracle.ResumeDiagnostic

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--check", path] =>
    if (← IO.FS.readFile path) = corpus then pure 0
    else IO.eprintln "resume diagnostic corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | ["--sensitivity"] =>
    IO.println s!"broken_selection_priority_race={sensitivity}"
    pure <| if sensitivity then 0 else 1
  | ["--stats"] => IO.println "cases=81\nstrict=64\nmodel_only=17\nhistory_dimensions=5\norders=2\nmaximum_rows=6" *> pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_resume_diagnostic [--check PATH|--output PATH|--sensitivity|--stats]" *> pure 2
