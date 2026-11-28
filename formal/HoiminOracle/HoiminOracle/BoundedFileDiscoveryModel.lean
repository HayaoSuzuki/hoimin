namespace HoiminOracle.BoundedFileDiscovery

inductive Entry
  | other
  | python (path : String)
  | malformed (path : String) (isPython : Bool)
  | walkFailure

inductive Failure
  | cancelled
  | overLimit
  | unsupportedPath (path : String)
  | walk

structure State where
  polls : Nat := 0
  outcome : Except Failure (List String) := .ok []

def admit (limit : Nat) (paths : List String) : Entry → Except Failure (List String)
  | .other | .malformed _ false => .ok paths
  | .walkFailure => .error .walk
  | .malformed path true => .error (.unsupportedPath path)
  | .python path =>
    if path ∈ paths then .ok paths
    else if limit ≤ paths.length then .error .overLimit
    else .ok (path :: paths)

def stepWith (admitEntry : List String → Entry → Except Failure (List String))
    (cancelAt : Option Nat) (state : State) (entry : Entry) : State :=
  match state.outcome with
  | .error _ => state
  | .ok paths =>
    let next := state.polls + 1
    ⟨next, if cancelAt.any (· ≤ next) then .error .cancelled else admitEntry paths entry⟩

def step (limit : Nat) := stepWith (admit limit)

def scan (limit : Nat) (cancelAt : Option Nat) (normal restored : List Entry) : State :=
  restored.foldl (step limit cancelAt) (normal.foldl (step limit cancelAt) {})

def ValidScan (limit : Nat) (state : State) : Prop :=
  match state.outcome with
  | .error _ => True
  | .ok paths => paths.Nodup ∧ paths.length ≤ limit

theorem duplicate_keeps_paths (limit : Nat) (path : String) (paths : List String)
    (known : path ∈ paths) : admit limit paths (.python path) = .ok paths := by
  simp [admit, known]

theorem admit_preserves_budget (limit : Nat) (paths : List String) (entry : Entry)
    (within : paths.Nodup ∧ paths.length ≤ limit) : ValidScan limit ⟨0, admit limit paths entry⟩ := by
  cases entry with
  | other => exact within
  | walkFailure => trivial
  | malformed path python => cases python <;> simp [admit, ValidScan, within]
  | python path =>
    by_cases duplicate : path ∈ paths
    · simp [admit, duplicate, ValidScan, within]
    · by_cases full : limit ≤ paths.length
      · simp [admit, duplicate, full, ValidScan]
      · have room : paths.length + 1 ≤ limit := by omega
        simp [admit, duplicate, full, ValidScan, within.1, room]

theorem step_preserves_budget (limit : Nat) (cancelAt : Option Nat) (state : State) (entry : Entry)
    (within : ValidScan limit state) : ValidScan limit (step limit cancelAt state entry) := by
  cases state with
  | mk polls outcome =>
    cases outcome with
    | error reason => exact within
    | ok paths =>
      by_cases cancelled : cancelAt.any (· ≤ polls + 1)
      · simp [step, stepWith, cancelled, ValidScan]
      · exact by
          simpa [step, stepWith, cancelled, ValidScan] using admit_preserves_budget limit paths entry within

private theorem fold_preserves_budget (limit : Nat) (cancelAt : Option Nat) (entries : List Entry)
    (state : State) (within : ValidScan limit state) :
    ValidScan limit (entries.foldl (step limit cancelAt) state) := by
  induction entries generalizing state with
  | nil => exact within
  | cons entry rest ih => exact ih _ (step_preserves_budget limit cancelAt state entry within)

theorem two_pass_scan_preserves_budget (limit : Nat) (cancelAt : Option Nat) (normal restored : List Entry) :
    ValidScan limit (scan limit cancelAt normal restored) :=
  fold_preserves_budget limit cancelAt restored _ (fold_preserves_budget limit cancelAt normal {} (by simp [ValidScan]))

theorem cancellation_precedes_any_entry (limit polls : Nat) (paths : List String) (entry : Entry) :
    step limit (some (polls + 1)) ⟨polls, .ok paths⟩ entry = ⟨polls + 1, .error .cancelled⟩ := by
  simp [step, stepWith]

theorem failed_scan_does_not_poll (limit : Nat) (cancelAt : Option Nat) (polls : Nat)
    (reason : Failure) (entry : Entry) :
    step limit cancelAt ⟨polls, .error reason⟩ entry = ⟨polls, .error reason⟩ := rfl

end HoiminOracle.BoundedFileDiscovery
