import HoiminOracle.ResumeBudgetModel
open HoiminOracle.ResumeBudget

def main (args : List String) : IO UInt32 := do
 let args := if args.head? = some "--" then args.drop 1 else args
 match args with
 | ["--check", path] =>
   if (← IO.FS.readFile path) = corpus then pure 0
   else IO.eprintln "corpus is stale" *> pure 1
 | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
 | ["--check-recheck", path] =>
   if (← IO.FS.readFile path) = recheckCorpus then pure 0
   else IO.eprintln "budget recheck corpus is stale" *> pure 1
 | ["--output-recheck", path] => IO.FS.writeFile path recheckCorpus *> pure 0
 | ["--sensitivity"] =>
   IO.println s!"broken_budget_status_completion={sensitivity}"
   IO.println s!"broken_budget_recheck={recheckSensitivity}"
   pure <| if sensitivity && recheckSensitivity then 0 else 1
 | ["--stats"] => IO.println "cases=108\nbudgets=3\nstatuses=3\nboolean_dimensions=2" *> pure 0
 | [] => IO.print corpus *> pure 0
 | _ => IO.eprintln "usage: generate_resume_budget [(--check|--output|--check-recheck|--output-recheck) PATH|--sensitivity|--stats]" *> pure 2
