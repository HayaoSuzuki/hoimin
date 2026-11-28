namespace HoiminOracle.ReviewSource

inductive File
  | invalidPath
  | missing
  | utf8
  | latin1
  | invalidUtf8
  | unsupportedEncoding
  deriving DecidableEq

def File.attemptsRead : File → Bool
  | .invalidPath => false
  | _ => true

def File.hasContext : File → Bool
  | .invalidPath | .missing => false
  | _ => true

inductive Shape
  | valid
  | mutation
  | span
  | original
  | location
  | identity
  deriving DecidableEq

structure Candidate where
  path : String
  hashMatches : Bool
  shape : Shape

inductive Failure
  | path | mutation | span | original | utf8 | encoding | location | identity
  deriving DecidableEq

inductive Outcome
  | fresh
  | stale
  | unavailable
  | invalid (failure : Failure)
  deriving DecidableEq

def validate (file : File) (shape : Shape) : Outcome :=
  if shape = .mutation then .invalid .mutation
  else if shape = .span then .invalid .span
  else if shape = .original then .invalid .original
  else if file = .invalidUtf8 then .invalid .utf8
  else if file = .unsupportedEncoding then .invalid .encoding
  else if shape = .location then .invalid .location
  else if shape = .identity then .invalid .identity
  else .fresh

def classifyWith (validator : File → Shape → Outcome) (file : File) (candidate : Candidate) : Outcome :=
  match file with
  | .invalidPath => .invalid .path
  | .missing => .unavailable
  | _ => if candidate.hashMatches then validator file candidate.shape else .stale

def classify := classifyWith validate

def paths (candidates : List Candidate) : List String :=
  (candidates.map Candidate.path).eraseDups

def captured (files : String → File) (candidates : List Candidate) : List (String × File) :=
  (paths candidates).map fun path => (path, files path)

def lookup (sources : List (String × File)) (path : String) : File :=
  ((sources.find? fun row => row.1 == path).map Prod.snd).getD .missing

def outcomes (files : String → File) (candidates : List Candidate) : List Outcome :=
  let sources := captured files candidates
  candidates.map fun candidate => classify (lookup sources candidate.path) candidate

private theorem lookup_observed_path (files : String → File) (observed : List String)
    (path : String) (member : path ∈ observed) :
    lookup (observed.map fun key => (key, files key)) path = files path := by
  induction observed with
  | nil => simp at member
  | cons key rest ih =>
    by_cases equal : key = path
    · subst key
      simp [lookup]
    · have tail : path ∈ rest := by simpa [Ne.symm equal] using member
      simpa [lookup, List.find?_cons, equal] using ih tail

theorem grouping_preserves_candidate_order (files : String → File) (candidates : List Candidate) :
    outcomes files candidates = candidates.map (fun candidate => classify (files candidate.path) candidate) := by
  unfold outcomes
  apply List.map_congr_left
  intro candidate member
  have seen : candidate.path ∈ paths candidates := by
    exact List.mem_eraseDups.mpr (List.mem_map.mpr ⟨candidate, member, rfl⟩)
  rw [captured, lookup_observed_path files (paths candidates) candidate.path seen]

private theorem distinct_first_paths (input : List String) : input.eraseDups.Nodup := by
  cases input with
  | nil => simp
  | cons head rest =>
    rw [List.eraseDups_cons, List.nodup_cons]
    refine ⟨?_, distinct_first_paths _⟩
    simp
termination_by input.length
decreasing_by
  simp only [List.length_cons]
  exact Nat.lt_succ_of_le (List.length_filter_le ..)

theorem each_path_is_visited_once (candidates : List Candidate) : (paths candidates).Nodup :=
  distinct_first_paths _

theorem first_candidate_path_is_visited_first (first : Candidate) (rest : List Candidate) :
    (paths (first :: rest)).head? = some first.path := by
  simp [paths, List.eraseDups_cons]

theorem stale_precedes_candidate_validation (file : File) (candidate : Candidate)
    (hasContext : file.hasContext = true) (hashDiffers : candidate.hashMatches = false) :
    classify file candidate = .stale := by
  cases file <;> simp [File.hasContext, classify, classifyWith, hashDiffers] at *

theorem wrong_original_precedes_deferred_decoding (file : File) :
    validate file .original = .invalid .original := by
  simp [validate]

end HoiminOracle.ReviewSource
