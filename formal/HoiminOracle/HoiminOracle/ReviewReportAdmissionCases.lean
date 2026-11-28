import HoiminOracle.ReviewReportAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.ReviewReportAdmission

private def plan : List Candidate := [⟨0, true⟩]
private def matchPlan : Candidate := ⟨0, true⟩
private def differFromPlan : Candidate := ⟨0, false⟩
private def extraA : Candidate := ⟨1, false⟩
private def extraB : Candidate := ⟨2, true⟩

private def gates : List (String × Report) :=
  let projects := [("same", Project.same), ("alias", .alias), ("other", .other), ("missing", .missing)]
  let baselines := [("passed", Baseline.passed), ("failed", .failed), ("missing", .missing)]
  let rows := [("matching", [matchPlan, extraA]), ("conflicting", [extraA, differFromPlan]),
    ("report-only", [extraB, extraA])]
  projects.flatMap fun (projectName, project) => baselines.flatMap fun (baselineName, baseline) =>
    [true, false].flatMap fun complete =>
      let possible := if baseline = .missing then [("no-candidates", [])] else rows
      possible.map fun (memberName, candidates) =>
        (s!"gates-{projectName}-{baselineName}-{complete}-{memberName}",
          { project, baseline, complete, candidates })

private def integrityCases : List (String × Report) :=
  let states := [("summary", false, true), ("identity", true, false), ("both", false, false)]
  let projects := [("same", Project.same), ("other", .other), ("missing", .missing)]
  states.flatMap fun (name, summaryMatches, identitiesMatch) =>
    projects.flatMap fun (projectName, project) => [true, false].map fun complete =>
      (s!"integrity-{name}-{projectName}-{complete}",
        { summaryMatches, identitiesMatch, project, complete, candidates := [extraA, differFromPlan] })

private def outerCases : List (String × Report) :=
  let artifacts := [("missing", Artifact.missing), ("oversized", .oversized),
    ("invalid-json", .invalidJson), ("invalid-config", .invalidConfig), ("wrong-schema", .wrongSchema)]
  artifacts.flatMap fun (name, artifact) =>
    let competing : Report :=
      { artifact
        summaryMatches := false
        identitiesMatch := false
        project := .missing
        baseline := .failed
        complete := false
        candidates := [differFromPlan] }
    [("ordinary", { artifact, candidates := [matchPlan] }), ("competing-failures", competing)].map
      fun (variant, input) => (s!"artifact-{name}-{variant}", input)

private def orderCases : List (String × Report) :=
  [("agree", matchPlan), ("disagree", differFromPlan)].flatMap fun (mode, overlap) =>
    [[overlap, extraA, extraB], [overlap, extraB, extraA], [extraA, overlap, extraB],
      [extraB, overlap, extraA], [extraA, extraB, overlap], [extraB, extraA, overlap]].zipIdx.map
      fun (candidates, index) => (s!"order-{mode}-{index}", { candidates })

private def cases :=
  gates ++ integrityCases ++ outerCases ++ orderCases ++ [("complete-empty", ({} : Report))]

private def artifactName : Artifact → String
  | .missing => "missing"
  | .oversized => "oversized"
  | .invalidJson => "invalid_json"
  | .invalidConfig => "invalid_config"
  | .wrongSchema => "wrong_schema"
  | .jsonl => "jsonl"

private def failureName : Failure → String
  | .read => "read"
  | .size => "size"
  | .parse => "parse"
  | .schema => "schema"
  | .summary => "summary"
  | .identity => "identity"
  | .projectResolution => "project_resolution"
  | .unusable => "unusable"
  | .projectMismatch => "project_mismatch"
  | .descriptor => "descriptor"

private def candidateJson (candidate : Candidate) : Lean.Json :=
  Lean.Json.mkObj [("id", Lean.toJson candidate.id), ("original_matches", Lean.toJson candidate.originalMatches)]

private def outcomeJson : Outcome → Lean.Json
  | .rejected failure => Lean.Json.mkObj [("kind", .str "rejected"), ("failure", .str (failureName failure))]
  | .admitted ids => Lean.Json.mkObj [("kind", .str "admitted"), ("candidate_ids", Lean.toJson ids)]

def corpus : String :=
  String.join (cases.map fun (id, report) =>
    let expected := admit plan report
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("mode", .str "internal-fixture"),
      ("id", .str id),
      ("artifact", .str (artifactName report.artifact)),
      ("summary_matches", Lean.toJson report.summaryMatches),
      ("identities_match", Lean.toJson report.identitiesMatch),
      ("project", .str (match report.project with
        | .same => "same" | .alias => "alias" | .other => "other" | .missing => "missing")),
      ("baseline", .str (match report.baseline with
        | .passed => "passed" | .failed => "failed" | .missing => "missing")),
      ("complete", Lean.toJson report.complete),
      ("candidates", .arr (report.candidates.toArray.map candidateJson)),
      ("plan_probes", Lean.toJson expected.planProbes),
      ("outcome", outcomeJson expected.outcome)
    ]).compress ++ "\n")

private def decoded (report : Report) : Bool :=
  report.artifact == .jsonl && (integrityFailure report).isNone

def sensitivity : IO UInt32 := do
  let variants : List (String × (Report → Observation)) :=
    [ ("identity_before_summary", fun report =>
        if report.artifact == .jsonl && !report.identitiesMatch then reject .identity else admit plan report)
    , ("usability_before_project_resolution", fun report =>
        if decoded report && !usable report then reject .unusable else admit plan report)
    , ("project_equality_before_usability", fun report =>
        if decoded report && report.project == .other then reject .projectMismatch else admit plan report)
    , ("trust_stable_id", fun report =>
        admit plan { report with candidates := report.candidates.map fun row => { row with originalMatches := true } })
    , ("reject_report_only_candidates", fun report =>
        let actual := admit plan report
        match actual.outcome with
        | .admitted ids => if ids.any (· != 0) then reject .descriptor else actual
        | _ => actual)
    , ("require_plan_candidate_in_report", fun report =>
        let actual := admit plan report
        match actual.outcome with
        | .admitted ids => if ids.contains 0 then actual else reject .descriptor
        | _ => actual)
    , ("continue_after_descriptor_conflict", fun report =>
        let actual := admit plan report
        if actual.outcome = .rejected .descriptor then
          { actual with planProbes := report.candidates.length }
        else actual)
    , ("sort_admitted_report_candidates", fun report =>
        let actual := admit plan report
        match actual.outcome with
        | .admitted ids => { actual with outcome := .admitted (ids.mergeSort (fun a b => a ≤ b)) }
        | _ => actual)
    ]
  for (name, variant) in variants do
    if let some (id, _) := cases.find? (fun (_, report) => variant report != admit plan report) then
      IO.println s!"review report admission sensitivity: {name}: {id}"
    else
      IO.eprintln s!"review report admission missed variant: {name}"
      return 1
  IO.println s!"review_report_admission_cases={cases.length}"
  return 0

end HoiminOracle.ReviewReportAdmission
