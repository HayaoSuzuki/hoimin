namespace HoiminOracle.SymbolCatalog

abbrev Name := List String

structure Line where
  before : String
  value : Option Bool := none
  after : String := ""

inductive Suite
  | empty
  | line (line : Line) (remaining : Suite)
  | block (heading : Line) (body remaining : Suite)
  | definition (name : String) (heading : List Line) (body remaining : Suite)

def definitions (enclosing : Name) : Suite → List Name
  | .empty => []
  | .line _ remaining => definitions enclosing remaining
  | .block _ body remaining => definitions enclosing body ++ definitions enclosing remaining
  | .definition name _ body remaining =>
    let qualified := enclosing ++ [name]
    qualified :: definitions qualified body ++ definitions enclosing remaining

structure Selector where
  name : Name
  modulePath : Option Name := none
  deriving BEq

def moduleMatches (current requested : Name) : Bool :=
  requested.reverse.isPrefixOf current.reverse

def availableSelectors (current : Name) (selectors : List Selector) : List Name :=
  selectors.filterMap fun selector =>
    if selector.modulePath.all (moduleMatches current) then some selector.name else none

def selected (current : Name) (selectors : List Selector) (enclosing : Name) : Bool :=
  selectors.isEmpty || (!enclosing.isEmpty &&
    (availableSelectors current selectors).any (·.isPrefixOf enclosing))

def modulePath (parents : Name) (stem : String) : Name :=
  if stem == "__init__" then parents else parents ++ [stem]

def dotted (name : Name) : String := String.intercalate "." name

theorem transparent_block_keeps_qualifier (enclosing : Name) (heading : Line) (body rest : Suite) :
    definitions enclosing (.block heading body rest) =
      definitions enclosing body ++ definitions enclosing rest := rfl

theorem definition_scopes_body_and_preserves_sibling_scope (enclosing : Name) (name : String)
    (heading : List Line) (body siblings : Suite) :
    definitions enclosing (.definition name heading body siblings) =
      (enclosing ++ [name]) :: definitions (enclosing ++ [name]) body ++ definitions enclosing siblings := rfl

theorem unrestricted_includes_module_body (current : Name) :
    selected current [] [] = true := rfl

theorem restricted_selection_excludes_module_body (current : Name) (head : Selector)
    (tail : List Selector) : selected current (head :: tail) [] = false := rfl

theorem a_selected_ancestor_includes_descendants (current ancestor tail : Name)
    (nonempty : ancestor ≠ []) :
    selected current [⟨ancestor, none⟩] (ancestor ++ tail) = true := by
  simp [selected, availableSelectors, nonempty]

theorem distinct_name_component_is_not_an_ancestor (enclosing : Name) (left right : String)
    (suffix : Name) (different : left ≠ right) :
    (enclosing ++ [left]).isPrefixOf (enclosing ++ right :: suffix) = false := by
  induction enclosing with
  | nil => simp [List.isPrefixOf, different]
  | cons head rest ih => simpa using ih

theorem module_suffix_matches_at_component_boundary (parents requested : Name) :
    moduleMatches (parents ++ requested) requested = true := by
  simp [moduleMatches, List.reverse_append]

end HoiminOracle.SymbolCatalog
