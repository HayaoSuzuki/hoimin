import HoiminOracle.BudgetCases

open HoiminOracle.BudgetAudit

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  if let some parent := path.parent then
    IO.FS.createDirAll parent
  IO.FS.writeFile path renderCorpus
  return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  try
    let actual ← IO.FS.readFile path
    if actual == renderCorpus then
      return 0
    IO.eprintln s!"stale Lean budget oracle corpus: {path}"
    return 1
  catch error =>
    IO.eprintln s!"cannot check Lean budget oracle corpus {path}: {error}"
    return 1

@[noinline] private def printStats (depth : Nat) : IO UInt32 := do
  unless boundedAuditPasses depth do
    IO.eprintln "bounded budget audit found an unsafe reachable state"
    return 2
  IO.println s!"depth={depth} alphabet={alphabetSize} states={reachableStateCount depth} transitions={checkedTransitionCount depth}"
  return 0

def main (args : List String) : IO UInt32 := do
  unless brokenWitnessesDetected do
    IO.eprintln "broken budget-model witnesses were not detected"
    return 2
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats auditDepth
  | _ =>
      IO.eprintln "usage: generate_budget (--output PATH | --check PATH | --stats)"
      return 2
