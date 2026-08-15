import HoiminOracle.ChangedTargetCases

open HoiminOracle.ChangedTarget

def printChecks (checks : List (String × Bool)) : IO UInt32 := do
  for (name, passed) in checks do
    IO.println s!"{name}={passed}"
  pure <| if checks.all (·.2) then 0 else 1

def jsonPath : Option String → String
  | none => "null"
  | some path => s!"\"{path}\""

def jsonNats (values : List Nat) : String :=
  "[" ++ String.intercalate "," (values.map toString) ++ "]"

def renderCase (case : AuditCase) : String :=
  "{\"schema\":1,\"id\":\"" ++ case.id ++ "\",\"mode\":\"" ++ case.mode ++
    "\",\"scenario\":\"" ++ case.scenario ++ "\",\"path\":" ++ jsonPath case.path ++
    ",\"eligible_lines\":" ++ jsonNats case.eligibleLines ++ "}"

-- The serialized oracle has a single source of truth: typed model inputs above,
-- evaluated through CombinedEligible for every bounded line.
def corpus : String := String.intercalate "\n" (auditCases.map renderCase) ++ "\n"

def main (args : List String) : IO UInt32 := do
  let args := if args.head? = some "--" then args.drop 1 else args
  match args with
  | ["--cases"] => printChecks fixedCases
  | ["--sensitivity"] => printChecks sensitivity
  | ["--stats"] =>
      IO.println s!"cases={auditCases.length}\nsensitivity_families={sensitivity.length}\nexplored_membership_states={exploredMembershipStates}\nmaximum_facts=2\nmaximum_ranges=2"
      pure 0
  | ["--check", path] =>
      let existing ← IO.FS.readFile path
      if existing = corpus then pure 0 else IO.eprintln "corpus is stale" *> pure 1
  | ["--output", path] => IO.FS.writeFile path corpus *> pure 0
  | [] => IO.print corpus *> pure 0
  | _ => IO.eprintln "usage: generate_changed_target [--cases|--sensitivity|--stats|--check PATH|--output PATH]" *> pure 2
