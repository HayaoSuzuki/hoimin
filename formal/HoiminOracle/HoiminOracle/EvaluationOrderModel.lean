import Std

namespace OrderAudit

inductive Event where
  | bindSource | bindDestination | lookup
  deriving DecidableEq, BEq, Repr

def observations (source destination : Bool) : List Event → List Bool
  | [] => []
  | .bindSource :: es => observations false destination es
  | .bindDestination :: es => observations source false es
  | .lookup :: es => (source && destination) :: observations source destination es

theorem source_shadowed_rejects (destination : Bool) (es : List Event) :
    (observations false destination es).all (! ·) = true := by
  induction es generalizing destination with
  | nil => rfl
  | cons e es ih =>
    cases e <;> simp [observations, ih]

theorem destination_shadowed_rejects (source : Bool) (es : List Event) :
    (observations source false es).all (! ·) = true := by
  induction es generalizing source with
  | nil => rfl
  | cons e es ih =>
    cases e <;> simp [observations, ih]

theorem lookup_observes_current (source destination : Bool) (es : List Event) :
    (observations source destination (.lookup :: es)).head? = some (source && destination) := by
  rfl

theorem bind_then_lookup_rejects (es : List Event) :
    (observations true true (.bindSource :: .lookup :: es)).head? = some false := by rfl

theorem lookup_before_bind_preserved (es : List Event) :
    (observations true true (.lookup :: .bindSource :: es)).head? = some true := by rfl

def candidateCount (es : List Event) : Nat := ((observations true true es).filter id).length

-- Deliberately broken: all writes are delayed until the statement completes.
def brokenDeferredWrites (es : List Event) : List Bool :=
  (es.filter (· == .lookup)).map (fun _ => true)

end OrderAudit
