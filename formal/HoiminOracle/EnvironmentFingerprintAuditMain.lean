import HoiminOracle.EnvironmentFingerprintModel
import HoiminOracle.ImportRootCases
import HoiminOracle.OperatorSelectorCases
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
  | ["--check-import-roots", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.ImportRoot.corpus then pure 0
      else IO.eprintln "import-root corpus is stale" *> pure 1
  | ["--output-import-roots", path] => IO.FS.writeFile path HoiminOracle.ImportRoot.corpus *> pure 0
  | ["--check-operator-selectors", path] =>
      if (← IO.FS.readFile path) = (← HoiminOracle.OperatorSelector.corpus) then pure 0
      else IO.eprintln "operator selector corpus is stale" *> pure 1
  | ["--output-operator-selectors", path] =>
      IO.FS.writeFile path (← HoiminOracle.OperatorSelector.corpus) *> pure 0
  | ["--sensitivity"] =>
      IO.println s!"presence_tracking_reuse_detected={sensitivity}"
      let status ← HoiminOracle.ImportRoot.sensitivity
      if status != 0 then return status
      HoiminOracle.OperatorSelector.sensitivity
  | ["--stats"] =>
      IO.println s!"schema=1\nvalues={values.length}\ntracking_modes=2\ninvocations_per_case=2\ncases={cases.length}"
      pure 0
  | _ =>
      IO.eprintln "usage: generate_environment_fingerprint --check PATH|--output PATH|--check-import-roots PATH|--output-import-roots PATH|--check-operator-selectors PATH|--output-operator-selectors PATH|--sensitivity|--stats"
      pure 2
