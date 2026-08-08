import HoiminOracle.Cases

open HoiminOracle

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
    IO.eprintln s!"stale Lean oracle corpus: {path}"
    return 1
  catch error =>
    IO.eprintln s!"cannot check Lean oracle corpus {path}: {error}"
    return 1

def main (args : List String) : IO UInt32 := do
  unless brokenWitnessesDetected do
    IO.eprintln "broken-model witnesses were not detected"
    return 2
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | _ =>
      IO.eprintln "usage: generate (--output PATH | --check PATH)"
      return 2
