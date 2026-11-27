import HoiminOracle.ImportRootModel

namespace HoiminOracle.TargetPath

open HoiminOracle.ImportRoot (Platform reduce)

inductive Selector
  | relative (components : List String)
  | absolute (components : List String)
  | invalidAnchor

def stripComponents (same : String → String → Bool) : List String → List String → Option (List String)
  | [], path => some path
  | _, [] => none
  | root :: parents, part :: path => if same root part then stripComponents same parents path else none

def rootComponents (platform : Platform) (components : List String) : List String :=
  components.filter fun part => part != "" && (platform == .windows || part != ".")

private def drivePrefix (component : String) : Bool :=
  match component.toList with
  | letter :: ':' :: _ => ('A' ≤ letter && letter ≤ 'Z') || ('a' ≤ letter && letter ≤ 'z')
  | _ => false

def relativeComponents (platform : Platform) (same : String → String → Bool)
    (root : List String) : Selector → Option (List String)
  | .relative path => some path
  | .invalidAnchor => none
  | .absolute path => do
    let remaining ← stripComponents same (rootComponents platform root) (rootComponents platform path)
    if platform == .windows && drivePrefix (remaining.headD "") then none else some remaining

def normalize (platform : Platform) (same : String → String → Bool) (root : List String)
    (path : Selector) : Option String :=
  ((relativeComponents platform same root path).bind (reduce [])).map (String.intercalate "/")

theorem strip_appended_path (same : String → String → Bool) (reflexive : ∀ part, same part part = true)
    (root child : List String) : stripComponents same root (root ++ child) = some child := by
  induction root with
  | nil => rfl
  | cons part rest ih => simp [stripComponents, reflexive, ih]

theorem strip_requires_whole_components (same : String → String → Bool) (root part : String)
    (parents path : List String) (different : same root part = false) :
    stripComponents same (root :: parents) (part :: path) = none := by
  simp [stripComponents, different]

theorem root_itself_is_empty (platform : Platform) (same : String → String → Bool)
    (reflexive : ∀ part, same part part = true) (root : List String) :
    normalize platform same root (.absolute root) = some "" := by
  have strip := strip_appended_path same reflexive (rootComponents platform root) []
  simp only [List.append_nil] at strip
  simp [normalize, relativeComponents, strip, drivePrefix, reduce]

theorem relative_escape_cannot_be_repaired (stack components suffix : List String)
    (escaped : reduce stack components = none) : reduce stack (components ++ suffix) = none := by
  induction components generalizing stack with
  | nil => simp [reduce] at escaped
  | cons part rest ih =>
    by_cases ignored : part == "" || part == "."
    · simp [reduce, ignored] at escaped ⊢
      exact ih stack escaped
    · by_cases parent : part == ".."
      · cases stack with
        | nil => simp [reduce, ignored, parent]
        | cons top previous =>
          simp [reduce, ignored, parent] at escaped ⊢
          exact ih previous escaped
      · simp [reduce, ignored, parent] at escaped ⊢
        exact ih (part :: stack) escaped

end HoiminOracle.TargetPath
