import HoiminOracle.EnvironmentFingerprintModel
open HoiminOracle.EnvironmentFingerprint

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  unless sensitivity do
    IO.eprintln "environment fingerprint sensitivity failed"
    return 2
  match args with
  | ["--check", path] =>
      if (← IO.FS.readFile path) = corpus then pure 0
      else IO.eprintln "environment fingerprint corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | ["--sensitivity"] =>
      IO.println s!"presence_tracking_reuse_detected={sensitivity}"
      pure 0
  | ["--stats"] =>
      IO.println s!"schema=1\nvalues={values.length}\ntracking_modes=2\ninvocations_per_case=2\ncases={cases.length}"
      pure 0
  | _ =>
      IO.eprintln "usage: generate_environment_fingerprint --check PATH|--output PATH|--sensitivity|--stats"
      pure 2
