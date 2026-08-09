import HoiminOracle.WorkspaceCases

open HoiminOracle.WorkspaceAudit

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
    IO.eprintln s!"stale Lean workspace lifecycle corpus: {path}"
    return 1
  catch error =>
    IO.eprintln s!"cannot check Lean workspace lifecycle corpus {path}: {error}"
    return 1

private def printStats : IO UInt32 := do
  unless boundedAuditPasses do
    IO.eprintln "bounded workspace lifecycle audit found an unsafe reachable state"
    return 2
  IO.println s!"depth={auditDepth} alphabet={alphabetSize} states={reachableStateCount} transitions={checkedTransitionCount}"
  return 0

def main (args : List String) : IO UInt32 := do
  unless brokenWitnessesDetected do
    IO.eprintln "broken workspace lifecycle witnesses were not detected"
    return 2
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats
  | _ =>
      IO.eprintln "usage: generate_workspace (--output PATH | --check PATH | --stats)"
      return 2
