namespace HoiminOracle.RelativePath

inductive Policy | portable | unixWorkspace

inductive Platform | unix | windows

def ordinary (component : String) : Bool := component != "" && component != "."

def allowedPart (policy : Policy) (component : String) : Bool :=
  ordinary component && (component != ".." &&
    component.toList.all (fun c => c != '\\' && c != '\x00' && c != '/') &&
    match policy with
    | .portable => !component.contains ':'
    | .unixWorkspace => true)

def allows (policy : Policy) (components : List String) : Bool :=
  !components.isEmpty && components.all (allowedPart policy)

def lexical (policy : Policy) (input : String) : Bool := allows policy (input.splitOn "/")

def normalizeParts (components : List String) : Option (List String) :=
  match components with
  | [] => none
  | first :: _ =>
    let cleaned := components.filter ordinary
    if first != "" && allows .portable cleaned then some cleaned else none

def exactFile (platform : Platform) (input : String) : Option String :=
  let slash := match platform with
    | .unix => input
    | .windows => String.ofList (input.toList.map fun c => if c == '\\' then '/' else c)
  (normalizeParts (slash.splitOn "/")).map (String.intercalate "/")

inductive CandidateOutcome | accepted | unsupportedSchema | invalidPath | hashMismatch

def candidate (pathAllowed schemaMatches hashMatches : Bool) : CandidateOutcome :=
  if !schemaMatches then .unsupportedSchema
  else if !pathAllowed then .invalidPath
  else if !hashMatches then .hashMismatch
  else .accepted

theorem portable_implies_unix (components : List String) (valid : allows .portable components = true) :
    allows .unixWorkspace components = true := by
  simp only [allows, Bool.and_eq_true, List.all_eq_true] at valid ⊢
  exact ⟨valid.1, fun part member => by
    have good := valid.2 part member
    simp_all [allowedPart]⟩

theorem normalized_output_is_portable (components result : List String)
    (normalized : normalizeParts components = some result) : allows .portable result = true := by
  cases components with
  | nil => simp [normalizeParts] at normalized
  | cons first rest =>
    dsimp [normalizeParts] at normalized
    split at normalized
    · rename_i accepted
      simp only [Bool.and_eq_true] at accepted
      simpa [← Option.some.inj normalized] using accepted.2
    · simp at normalized

theorem normalize_preserves_portable (components : List String) (valid : allows .portable components = true) :
    normalizeParts components = some components := by
  cases components with
  | nil => simp [allows] at valid
  | cons first rest =>
    have every := valid
    simp only [allows, Bool.and_eq_true, List.all_eq_true] at every
    have ordinaryParts : ∀ part ∈ first :: rest, ordinary part = true := by
      intro part member
      have allowed := every.2 part member
      simp only [allowedPart, Bool.and_eq_true] at allowed
      exact allowed.1
    have unchanged := List.filter_eq_self.mpr ordinaryParts
    have relative := ordinaryParts first (by simp)
    simp only [ordinary, Bool.and_eq_true] at relative
    simp [normalizeParts, unchanged, relative.1, valid]

theorem normalization_is_idempotent (components result : List String)
    (normalized : normalizeParts components = some result) : normalizeParts result = some result :=
  normalize_preserves_portable result (normalized_output_is_portable components result normalized)

theorem invalid_path_precedes_hash (hashMatches : Bool) :
    candidate false true hashMatches = .invalidPath := rfl

end HoiminOracle.RelativePath
