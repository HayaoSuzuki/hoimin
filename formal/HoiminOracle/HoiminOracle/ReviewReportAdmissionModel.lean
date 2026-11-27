namespace HoiminOracle.ReviewReportAdmission

inductive Artifact | missing | oversized | invalidJson | invalidConfig | wrongSchema | jsonl
  deriving DecidableEq

inductive Project | same | alias | other | missing
  deriving DecidableEq

inductive Baseline | passed | failed | missing
  deriving DecidableEq

structure Candidate where
  id : Nat
  originalMatches : Bool
  deriving DecidableEq

structure Report where
  artifact : Artifact := .jsonl
  summaryMatches : Bool := true
  identitiesMatch : Bool := true
  project : Project := .same
  baseline : Baseline := .passed
  complete : Bool := true
  candidates : List Candidate := []

inductive Failure
  | read | size | parse | schema | summary | identity | projectResolution | unusable | projectMismatch
  | descriptor
  deriving DecidableEq

inductive Outcome
  | rejected (failure : Failure)
  | admitted (candidateIds : List Nat)
  deriving DecidableEq

structure Observation where
  planProbes : Nat
  outcome : Outcome
  deriving DecidableEq

def reject (failure : Failure) : Observation := ⟨0, .rejected failure⟩

/-- Equal IDs select the plan row. The report can contain IDs absent from the plan. -/
def conflicts (planned : List Candidate) (current : Candidate) : Bool :=
  match planned.find? (fun row => row.id == current.id) with
  | none => false
  | some previous => previous.originalMatches != current.originalMatches

def join (planned reported : List Candidate) : Observation :=
  match reported.findIdx? (conflicts planned) with
  | some index => ⟨index + 1, .rejected .descriptor⟩
  | none => ⟨reported.length, .admitted (reported.map Candidate.id)⟩

def artifactFailure : Artifact → Option Failure
  | .missing => some .read
  | .oversized => some .size
  | .invalidJson | .invalidConfig => some .parse
  | .wrongSchema => some .schema
  | .jsonl => none

def integrityFailure (report : Report) : Option Failure :=
  if !report.summaryMatches then some .summary
  else if !report.identitiesMatch then some .identity
  else none

def usable (report : Report) : Bool := report.baseline == .passed && report.complete

def afterIntegrity (planned : List Candidate) (report : Report) : Observation :=
  if report.project = .missing then reject .projectResolution
  else if !usable report then reject .unusable
  else if report.project = .other then reject .projectMismatch
  else join planned report.candidates

def admit (planned : List Candidate) (report : Report) : Observation :=
  match artifactFailure report.artifact with
  | some failure => reject failure
  | none => match integrityFailure report with
    | some failure => reject failure
    | none => afterIntegrity planned report

theorem artifact_failure_precedes_report_checks (planned : List Candidate) (report : Report)
    (failure : Failure) (failed : artifactFailure report.artifact = some failure) :
    admit planned report = reject failure := by
  simp [admit, failed]

theorem summary_precedes_identity_and_project (planned : List Candidate) (report : Report)
    (decoded : report.artifact = .jsonl) (badSummary : report.summaryMatches = false) :
    admit planned report = reject .summary := by
  simp [admit, artifactFailure, integrityFailure, decoded, badSummary]

theorem resolution_precedes_usability_and_join (planned : List Candidate) (report : Report)
    (decoded : report.artifact = .jsonl) (valid : integrityFailure report = none)
    (missing : report.project = .missing) :
    admit planned report = reject .projectResolution := by
  simp [admit, afterIntegrity, decoded, valid, missing, artifactFailure]

theorem accepted_join_preserves_order (planned reported : List Candidate)
    (compatible : ∀ row ∈ reported, conflicts planned row = false) :
    join planned reported = ⟨reported.length, .admitted (reported.map Candidate.id)⟩ := by
  have found : reported.findIdx? (conflicts planned) = none :=
    List.findIdx?_eq_none_iff.mpr compatible
  simp [join, found]

theorem descriptor_rejection_stops_at_first_conflict (planned reported : List Candidate)
    (index : Nat)
    (first : ∃ inRange : index < reported.length,
      conflicts planned reported[index] ∧
        ∀ earlier (before : earlier < index),
          ¬conflicts planned (reported[earlier]'(Nat.lt_trans before inRange))) :
    join planned reported = ⟨index + 1, .rejected .descriptor⟩ := by
  have found := List.findIdx?_eq_some_iff_getElem.mpr first
  simp [join, found]

end HoiminOracle.ReviewReportAdmission
