import HoiminOracle.EmptyDirectoryModel
open HoiminOracle.EmptyDirectory

def main (args : List String) : IO UInt32 := do
 let args := if args.head? = some "--" then args.drop 1 else args
 match args with
 | ["--check", path] =>
   if (← IO.FS.readFile path) = corpus then pure 0
   else IO.eprintln "corpus is stale" *> pure 1
 | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
 | ["--sensitivity"] =>
   IO.println s!"lost_empty_ignored_exclusion_retained_extra={sensitivity}"
   pure <| if sensitivity then 0 else 1
 | ["--stats"] => IO.println "cases=24\nentry_states=3\nboolean_dimensions=3" *> pure 0
 | [] => IO.print corpus *> pure 0
 | _ => IO.eprintln "usage: generate_empty_directory [--check PATH|--output PATH|--sensitivity|--stats]" *> pure 2
