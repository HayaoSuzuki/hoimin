import Lean
open Lean
namespace HoiminOracle.GlobSelection

inductive FileName where
  | a | b | c | pos | neg
  deriving BEq, DecidableEq
inductive Pattern where
  | pos | neg | escaped | literal
  deriving BEq, DecidableEq

def FileName.path : FileName → String
  | .a => "src/a.py" | .b => "src/b.py" | .c => "src/c.py"
  | .pos => "src/[ab].py" | .neg => "src/[!a].py"
def Pattern.text : Pattern → String
  | .pos => "src/[ab].py" | .neg => "src/[!a].py"
  | .escaped => "src/\\[ab\\].py" | .literal => "src/a.py"
def globMatches : Pattern → FileName → Bool
  | .pos, .a | .pos, .b => true
  | .neg, .b | .neg, .c => true
  | .escaped, .pos | .literal, .a => true
  | _, _ => false
def keep (p : Pattern) (f : FileName) : Bool := !globMatches p f

def rawSame : Pattern → FileName → Bool
  | .pos, .pos | .neg, .neg | .literal, .a => true
  | _, _ => false
def brokenKeep (p : Pattern) (f : FileName) : Bool := keep p f && !rawSame p f

set_option maxHeartbeats 10000 in
 theorem semanticFilterIdempotent (p : Pattern) (f : FileName) :
     (keep p f && keep p f) = keep p f := by cases h : keep p f <;> rfl

set_option maxHeartbeats 10000 in
 example : keep .pos .pos = true ∧ brokenKeep .pos .pos = false := by decide
set_option maxHeartbeats 10000 in
 example : keep .escaped .pos = false ∧ keep .pos .pos = true := by decide

-- Deliberate error: interpret the escaped literal as a character class.
def brokenEscaped (f : FileName) : Bool := keep .pos f
set_option maxHeartbeats 10000 in
 example : brokenEscaped .pos = true ∧ keep .escaped .pos = false := by decide


def files : List FileName := [.a, .b, .c, .pos, .neg]

def render (p : Pattern) (selector : String) (chosen : FileName) : Json :=
 let selectedFiles := if selector == "source" then files else [chosen]
 let retained := selectedFiles.filter (keep p)
 Json.mkObj [
  ("schema", toJson (1 : Nat)), ("mode", toJson "strict"),
  ("files", toJson (files.map FileName.path)), ("pattern", toJson p.text),
  ("selector", toJson selector), ("selected_path", toJson chosen.path),
  ("expected_paths", toJson (retained.map FileName.path)),
  ("expected_error", toJson (selector != "source" && retained.isEmpty)),
  ("broken_paths", toJson ((selectedFiles.filter (brokenKeep p)).map FileName.path))]

def corpus : String := String.join <|
 [Pattern.pos, .neg, .escaped, .literal].flatMap fun p =>
  [("source", FileName.pos), ("file", .pos), ("file", .neg), ("line", .pos), ("line", .neg)].map
   fun (selector, chosen) => (render p selector chosen).compress ++ "\n"

def sensitivity : Bool := keep .pos .pos != brokenKeep .pos .pos &&
 keep .neg .neg != brokenKeep .neg .neg && brokenEscaped .pos != keep .escaped .pos

end HoiminOracle.GlobSelection
