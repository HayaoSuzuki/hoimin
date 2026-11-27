namespace HoiminOracle.AstDepth

/-- An ordered forest. Each entry contains the node's own forest of children. -/
inductive Forest
  | empty
  | cons (children siblings : Forest)

def visitOrder (reverseSiblings : Bool) (depth : Nat) : Forest → List Nat
  | .empty => []
  | .cons children siblings =>
    let here := depth :: visitOrder reverseSiblings (depth + 1) children
    let rest := visitOrder reverseSiblings depth siblings
    if reverseSiblings then rest ++ here else here ++ rest

structure Frame where
  depth : Nat
  children : Forest

def pushChildren (depth : Nat) : Forest → List Frame
  | .empty => []
  | .cons children siblings => pushChildren depth siblings ++ [⟨depth, children⟩]

def flatten (pending : List Frame) : List Nat :=
  pending.flatMap fun frame => frame.depth :: visitOrder true (frame.depth + 1) frame.children

inductive Result | accepted | syntax | cancelled | depth | outOfFuel
  deriving BEq

structure Observation where
  result : Result
  polls : Nat
  deriving BEq

def stopAt (limit : Nat) (cancelAt : Option Nat) (poll depth : Nat) : Option Result :=
  if cancelAt == some poll then some .cancelled
  else if limit < depth then some .depth
  else none

def scan (limit : Nat) (cancelAt : Option Nat) : Nat → List Nat → Observation
  | polls, [] => ⟨.accepted, polls⟩
  | polls, depth :: rest =>
    match stopAt limit cancelAt (polls + 1) depth with
    | some reason => ⟨reason, polls + 1⟩
    | none => scan limit cancelAt (polls + 1) rest

def worklist (limit : Nat) (cancelAt : Option Nat) : Nat → Nat → List Frame → Observation
  | _, polls, [] => ⟨.accepted, polls⟩
  | 0, polls, _ :: _ => ⟨.outOfFuel, polls⟩
  | fuel + 1, polls, frame :: rest =>
    match stopAt limit cancelAt (polls + 1) frame.depth with
    | some reason => ⟨reason, polls + 1⟩
    | none => worklist limit cancelAt fuel (polls + 1)
        (pushChildren (frame.depth + 1) frame.children ++ rest)

def checked (syntaxValid : Bool) (limit : Nat) (cancelAt : Option Nat)
    (schedule : List Nat) : Observation :=
  if syntaxValid then scan limit cancelAt 0 schedule else ⟨.syntax, 0⟩

theorem flatten_push (forest : Forest) (depth : Nat) :
    flatten (pushChildren depth forest) = visitOrder true depth forest := by
  induction forest with
  | empty => rfl
  | cons children rest _ ih =>
    simp [pushChildren, flatten, visitOrder, List.flatMap_append, ← ih]

theorem flatten_after_step (frame : Frame) (rest : List Frame) :
    flatten (pushChildren (frame.depth + 1) frame.children ++ rest) =
      (flatten (frame :: rest)).tail := by
  rw [show flatten (pushChildren (frame.depth + 1) frame.children ++ rest) =
    flatten (pushChildren (frame.depth + 1) frame.children) ++ flatten rest by
      simp [flatten, List.flatMap_append], flatten_push]
  simp [flatten]

theorem worklist_matches_schedule (limit : Nat) (cancelAt : Option Nat) (fuel polls : Nat)
    (pending : List Frame) (enough : (flatten pending).length ≤ fuel) :
    worklist limit cancelAt fuel polls pending = scan limit cancelAt polls (flatten pending) := by
  induction fuel generalizing pending polls with
  | zero =>
    cases pending with
    | nil => rfl
    | cons frame rest => simp [flatten] at enough
  | succ fuel ih =>
    cases pending with
    | nil => rfl
    | cons frame rest =>
      have scheduled : flatten (frame :: rest) = frame.depth ::
          flatten (pushChildren (frame.depth + 1) frame.children ++ rest) := by
        rw [flatten_after_step]
        rfl
      rw [scheduled] at enough ⊢
      have remaining : (flatten (pushChildren (frame.depth + 1) frame.children ++ rest)).length ≤ fuel := by
        simpa using enough
      simp only [worklist, scan]
      split
      · rfl
      · exact ih _ _ remaining

theorem invalid_syntax_has_no_polls (limit : Nat) (cancelAt : Option Nat) (schedule : List Nat) :
    checked false limit cancelAt schedule = ⟨.syntax, 0⟩ := rfl

theorem cancellation_precedes_depth (limit poll depth : Nat) :
    stopAt limit (some poll) poll depth = some .cancelled := by
  simp [stopAt]

theorem at_limit_without_cancellation (poll limit : Nat) :
    stopAt limit none poll limit = none := by
  simp [stopAt]

theorem accepts_iff_every_node_within_limit (limit polls : Nat) (schedule : List Nat) :
    (scan limit none polls schedule).result = .accepted ↔ ∀ depth ∈ schedule, depth ≤ limit := by
  induction schedule generalizing polls with
  | nil => simp [scan]
  | cons depth rest ih =>
    by_cases within : depth ≤ limit
    · simp [scan, stopAt, Nat.not_lt_of_ge within, ih, within]
    · simp [scan, stopAt, Nat.lt_of_not_ge within, within]

end HoiminOracle.AstDepth
