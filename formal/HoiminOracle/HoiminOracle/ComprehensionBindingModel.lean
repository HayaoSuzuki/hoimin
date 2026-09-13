import HoiminOracle.BindingFlowModel

namespace HoiminOracle.ComprehensionBinding
open BindingFlow

-- Paths are innermost-first, as in BindingFlow.resolveFrom. Python source
-- validity (notably class-body walrus restrictions) is an adapter premise.
def containing : List Frame → List Frame
  | [] => []
  | frame :: rest =>
      if frame.kind = .comprehension then containing rest else frame :: rest

def nonlocalOwner (name : Name) : List Frame → Option Nat
  | [] => none
  | frame :: rest =>
      if frame.kind == .function && frame.whole.get name != .absent then
        some frame.id
      else nonlocalOwner name rest

def destination (name : Name) (path : List Frame) : Option Nat :=
  match containing path with
  | [] => none
  | frame :: rest =>
      match frame.directive with
      | .normal => some frame.id
      | .global => (frame :: rest).find? (fun f => f.kind == .module) |>.map Frame.id
      | .nonlocal => nonlocalOwner name rest

-- A routed comprehension write is possible, never assumed executed. A
-- function declaration is static even when its comprehension does not run.
def writeFrame (name : Name) (frame : Frame) : Frame :=
  if frame.kind == .function || frame.kind == .comprehension then
    { frame with whole := frame.whole.set name .shadowed }
  else
    { frame with
      before := frame.before.set name ((frame.before.get name).meet .shadowed)
      whole := frame.whole.set name .unknown }

def writeAt (name : Name) (owner : Option Nat) (path : List Frame) : List Frame :=
  path.map fun frame => if owner == some frame.id then writeFrame name frame else frame

def namedWrite (name : Name) (path : List Frame) : List Frame :=
  writeAt name (destination name path) path

def brokenCurrentWrite (name : Name) (path : List Frame) : List Frame :=
  writeAt name (path.head?.map Frame.id) path

theorem skips_comprehension (name : Name) (frame : Frame) (rest : List Frame)
    (h : frame.kind = .comprehension) :
    destination name (frame :: rest) = destination name rest := by
  simp [destination, containing, h]

theorem stops_at_normal_boundary (name : Name) (frame : Frame) (rest : List Frame)
    (h : frame.kind ≠ .comprehension) (d : frame.directive = .normal) :
    destination name (frame :: rest) = some frame.id := by
  simp [destination, containing, h, d]

theorem containing_global (name : Name) (frame : Frame) (rest : List Frame)
    (h : frame.kind ≠ .comprehension) (d : frame.directive = .global) :
    destination name (frame :: rest) =
      ((frame :: rest).find? (fun f => f.kind == .module)).map Frame.id := by
  simp [destination, containing, h, d]

theorem containing_nonlocal (name : Name) (frame : Frame) (rest : List Frame)
    (h : frame.kind ≠ .comprehension) (d : frame.directive = .nonlocal) :
    destination name (frame :: rest) = nonlocalOwner name rest := by
  simp [destination, containing, h, d]

def module : Frame := moduleFrame emptyEnv emptyEnv
def comp : Frame := { id := 1, kind := .comprehension }

def allows (path : List Frame) : Bool :=
  let environment := resolveCandidate { path }
  environment.source == .known .builtin && environment.destination == .known .builtin

-- Fixed regression witness: old current-scope writes leave the containing
-- module's later call eligible for an invalid builtin replacement.
theorem old_current_scope_detected :
    allows ((brokenCurrentWrite .source [comp, module]).drop 1) = true ∧
    allows ((namedWrite .source [comp, module]).drop 1) = false := by decide

-- Declarations precede evaluation; ordered effects follow expression completion.
structure RoutedWrite where
  name : Name
  owner : Option Nat

inductive Summary where
  | observe (dropFrames : Nat)
  | sequence (left right : Summary)
  | comprehension (firstIterable : Summary) (writes : List RoutedWrite)
  | empty

def declareAt (write : RoutedWrite) (path : List Frame) : List Frame :=
  path.map fun frame =>
    if write.owner == some frame.id then
      let fact := if frame.kind == .function || frame.kind == .comprehension then Fact.shadowed else .unknown
      { frame with whole := frame.whole.set write.name fact }
    else frame

def declarations : Summary → List RoutedWrite
  | .observe _ | .empty => []
  | .sequence left right => declarations left ++ declarations right
  | .comprehension first writes => declarations first ++ writes

def publish (writes : List RoutedWrite) (path : List Frame) : List Frame :=
  writes.foldl (fun frames write => writeAt write.name write.owner frames) path

def interpret (path : List Frame) : Summary → List Bool × List Frame
  | .empty => ([], path)
  | .observe dropFrames => ([allows (path.drop dropFrames)], path)
  | .sequence left right =>
    let (observations, next) := interpret path left
    let (later, final) := interpret next right
    (observations ++ later, final)
  | .comprehension first writes =>
    let (observations, next) := interpret path first
    (observations, publish writes next)

def evaluate (path : List Frame) (summary : Summary) : List Bool :=
  let declared := (declarations summary).foldl (fun frames write => declareAt write frames) path
  (interpret declared summary).1

-- Same declaration environment is essential: function locals are static.
theorem first_iterable_independent (path : List Frame) (first : Summary)
    (left right : List RoutedWrite) :
    (interpret path (.comprehension first left)).1 =
      (interpret path (.comprehension first right)).1 := by rfl

theorem publication_cannot_invent_builtin (fact : Fact) :
    fact.meet .shadowed ≠ .known .builtin := by
  cases fact <;> simp [Fact.meet]

theorem static_source_ineligible (id : Nat) (before whole : Env) (rest : List Frame) :
    allows ({ id, kind := .function, before, whole := whole.set .source .shadowed } :: rest) = false := by
  have h : (Fact.shadowed == Fact.known Target.builtin) = false := by decide
  simp [allows, resolveCandidate, resolve, resolveFrom, Env.set, Env.get, factResolution, h]

def firstWitness : Summary :=
  .comprehension (.observe 1) [{ name := .destination, owner := destination .destination [comp, module] }]
theorem early_publication_detected :
    evaluate [comp, module] firstWitness = [true] ∧
    (interpret (namedWrite .destination [comp, module]) (.observe 1)).1 = [false] := by decide

end HoiminOracle.ComprehensionBinding
