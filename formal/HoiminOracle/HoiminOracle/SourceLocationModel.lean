namespace HoiminOracle.SourceLocation

structure Location where
  line : Nat := 1
  column : Nat := 0
  deriving BEq

structure Boundary where
  offset : Nat
  location : Location
  startsLine : Bool
  deriving BEq

def utf8Width (scalar : Char) : Nat := (String.singleton scalar).utf8ByteSize
def bom : Char := Char.ofNat 0xfeff

def endsLine (scalar : Char) (next : Option Char) : Bool :=
  scalar == '\n' || (scalar == '\r' && next != some '\n')

def countsColumn (offset : Nat) (scalar : Char) : Nat :=
  if offset == 0 && scalar == bom then 0 else 1

def step (breakAfter : Char → Option Char → Bool) (columnWidth : Nat → Char → Nat)
    (state : Boundary) (scalar : Char) (next : Option Char) : Boundary :=
  let offset := state.offset + utf8Width scalar
  if breakAfter scalar next then ⟨offset, ⟨state.location.line + 1, 0⟩, true⟩
  else ⟨offset, { state.location with column := state.location.column + columnWidth state.offset scalar }, false⟩

def traceFrom (breakAfter : Char → Option Char → Bool) (columnWidth : Nat → Char → Nat) :
    Boundary → List Char → List Boundary
  | cursor, [] => [cursor]
  | cursor, scalar :: rest =>
    cursor :: traceFrom breakAfter columnWidth (step breakAfter columnWidth cursor scalar rest.head?) rest

def traceWith (breakAfter : Char → Option Char → Bool) (columnWidth : Nat → Char → Nat) (input : String) :
    List Boundary :=
  traceFrom breakAfter columnWidth ⟨0, {}, true⟩ input.toList

def trace := traceWith endsLine countsColumn

def locate (boundaries : List Boundary) (offset : Nat) : Option Location :=
  (boundaries.find? (fun boundary => boundary.offset == offset)).map Boundary.location

def starts (boundaries : List Boundary) : List Nat :=
  (boundaries.filter (·.startsLine)).map Boundary.offset

def excess (widths : List Nat) : Nat := (widths.map (· - 1)).sum

theorem sum_is_scalar_count_plus_excess (widths : List Nat) (positive : ∀ w ∈ widths, 0 < w) :
    widths.sum = widths.length + excess widths := by
  induction widths with
  | nil => rfl
  | cons width rest ih =>
    have first := positive width (by simp)
    have tail := ih (fun w member => positive w (by simp [member]))
    simp only [excess, List.sum_cons, List.length_cons, List.map_cons] at tail ⊢
    omega

theorem indexed_column_counts_scalars (earlier within : List Nat) (hiddenInitialBom : Nat)
    (positive : ∀ w ∈ within, 0 < w) :
    within.sum - (excess (earlier ++ within) - excess earlier) - hiddenInitialBom =
      within.length - hiddenInitialBom := by
  have expanded : excess (earlier ++ within) = excess earlier + excess within := by
    simp [excess]
  rw [expanded]
  have count := sum_is_scalar_count_plus_excess within positive
  omega

theorem remaining_line_at_crlf_midpoint (cursor : Boundary) :
    (step endsLine countsColumn cursor '\r' (some '\n')).location =
      ⟨cursor.location.line, cursor.location.column + 1⟩ := by
  have crNotBom : '\r' ≠ bom := by decide
  simp [step, endsLine, countsColumn, crNotBom]

theorem crlf_finishes_once (cursor : Boundary) (next : Option Char) :
    step endsLine countsColumn (step endsLine countsColumn cursor '\r' (some '\n')) '\n' next =
      ⟨cursor.offset + 2, ⟨cursor.location.line + 1, 0⟩, true⟩ := by
  simp [step, endsLine]
  rfl

theorem bom_at_origin_is_not_a_column (next : Option Char) :
    step endsLine countsColumn ⟨0, {}, true⟩ bom next = ⟨3, {}, false⟩ := by
  simp [step, endsLine, countsColumn]
  rfl

theorem subsequent_bom_counts (offset : Nat) (nonzero : offset ≠ 0) :
    countsColumn offset bom = 1 := by
  simp [countsColumn, nonzero]

end HoiminOracle.SourceLocation
