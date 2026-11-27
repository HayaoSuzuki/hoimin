namespace HoiminOracle.OperatorSelector

structure Operator where
  name : String
  enabled : Bool
  families : List String
  deriving DecidableEq, BEq

inductive Direction | including | excluding
  deriving DecidableEq, BEq

structure Argument where
  direction : Direction
  value : String

inductive Issue
  | unknown (selector : String)
  | empty
  deriving DecidableEq, BEq

def selectorNames (direction : Direction) (arguments : List Argument) : List String :=
  (arguments.filter (·.direction == direction)).flatMap (·.value.splitOn ",")

def expand (catalog : List Operator) (name : String) : Except Issue (List String) :=
  let family := catalog.filter fun operator => operator.families.contains name
  if !family.isEmpty then .ok (family.map Operator.name)
  else if catalog.any (·.name == name) then .ok [name]
  else .error (.unknown name)

def expandAll (catalog : List Operator) (names : List String) : Except Issue (List String) :=
  (names.mapM (expand catalog)).map List.flatten

def selected (catalog : List Operator) (included excluded : List String) : List String :=
  (catalog.map Operator.name).filter fun name => included.contains name && !(excluded.contains name)

def resolve (catalog : List Operator) (arguments : List Argument) : Except Issue (List String) := do
  let names := selectorNames .including arguments
  let included ←
    if names.isEmpty then pure ((catalog.filter Operator.enabled).map Operator.name)
    else expandAll catalog names
  let excluded ← expandAll catalog (selectorNames .excluding arguments)
  let members := selected catalog included excluded
  if members.isEmpty then throw .empty else pure members

theorem excluded_operator_is_absent (catalog : List Operator) (included excluded : List String)
    (name : String) (blocked : excluded.contains name = true) :
    name ∉ selected catalog included excluded := by
  intro member
  have present : name ∈ excluded := List.contains_iff_mem.mp blocked
  simp [selected, present] at member

theorem selected_operator_was_included (catalog : List Operator) (included excluded : List String)
    (name : String) (member : name ∈ selected catalog included excluded) :
    included.contains name = true := by
  simp only [selected, List.mem_filter, Bool.and_eq_true] at member
  exact member.2.1

end HoiminOracle.OperatorSelector
