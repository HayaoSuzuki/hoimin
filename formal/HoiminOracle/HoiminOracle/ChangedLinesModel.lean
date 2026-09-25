import Std

namespace HoiminOracle.ChangedLines

inductive Eol where
  | lf | crlf | cr
  deriving BEq, Repr, DecidableEq

def Eol.text : Eol → String
  | .lf => "\n" | .crlf => "\r\n" | .cr => "\r"
def Eol.label : Eol → String
  | .lf => "LF" | .crlf => "CRLF" | .cr => "CR"

def step (s : Nat × Bool) (c : Char) : Nat × Bool :=
  if c == '\r' then (s.1 + 1, true)
  else if c == '\n' then (s.1 + if s.2 then 0 else 1, false)
  else (s.1, false)

-- Observations are code-token positions after separators, never the interior
-- of a CRLF pair. These are the original issue 612 model equations.
def pythonLine (before : String) : Nat :=
  (before.toList.foldl step (1, false)).1
def gitLine (before : String) : Nat :=
  1 + (before.toList.filter (· == '\n')).length
def naiveEither (before : String) : Nat :=
  1 + (before.toList.filter (fun c => c == '\n' || c == '\r')).length

def inside (first last line : Nat) : Bool := decide (first ≤ line ∧ line ≤ last)

set_option maxHeartbeats 10000 in
theorem crlfOne (n : Nat) (previous : Bool) :
    (step (step (n, previous) '\r') '\n').1 = n + 1 := by simp [step]

def sensitivity : List (String × Bool) := [
  ("lf-only-misses-cr", pythonLine "x\ry" == 2 && gitLine "x\ry" == 1),
  ("double-counts-crlf", pythonLine "x\r\ny" == 2 && naiveEither "x\r\ny" == 3),
  ("mixed-coordinate-selection", inside 1 1 (gitLine "x\ry") &&
      !(inside 1 1 (pythonLine "x\ry")))]

set_option maxHeartbeats 10000 in
example : sensitivity.all (·.2) = true := by decide

end HoiminOracle.ChangedLines
