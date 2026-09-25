import HoiminOracle.GlobSelectionModel
open HoiminOracle.GlobSelection

def main (args : List String) : IO UInt32 := do
 let args := if args.head? = some "--" then args.drop 1 else args
 match args with
 | ["--check", path] =>
   if (← IO.FS.readFile path) = corpus then pure 0
   else IO.eprintln "corpus is stale" *> pure 1
 | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
 | ["--sensitivity"] =>
   IO.println s!"raw_literal_and_escape={sensitivity}"
   pure <| if sensitivity then 0 else 1
 | ["--stats"] => IO.println "cases=20\npatterns=4\nfiles=5\nselectors=5" *> pure 0
 | [] => IO.print corpus *> pure 0
 | _ => IO.eprintln "usage: generate_glob_selection [--check PATH|--output PATH|--sensitivity|--stats]" *> pure 2
