import HoiminOracle.ChangedContextCases
import HoiminOracle.ChangedConfigCases
import HoiminOracle.SelectorSyntaxCases
import HoiminOracle.LineCoverageCases

open HoiminOracle.ChangedContext

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--check", path] =>
      if (← IO.FS.readFile path) = corpus then pure 0
      else IO.eprintln "corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | ["--check-config", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.ChangedConfig.corpus then pure 0
      else IO.eprintln "changed config corpus is stale" *> pure 1
  | ["--output-config", path] => IO.FS.writeFile path HoiminOracle.ChangedConfig.corpus *> pure 0
  | ["--check-selector-syntax", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.SelectorSyntax.corpus then pure 0
      else IO.eprintln "selector syntax corpus is stale" *> pure 1
  | ["--output-selector-syntax", path] =>
      IO.FS.writeFile path HoiminOracle.SelectorSyntax.corpus *> pure 0
  | ["--check-line-coverage", path] =>
      if (← IO.FS.readFile path) = HoiminOracle.LineCoverage.corpus then pure 0
      else IO.eprintln "line coverage corpus is stale" *> pure 1
  | ["--output-line-coverage", path] =>
      IO.FS.writeFile path HoiminOracle.LineCoverage.corpus *> pure 0
  | ["--sensitivity"] =>
      for (name, passed) in sensitivity do IO.println s!"{name}={passed}"
      unless sensitivity.all (·.2) do return 1
      let status ← HoiminOracle.ChangedConfig.sensitivity
      if status != 0 then return status
      let status ← HoiminOracle.SelectorSyntax.sensitivity
      if status != 0 then return status
      HoiminOracle.LineCoverage.sensitivity
  | ["--stats"] =>
      IO.println s!"cases={fixtures.length * contexts.length}\nmaximum_file_lines=8\nmaximum_hunks=2\nsensitivity_families={sensitivity.length}"
      pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_context [--check PATH|--output PATH|--check-config PATH|--output-config PATH|--check-selector-syntax PATH|--output-selector-syntax PATH|--check-line-coverage PATH|--output-line-coverage PATH|--sensitivity|--stats]" *> pure 2
