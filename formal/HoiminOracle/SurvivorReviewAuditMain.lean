import HoiminOracle.SurvivorReviewCases

open HoiminOracle.SurvivorReview

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path renderCorpus
    return 0
  | ["--check", path] =>
    return if (← IO.FS.readFile path) = renderCorpus then 0 else 1
  | ["--sensitivity"] =>
    IO.println s!"identity_and_revision_guards_detected={sensitivityPasses}"
    return if sensitivityPasses then 0 else 2
  | _ =>
    IO.eprintln "usage: generate_survivor_review --output PATH | --check PATH | --sensitivity"
    return 2
