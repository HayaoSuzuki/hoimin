import HoiminOracle.ChangedTargetCases

open HoiminOracle.ChangedTarget

def printChecks (checks : List (String × Bool)) : IO UInt32 := do
  for (name, passed) in checks do
    IO.println s!"{name}={passed}"
  pure <| if checks.all (·.2) then 0 else 1

def corpus : String := String.intercalate "\n"
  [ "{\"schema\":1,\"id\":\"modified-overlap\",\"mode\":\"strict\",\"scenario\":\"modified_overlap\",\"path\":\"pkg/a.py\",\"eligible_lines\":[2,3,4,5,6,7]}"
  , "{\"schema\":1,\"id\":\"explicit-line\",\"mode\":\"strict\",\"scenario\":\"explicit_intersection\",\"path\":\"pkg/a.py\",\"eligible_lines\":[4,5]}"
  , "{\"schema\":1,\"id\":\"symbol-line\",\"mode\":\"strict\",\"scenario\":\"symbol_intersection\",\"path\":\"pkg/a.py\",\"eligible_lines\":[5]}"
  , "{\"schema\":1,\"id\":\"rename-destination\",\"mode\":\"strict\",\"scenario\":\"rename\",\"path\":\"pkg/new.py\",\"eligible_lines\":[2]}"
  , "{\"schema\":1,\"id\":\"deleted-binary\",\"mode\":\"strict\",\"scenario\":\"excluded\",\"path\":null,\"eligible_lines\":[]}"
  , "{\"schema\":1,\"id\":\"untracked-unterminated\",\"mode\":\"strict\",\"scenario\":\"untracked\",\"path\":\"pkg/new.py\",\"eligible_lines\":[1,2]}"
  , "{\"schema\":1,\"id\":\"diff-base-worktree\",\"mode\":\"strict\",\"scenario\":\"diff_base\",\"path\":\"pkg/a.py\",\"eligible_lines\":[2,3]}"
  , "{\"schema\":1,\"id\":\"hostile-parser\",\"mode\":\"internal-fixture\",\"scenario\":\"parser_isolation\",\"path\":\"pkg/good.py\",\"eligible_lines\":[3]}"
  , "{\"schema\":1,\"id\":\"non-utf8-path\",\"mode\":\"model-only\",\"scenario\":\"non_utf8_path\",\"path\":null,\"eligible_lines\":[]}"
  , "{\"schema\":1,\"id\":\"git-failure\",\"mode\":\"infrastructure-error\",\"scenario\":\"git_failure\",\"path\":null,\"eligible_lines\":[]}"
  ] ++ "\n"

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--cases"] => printChecks fixedCases
  | ["--sensitivity"] => printChecks sensitivity
  | ["--stats"] =>
      IO.println "cases=10\nsensitivity_families=10\nmaximum_facts=2\nmaximum_ranges=3"
      pure 0
  | ["--check", path] =>
      let existing ← IO.FS.readFile path
      if existing = corpus then pure 0 else IO.eprintln "corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_target [--cases|--sensitivity|--stats|--check PATH|--output PATH]" *> pure 2
