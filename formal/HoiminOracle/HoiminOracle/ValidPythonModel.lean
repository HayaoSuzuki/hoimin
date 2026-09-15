import HoiminOracle.AnnotationScopeModel
import HoiminOracle.ComprehensionBindingModel
import HoiminOracle.ExceptionMatchBindingModel
import HoiminOracle.CandidateSpanModel

namespace HoiminOracle.ValidPython
open BindingFlow

-- Trusted source/destination identities are premises supplied by each consumer's
-- worksheet, not inferred from Python spelling by this reduced model.
def pairAllowed (sourceVisible destinationVisible : Bool) : Bool :=
  sourceVisible && destinationVisible

theorem blocked_source (destinationVisible : Bool) :
    pairAllowed false destinationVisible = false := by simp [pairAllowed]
theorem blocked_destination (sourceVisible : Bool) :
    pairAllowed sourceVisible false = false := by simp [pairAllowed]

def scopeAllowed (path : List Frame) : Bool := ComprehensionBinding.allows path

def genericAllowed (name : Name) (visible : Bool) : Bool :=
  let whole := if visible then emptyEnv.set name .shadowed else emptyEnv
  scopeAllowed [functionFrame 1 whole, moduleFrame emptyEnv emptyEnv]

theorem generic_source_hidden : genericAllowed .source true = false := by decide
theorem generic_destination_hidden : genericAllowed .destination true = false := by decide

def walrusAllowed (name : Name) : Bool :=
  scopeAllowed ((ComprehensionBinding.namedWrite name
    [ComprehensionBinding.comp, ComprehensionBinding.module]).drop 1)

def iterationAllowed : Bool :=
  (AnnotationScope.observeComprehension emptyEnv .source).after == emptyEnv

def handlerAllowed : Bool :=
  let environment := ExceptionMatchBinding.bindTarget emptyEnv .source
  AnnotationScope.resolutionOfFact environment.source == .definitelyBuiltin

-- A finite exact subset: booleans/integers and integral complex components.
-- Floating-point rounding, NaN and arbitrary key evaluation are excluded.
structure Key where
  spelling : String
  real : Int
  imaginary : Int := 0
  deriving Repr

def sameValue (a b : Key) : Bool := a.real == b.real && a.imaginary == b.imaginary
def uniqueReplacement (replacement : Key) (siblings : List Key) : Bool :=
  !(siblings.any (sameValue replacement))

theorem equal_sibling_rejected (key other : Key) (h : sameValue key other = true) :
    uniqueReplacement key [other] = false := by simp [uniqueReplacement, h]

-- Direct tuple elements only: nested expressions and starred expressions are
-- ordinary here. Parser correctness and all other eligibility guards are premises.
inductive TupleElement where
  | expression
  | slice
  deriving BEq, DecidableEq, Repr

def tupleAllowed (elements : List TupleElement) : Bool :=
  elements.all (· == .expression)

theorem allowed_elements (elements : List TupleElement) (h : tupleAllowed elements = true) :
    ∀ element ∈ elements, element = .expression := by
  intro element member
  have result := (List.all_eq_true.mp h) element member
  cases element with
  | expression => rfl
  | slice => contradiction

def brokenTupleAllowed (_elements : List TupleElement) : Bool := true

def tupleInputs : Nat → List (List TupleElement)
  | 0 => [[]]
  | n + 1 => (tupleInputs n).flatMap fun tail =>
      [TupleElement.expression, .slice].map (· :: tail)

def tupleDomain : List (List TupleElement) :=
  [1, 2, 3].flatMap tupleInputs

def tupleWitnesses := tupleDomain.filter fun elements =>
  tupleAllowed elements != brokenTupleAllowed elements

set_option maxHeartbeats 100000 in
 theorem tuple_finite_sensitivity : tupleDomain.length = 14 ∧
    tupleWitnesses.length = 11 ∧ tupleWitnesses.head? = some [.slice] := by decide

theorem direct_slice_blocked : tupleAllowed [.slice] = false := by decide

def replace (source replacement : List Nat) (start length : Nat) : List Nat :=
  CandidateSpan.replaceBytes source {
    path := "subject.py", start, length, original := (source.drop start).take length,
    replacement, operator := "fixture", line := 1, column := 0, sourceHash := "",
    identity := {
      schema := 1, sourceHash := "", path := "subject.py", start, length,
      operator := "fixture", replacement }
  }

theorem one_span (source replacement : List Nat) (start length : Nat) :
    replace source replacement start length =
      source.take start ++ replacement ++ source.drop (start + length) := by rfl

-- Deliberately wrong rules. Finite searches are over two names, two visibility
-- states and three exact key aliases; no unbounded Python-language exploration.
def brokenWalrus (name : Name) : Bool :=
  scopeAllowed ((ComprehensionBinding.brokenCurrentWrite name
    [ComprehensionBinding.comp, ComprehensionBinding.module]).drop 1)
def brokenGeneric (_name : Name) (_visible : Bool) : Bool := true
def brokenLexical (replacement : Key) (siblings : List Key) : Bool :=
  !(siblings.any (fun key => replacement.spelling == key.spelling))
def keyWitnesses : List (Key × Key) := [
  (⟨"False", 0, 0⟩, ⟨"0", 0, 0⟩),
  (⟨"True", 1, 0⟩, ⟨"1", 1, 0⟩),
  (⟨"1-2j", 1, -2⟩, ⟨"1.0-2.0j", 1, -2⟩)]
def sensitivity : Bool :=
  [.source, .destination].all (fun name => walrusAllowed name != brokenWalrus name) &&
  [.source, .destination].all (fun name => genericAllowed name true != brokenGeneric name true) &&
  keyWitnesses.all (fun (a,b) => uniqueReplacement a [b] != brokenLexical a [b]) &&
  replace [65,66,67] [88] 1 1 != [88]

theorem finite_broken_variants_detected : sensitivity = true := by decide
end HoiminOracle.ValidPython
