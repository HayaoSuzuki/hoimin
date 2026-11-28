namespace HoiminOracle.ImportRoot

inductive Platform | posix | windows
  deriving DecidableEq, BEq

inductive Issue
  | missingCliValue
  | invalid (path : String)
  | nonNormalized
  deriving DecidableEq, BEq

private def hasDrivePrefix (path : String) : Bool :=
  match path.toList with
  | letter :: ':' :: _ =>
      ('A' ≤ letter && letter ≤ 'Z') || ('a' ≤ letter && letter ≤ 'z')
  | _ => false

private def rooted (platform : Platform) (path : String) : Bool :=
  path.startsWith "/" ||
    (platform == .windows && (path.startsWith "\\" || hasDrivePrefix path))

def reduce (stack : List String) : List String → Option (List String)
  | [] => some stack.reverse
  | component :: rest =>
      if component == "" || component == "." then reduce stack rest
      else if component == ".." then
        match stack with
        | [] => none
        | _ :: parent => reduce parent rest
      else reduce (component :: stack) rest

def canonicalPath (platform : Platform) (path : String) : Except Issue String :=
  if path.isEmpty || rooted platform path then .error (.invalid path)
  else
    let separated := if platform == .windows then path.replace "\\" "/" else path
    match reduce [] (separated.splitOn "/") with
    | none => .error (.invalid path)
    | some [] => .ok "."
    | some parts => .ok (String.intercalate "/" parts)

private def canonicalize (platform : Platform) : List String → Except Issue (List String)
  | [] => .ok []
  | path :: rest =>
      match canonicalPath platform path with
      | .error issue => .error issue
      | .ok head =>
        match canonicalize platform rest with
        | .error issue => .error issue
        | .ok tail => .ok (head :: tail)

def normalize (platform : Platform) (input : List String) : Except Issue (List String) :=
  match canonicalize platform input with
  | .ok paths => .ok paths.eraseDups
  | .error issue => .error issue

def checked (platform : Platform) (input : List String) : Except Issue (List String) := do
  let normalized ← normalize platform input
  if normalized == input then pure input else throw .nonNormalized

-- Raw configuration passes the normalized list through checked
-- construction. On Windows the joined path can acquire a drive prefix.
def fromCli (platform : Platform) (input : List String) : Except Issue (List String) := do
  if input.contains "" then throw .missingCliValue
  let normalized ← normalize platform input
  checked platform normalized

theorem parent_at_root_rejects (rest : List String) : reduce [] (".." :: rest) = none := rfl

theorem first_invalid_path_is_reported (platform : Platform) (path : String)
    (rest : List String) (invalid : canonicalPath platform path = .error (.invalid path)) :
    normalize platform (path :: rest) = .error (.invalid path) := by
  simp [normalize, canonicalize, invalid]

theorem first_root_keeps_precedence (platform : Platform) (path canon : String)
    (rest normalized : List String) (first : canonicalPath platform path = .ok canon)
    (accepted : normalize platform (path :: rest) = .ok normalized) :
    normalized.head? = some canon := by
  cases remaining : canonicalize platform rest with
  | error issue => simp [normalize, canonicalize, first, remaining] at accepted
  | ok paths =>
    simp [normalize, canonicalize, first, remaining, List.eraseDups_cons] at accepted
    simp [← accepted]

end HoiminOracle.ImportRoot
