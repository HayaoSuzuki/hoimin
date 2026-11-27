namespace HoiminOracle.FormatRemoval

inductive ExpressionKind | safe | assignment | yield
  deriving BEq

structure Field where
  expression : String := "value"
  grouping : Nat := 0
  innerDepth : Nat := 0
  debug : Bool := false
  conversion : Option Char := none
  kind : ExpressionKind := .safe

inductive Elements
  | absent
  | empty
  | text (value : String) (rest : Elements)
  | interpolation (field : Field) (specification rest : Elements)

structure Reads where
  expressions : List String := []
  blocksRemoval : Bool := false
  depth : Nat := 0

def footprint : Elements → Reads
  | .absent | .empty => {}
  | .text _ rest => footprint rest
  | .interpolation field nested rest =>
    let within := footprint nested
    let after := footprint rest
    {
      expressions := field.expression :: within.expressions ++ after.expressions
      blocksRemoval := field.kind != .safe || within.blocksRemoval || after.blocksRemoval
      depth := max (1 + max (field.grouping + field.innerDepth) within.depth) after.depth
    }

inductive Removal
  | omit
  | guard (explicitRepr : Bool) (expressions : List String)
  deriving BEq

def needsRepr (field : Field) : Bool := field.debug && field.conversion.isNone

def select (field : Field) (ambientDepth : Nat) (specification : Elements) : Option Removal :=
  match specification with
  | .absent => none
  | .empty => if field.debug then some .omit else none
  | spec =>
    let reads := footprint spec
    if reads.blocksRemoval then none
    else if reads.expressions.isEmpty then some .omit
    else if ambientDepth + reads.depth + 2 ≤ 200 then
      some (.guard (needsRepr field) reads.expressions)
    else none

def retainedReads : Removal → List String
  | .omit => []
  | .guard _ expressions => expressions

def executedReads (emptyArmSelected : Bool) (removal : Removal) : List String :=
  if emptyArmSelected then [] else retainedReads removal

def renderReplacement (quote : Char) : Removal → String
  | .omit => ""
  | .guard explicitRepr expressions =>
    let alternateEmpty := if quote == '"' then "''" else "\"\""
    let tuple := String.join (expressions.map fun expression => "(" ++ expression ++ "),")
    (if explicitRepr then "!r" else "") ++ ":{" ++ alternateEmpty ++ " if True else (" ++ tuple ++ ")}"

theorem nested_reads_follow_source_order (field : Field) (nested rest : Elements) :
    (footprint (.interpolation field nested rest)).expressions =
      field.expression :: (footprint nested).expressions ++ (footprint rest).expressions := rfl

theorem guard_preserves_reads_in_unselected_arm (repr : Bool) (expressions : List String) :
    executedReads true (.guard repr expressions) = [] ∧
      executedReads false (.guard repr expressions) = expressions := ⟨rfl, rfl⟩

theorem unsafe_read_blocks_enclosing_removal (field : Field) (ambient : Nat)
    (child : Field) (nested rest : Elements) (unsafeRead : (child.kind != .safe) = true) :
    select field ambient (.interpolation child nested rest) = none := by
  simp [select, footprint, unsafeRead]

theorem dynamic_rewrite_uses_original_reads (field : Field) (ambient : Nat)
    (child : Field) (nested rest : Elements)
    (safe : (footprint (.interpolation child nested rest)).blocksRemoval = false)
    (within : ambient + (footprint (.interpolation child nested rest)).depth + 2 ≤ 200) :
    select field ambient (.interpolation child nested rest) =
      some (.guard (needsRepr field) (footprint (.interpolation child nested rest)).expressions) := by
  simp only [footprint] at safe within ⊢
  simp [select, footprint, safe, within]

end HoiminOracle.FormatRemoval
