import Std

namespace HoiminOracle.PrivateAnnotationImport

inductive Origin where
  | typing | other
  deriving BEq, Repr, DecidableEq

def mangle (cls name : String) : String :=
  let stripped := String.ofList (cls.toList.dropWhile (· == '_'))
  if name.startsWith "__" && !(name.endsWith "__") && !stripped.isEmpty
  then "_" ++ stripped ++ name else name

def lookup (key : String) (env : List (String × Origin)) : Option Origin :=
  match env with
  | [] => none
  | (k, v) :: rest => if key == k then some v else lookup key rest

def envFor (cls alias : String) (overwrite : Bool) : List (String × Origin) :=
  let key := mangle cls alias
  if overwrite then [(key, .other), (key, .typing)] else [(key, .typing)]

def allowed (cls alias : String) (overwrite : Bool) : Bool :=
  lookup (mangle cls alias) (envFor cls alias overwrite) == some .typing

-- Deliberate defect: distinct AST spellings treated as distinct namespace keys.
def brokenAllowed (cls alias : String) (overwrite : Bool) : Bool :=
  let env := if overwrite then [(mangle cls alias, Origin.other), (alias, .typing)]
    else [(alias, Origin.typing)]
  lookup alias env == some .typing

set_option maxHeartbeats 10000 in
 theorem lastWriteShadows (key : String) (env : List (String × Origin)) :
    lookup key ((key, .other) :: env) = some .other := by simp [lookup]

-- Deliberate boundary defects: keep class-leading underscores / mangle suffix-dunder.
def brokenNoStrip (cls name : String) : String := "_" ++ cls ++ name
def brokenTrailing (cls name : String) : String := "_" ++ cls ++ name

def eligible (cls alias : String) (overwrite : Bool) : Bool :=
  allowed cls alias overwrite && mangle cls alias == alias

set_option maxHeartbeats 10000 in
theorem eligible_is_allowed (cls alias : String) (overwrite : Bool)
    (h : eligible cls alias overwrite = true) : allowed cls alias overwrite = true := by
  simp_all [eligible]

def sensitivity : Bool :=
  (mangle "_C" "__Alias" != brokenNoStrip "_C" "__Alias") &&
  (mangle "C" "__Alias__" != brokenTrailing "C" "__Alias__") &&
  (!allowed "C" "__Alias" true && brokenAllowed "C" "__Alias" true)

end HoiminOracle.PrivateAnnotationImport
