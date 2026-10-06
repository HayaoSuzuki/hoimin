import Std

/- Model-only audit: AST classification is an input, not a verified parser. -/
namespace StatementDelete
inductive Effect | ordinary | named | await | yield | yieldFrom
  deriving DecidableEq, Repr

def eligible (callStatement : Bool) (effects : List Effect) : Bool :=
  callStatement && effects.all (· == .ordinary)

def replace (callStatement : Bool) (effects : List Effect) (original : String) : String :=
  if eligible callStatement effects then "pass" else original

theorem excludes_non_calls (effects : List Effect) (source : String) :
    replace false effects source = source := by simp [replace, eligible]

theorem accepted_effects_are_ordinary (effects : List Effect)
    (h : eligible true effects = true) : ∀ e ∈ effects, e = .ordinary := by
  simpa [eligible, List.all_eq_true] using h

theorem one_span_preserves_neighbors (before suffix original : String)
    (effects : List Effect) (h : eligible true effects = true) :
    before ++ replace true effects original ++ suffix = before ++ "pass" ++ suffix := by
  simp [replace, h]

-- Small boundary classes, not an unbounded parser equivalence claim.
example : eligible true [.ordinary, .ordinary] = true := by decide
example : eligible true [.ordinary, .named] = false := by decide
example : eligible true [.await] = false := by decide
example : eligible true [.yield] = false := by decide
example : eligible true [.yieldFrom] = false := by decide

-- Sensitivity: the broken outer-node-only gate accepts the smallest bad witness.
def brokenEligible (callStatement : Bool) (_effects : List Effect) := callStatement
example : brokenEligible true [.yield] ≠ eligible true [.yield] := by decide
end StatementDelete
