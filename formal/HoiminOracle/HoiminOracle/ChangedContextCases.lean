import HoiminOracle.ChangedContextProofs
import Lean

namespace HoiminOracle.ChangedContext

open Lean (toJson)

structure Fixture where
  name : String
  before : List Nat
  after : List Nat
  hunks : List Hunk := []
  explicit : List Nat := []
  untracked : Bool := false

-- Distinct identifiers prevent Git from ambiguously aligning repeated lines.
def fixtures : List Fixture :=
  [ ⟨"modified", [1,2,3,4,5], [1,2,30,4,5], [⟨3,1⟩], [], false⟩
  , ⟨"two-hunks", [1,2,3,4,5,6,7,8], [1,20,3,4,5,6,70,8], [⟨2,1⟩,⟨7,1⟩], [], false⟩
  , ⟨"deletion-middle", [1,2,3,4,5], [1,2,4,5], [⟨2,0⟩], [], false⟩
  , ⟨"deletion-bof", [1,2,3], [2,3], [⟨0,0⟩], [], false⟩
  , ⟨"deletion-eof", [1,2,3], [1,2], [⟨2,0⟩], [], false⟩
  , ⟨"deletion-all", [1,2], [], [⟨0,0⟩], [], false⟩
  , ⟨"insertion-bof", [1,2,3], [9,1,2,3], [⟨1,1⟩], [], false⟩
  , ⟨"insertion-eof", [1,2,3], [1,2,3,9], [⟨4,1⟩], [], false⟩
  , ⟨"explicit-intersection", [1,2,3,4,5], [1,2,30,4,5], [⟨3,1⟩], [1,2], false⟩
  , ⟨"unchanged", [1,2,3], [1,2,3], [], [], false⟩
  , ⟨"untracked", [], [1,2,3], [], [], true⟩ ]

def contexts : List Nat := [0,1,2,1073741823]

def source (lines : List Nat) : String :=
  String.join (lines.map fun n => s!"value_{n} = {n} + 1\n")

def jsonNats (xs : List Nat) : Lean.Json := toJson xs

def render (f : Fixture) (context : Nat) : Lean.Json := Lean.Json.mkObj
  [("schema", toJson (1 : Nat)), ("id", toJson s!"{f.name}-{context}"),
   ("before", toJson (source f.before)), ("after", toJson (source f.after)),
   ("context", toJson context), ("explicit", jsonNats f.explicit),
   ("untracked", toJson f.untracked),
   ("eligible_lines", jsonNats (selected f.hunks f.after.length context f.explicit f.untracked))]

def corpus : String := String.join <| fixtures.flatMap fun f =>
  contexts.map fun context => (render f context).compress ++ "\n"

-- Deliberately faulty alternatives must disagree on a witness.
def sensitivity : List (String × Bool) :=
  [ ("ignored-context", decide (selected [⟨3,1⟩] 5 1 ≠ selected [⟨3,1⟩] 5 0))
  , ("dropped-deletion", decide (selected [⟨2,0⟩] 4 1 ≠ selected [] 4 1))
  , ("gap-treated-as-line", decide (selected [⟨2,0⟩] 4 1 ≠ selected [⟨2,1⟩] 4 1))
  , ("explicit-union", decide (selected [⟨3,1⟩] 5 1 [1,2] ≠ [1,2,3,4]))
  , ("missing-file-bound", decide (selected [⟨1,1⟩] 2 2 ≠ [0,1,2,3])) ]

end HoiminOracle.ChangedContext
