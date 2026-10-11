import HoiminOracle.SurvivorReviewProofs
import Lean.Data.Json

namespace HoiminOracle.SurvivorReview

def differences : List String :=
  ["same", "project", "id", "schema", "path", "span", "length", "original",
   "replacement", "operator", "line", "column", "symbol", "hash"]

def baseKey : Key := ⟨0, 0, List.replicate 11 0⟩

def changedKey (difference : String) : Key :=
  if difference = "project" then { baseKey with project := 1 }
  else if difference = "id" then { baseKey with id := 1 }
  else if difference = "same" then baseKey
  else { baseKey with descriptor := baseKey.descriptor.set (differences.idxOf difference - 3) 1 }

def caseJson (difference : String) (source current expected : Nat) : Lean.Json := Id.run do
  let state : State := ⟨current, if current = 0 then [] else [current]⟩
  let applicable := applicability baseKey (changedKey difference) source
  let (result, next) := applyUpdate state applicable expected
  return Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("difference", Lean.toJson difference),
    ("source", Lean.toJson source), ("current", Lean.toJson current),
    ("expected", Lean.toJson expected), ("applicability", Lean.toJson applicable),
    ("result", Lean.toJson result), ("next_revision", Lean.toJson next.revision),
    ("history", Lean.toJson next.history)]

def renderCorpus : String := String.join <| differences.flatMap fun difference =>
  [0, 1, 2].flatMap fun source =>
    [(0, 0), (1, 1), (1, 0), (maxRevision, maxRevision)].map fun (current, expected) =>
      (caseJson difference source current expected).compress ++ "\n"

-- Mutants omit one of the actual model guards, not a hand-written expected result.
def sensitivityPasses : Bool :=
  let identityDetected := (differences.filter (· != "same")).all fun difference =>
    applicability baseKey (changedKey difference) 0 != applicability baseKey baseKey 0
  let revisionDetected := (applyUpdate ⟨1, [1]⟩ "current" 0).1 !=
    (applyUpdate ⟨1, [1]⟩ "current" 1).1
  identityDetected && revisionDetected

end HoiminOracle.SurvivorReview
