import HoiminOracle.WorkerGrantPublicationCases

open HoiminOracle.WorkerGrantPublication

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) = corpus then return 0
    IO.eprintln s!"stale worker grant publication corpus: {path}"
    return 1
  | ["--sensitivity"] => sensitivity
  | _ => IO.eprintln "usage: generate_worker_grant_publication --output PATH | --check PATH | --sensitivity" *> pure 2
