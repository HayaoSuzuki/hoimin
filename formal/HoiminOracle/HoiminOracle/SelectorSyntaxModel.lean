namespace HoiminOracle.SelectorSyntax

inductive Platform | posix | windows
  deriving DecidableEq, BEq

def maxLine : Nat := 4294967295

structure Line where
  path : String
  start : Nat
  finish : Nat
  deriving DecidableEq, BEq

structure Symbol where
  module : String
  qualname : String
  deriving DecidableEq, BEq

inductive Argument
  | line (value : String)
  | symbol (value : String)
  deriving DecidableEq, BEq

inductive Issue
  | line (value : String)
  | symbol (value : String)
  | symbolNeedsSource
  | missing
  deriving DecidableEq, BEq

structure Selection where
  lines : List Line
  symbols : List Symbol
  deriving DecidableEq, BEq

def splitFirst (separator value : String) : Option (String × String) :=
  match value.splitOn separator with
  | first :: next :: rest => some (first, String.intercalate separator (next :: rest))
  | _ => none

def splitLast (separator value : String) : Option (String × String) :=
  match (value.splitOn separator).reverse with
  | last :: before :: rest => some (String.intercalate separator (before :: rest).reverse, last)
  | _ => none

private def decimal (value : String) : Option Nat :=
  let digits := if value.startsWith "+" then (value.drop 1).toString else value
  if digits.toList.all Char.isDigit then digits.toNat? else none

def validRange (start finish : Nat) : Bool :=
  0 < start && start ≤ finish && finish ≤ maxLine

theorem valid_range_is_one_based_and_bounded (start finish : Nat)
    (valid : validRange start finish = true) :
    1 ≤ start ∧ start ≤ finish ∧ finish ≤ maxLine := by
  simp [validRange] at valid
  omega

private def nativePath (platform : Platform) (path : String) : Option String :=
  if path.isEmpty then none
  else if platform == .windows then some (path.replace "\\" "/")
  else if path.contains '\\' then none
  else some path

def parseLineWith (split : String → Option (String × String)) (platform : Platform)
    (value : String) : Option Line := do
  let (path, range) ← split value
  let path ← nativePath platform path
  let (start, finish) := (splitFirst "-" range).getD (range, range)
  let start ← decimal start
  let finish ← decimal finish
  if validRange start finish then some ⟨path, start, finish⟩ else none

def parseLine := parseLineWith (splitLast ":")

def parseSymbolWith (split : String → Option (String × String)) (value : String) :
    Option Symbol := do
  let (module, qualname) ← split value
  if module.isEmpty || qualname.isEmpty then none else some ⟨module, qualname⟩

def parseSymbol := parseSymbolWith (splitFirst ":")

def lineArgs (arguments : List Argument) : List String :=
  arguments.filterMap fun arg => match arg with | .line value => some value | _ => none

def symbolArgs (arguments : List Argument) : List String :=
  arguments.filterMap fun arg => match arg with | .symbol value => some value | _ => none

def require (issue : Issue) : Option α → Except Issue α
  | some result => .ok result
  | none => .error issue

def resolveWith (lineParser : String → Option Line) (symbolParser : String → Option Symbol)
    (source : Bool) (arguments : List Argument) : Except Issue Selection := do
  if !source && arguments.isEmpty then throw .missing
  let lines ← (lineArgs arguments).mapM fun value =>
    require (.line value) (lineParser value)
  let symbols ← (symbolArgs arguments).mapM fun value =>
    require (.symbol value) (symbolParser value)
  if !source && !symbols.isEmpty then throw .symbolNeedsSource
  pure ⟨lines, symbols⟩

def resolve (platform : Platform) :=
  resolveWith (parseLine platform) parseSymbol

end HoiminOracle.SelectorSyntax
