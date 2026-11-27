import HoiminOracle.StageMetricsModel
import Lean.Data.Json

namespace HoiminOracle.StageMetrics

private structure Fixture where
  id : String
  initialStages : List Stage := [.targets]
  transitions : List Transition

private def stageName : Stage → String
  | .analysis => "analysis" | .baseline => "baseline" | .cleanup => "cleanup"
  | .copy => "copy" | .mutants => "mutants" | .preflight => "preflight" | .targets => "targets"

private def phaseName : Phase → String
  | .validate => "validate" | .preflight => "preflight" | .copy => "copy"
  | .materializationVerification => "materialization_verification" | .baseline => "baseline"
  | .baselineRestoration => "baseline_restoration" | .budgetCheck => "budget_check"
  | .analyze => "analyze" | .mutants => "mutants" | .finalize => "finalize"
  | .cleaning => "cleaning" | .finished => "finished"

private def signalName : Signal → String
  | .ordinary => "ordinary" | .targetsResolved => "targets_resolved"
  | .preflightCompleted => "preflight_completed" | .cleanupFinished => "cleanup_finished"

private def warningMessage : Warning → String
  | .alreadyStarted stage => s!"stage {stageName stage} has already started"
  | .notStarted stage => s!"stage {stageName stage} has not started"
  | .alreadyFinished stage => s!"stage {stageName stage} has already finished"

private def atMs (time : Nat) (previous next : Phase) (signal : Signal := .ordinary) :
    Transition :=
  ⟨time * WorkerMetrics.nanosPerMilli, previous, next, signal⟩

private def fixtures : List Fixture := [
  { id := "successful-with-session-and-budget",
    transitions := [atMs 2 .preflight .preflight .targetsResolved,
      atMs 5 .preflight .preflight .preflightCompleted, atMs 10 .preflight .copy,
      atMs 12 .copy .copy, atMs 13 .copy .materializationVerification,
      atMs 17 .materializationVerification .baseline, atMs 20 .baseline .baselineRestoration,
      atMs 22 .baselineRestoration .baselineRestoration, atMs 25 .baselineRestoration .budgetCheck,
      atMs 29 .budgetCheck .analyze, atMs 37 .analyze .mutants, atMs 80 .mutants .finalize,
      atMs 95 .finalize .cleaning, atMs 100 .cleaning .finalize .cleanupFinished,
      atMs 110 .finalize .finished] },
  { id := "preflight-completion-and-copy-at-same-time", initialStages := [.preflight],
    transitions := [atMs 2 .preflight .copy .preflightCompleted,
      atMs 4 .copy .materializationVerification, atMs 6 .materializationVerification .baseline] },
  { id := "stop-with-unresolved-targets",
    transitions := [atMs 4 .preflight .cleaning, atMs 9 .cleaning .finalize,
      atMs 10 .finalize .finished] },
  { id := "stop-during-preflight",
    transitions := [atMs 4 .preflight .preflight .targetsResolved, atMs 9 .preflight .cleaning,
      atMs 12 .cleaning .finalize .cleanupFinished] },
  { id := "stop-during-copy-verification", initialStages := [.copy],
    transitions := [atMs 2 .copy .materializationVerification,
      atMs 6 .materializationVerification .cleaning, atMs 8 .cleaning .finalize .cleanupFinished] },
  { id := "stop-during-baseline-restoration", initialStages := [.baseline],
    transitions := [atMs 2 .baseline .baselineRestoration, atMs 8 .baselineRestoration .cleaning,
      atMs 10 .cleaning .finished] },
  { id := "baseline-nonzero-exit", initialStages := [.baseline],
    transitions := [atMs 2 .baseline .finalize, atMs 8 .finalize .cleaning,
      atMs 10 .cleaning .finalize .cleanupFinished] },
  { id := "analysis-with-no-mutants", initialStages := [.analysis],
    transitions := [atMs 2 .analyze .finalize, atMs 4 .finalize .cleaning,
      atMs 6 .cleaning .finished] },
  { id := "fractional-stage-elapsed", initialStages := [],
    transitions := [⟨900000, .preflight, .copy, .ordinary⟩,
      ⟨1100000, .copy, .materializationVerification, .ordinary⟩,
      ⟨1800000, .materializationVerification, .baseline, .ordinary⟩,
      ⟨2800000, .baseline, .finalize, .ordinary⟩] },
  { id := "missing-targets-stops-before-starting-preflight", initialStages := [],
    transitions := [atMs 2 .preflight .preflight .targetsResolved,
      atMs 4 .preflight .preflight .preflightCompleted] },
  { id := "duplicate-preflight-retains-closed-targets", initialStages := [.targets, .preflight],
    transitions := [atMs 2 .preflight .preflight .targetsResolved,
      atMs 6 .preflight .copy .preflightCompleted] },
  { id := "missing-preflight-does-not-open-copy", initialStages := [],
    transitions := [atMs 2 .preflight .copy .preflightCompleted,
      atMs 5 .copy .baseline] },
  { id := "finished-stage-remains-closed", initialStages := [.copy],
    transitions := [atMs 3 .copy .finalize, atMs 7 .preflight .copy,
      atMs 10 .copy .baseline] }
]

private structure Observation where
  stages : List (Stage × Nat)
  warnings : List Warning
  deriving BEq

private def trace (ops : Transition → List Operation) (item : Fixture) : List Observation :=
  let rec go (state : State) (warnings : List Warning) : List Transition → List Observation
    | [] => [⟨published state, warnings⟩]
    | event :: rest =>
        let (next, warning) := perform state event.atNanos (ops event)
        ⟨published state, warnings⟩ :: go next (warnings ++ warning.toList) rest
  go (initial item.initialStages) [] item.transitions

private def stagesJson (observation : Observation) : Lean.Json := Lean.Json.mkObj [
  ("stages", .arr (observation.stages.toArray.map fun (stage, elapsed) => Lean.Json.mkObj [
    ("name", .str (stageName stage)), ("elapsed_ms", Lean.toJson elapsed)])),
  ("warnings", .arr (observation.warnings.toArray.map fun warning => Lean.Json.mkObj [
    ("code", .str "metrics.state"), ("message", .str (warningMessage warning))]))]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("initial_stages", .arr (item.initialStages.toArray.map fun stage => .str (stageName stage))),
  ("transitions", .arr (item.transitions.toArray.map fun event => Lean.Json.mkObj [
    ("at_ns", Lean.toJson event.atNanos), ("previous", .str (phaseName event.previous)),
    ("next", .str (phaseName event.next)), ("signal", .str (signalName event.signal))])),
  ("expected", .arr ((trace operations item).toArray.map stagesJson))]

def corpus : String :=
  String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private inductive PublicWork
  | killedMutants | limitedMutants | noCandidates | failedBaseline
  deriving DecidableEq, BEq

private inductive SessionPass
  | ephemeral | fresh | resume
  deriving DecidableEq, BEq

private structure PublicFixture where
  id : String
  work : PublicWork
  passes : List SessionPass := [.ephemeral]

private def publicFixtures : List PublicFixture := [
  { id := "killed-mutants", work := .killedMutants },
  { id := "no-candidates", work := .noCandidates },
  { id := "baseline-failed", work := .failedBaseline },
  { id := "completed-session-starts-fresh", work := .killedMutants, passes := [.fresh, .resume] },
  { id := "limited-session-reuses-executed-mutants",
    work := .limitedMutants, passes := [.fresh, .resume] }
]

private def publicWorkName : PublicWork → String
  | .killedMutants => "killed_mutants"
  | .limitedMutants => "limited_mutants"
  | .noCandidates => "no_candidates"
  | .failedBaseline => "failed_baseline"

private def passName : SessionPass → String
  | .ephemeral => "ephemeral"
  | .fresh => "fresh"
  | .resume => "resume"

private def publicTransitions (work : PublicWork) : List Transition :=
  let setup := [atMs 1 .preflight .preflight .targetsResolved,
    atMs 2 .preflight .preflight .preflightCompleted, atMs 3 .preflight .copy,
    atMs 4 .copy .materializationVerification, atMs 5 .materializationVerification .baseline]
  let afterBaseline := [atMs 7 .baseline .baselineRestoration,
    atMs 8 .baselineRestoration .analyze]
  let run := match work with
    | .failedBaseline => [atMs 7 .baseline .finalize]
    | .noCandidates => afterBaseline ++ [atMs 9 .analyze .finalize]
    | .killedMutants | .limitedMutants =>
        afterBaseline ++ [atMs 9 .analyze .mutants, atMs 10 .mutants .finalize]
  setup ++ run ++ [atMs 11 .finalize .cleaning, atMs 12 .cleaning .finalize .cleanupFinished,
    atMs 13 .finalize .finished]

private def discovered : PublicWork → Nat
  | .killedMutants => 2
  | .limitedMutants => 3
  | _ => 0

private def eligibleToReuse (work : PublicWork) (session : SessionPass) : Bool :=
  session == .resume && work == .limitedMutants

private def scheduled (work : PublicWork) : Nat := min (discovered work) 2

private def executed (work : PublicWork) (session : SessionPass) : Nat :=
  if eligibleToReuse work session then 0 else scheduled work

private def exitCode : PublicWork → Nat
  | .failedBaseline => 3
  | .limitedMutants => 4
  | _ => 0

private def resumeJson (work : PublicWork) (session : SessionPass) : Lean.Json :=
  if eligibleToReuse work session then Lean.Json.mkObj [("status", .str "resumed")]
  else if session == .resume then Lean.Json.mkObj [
    ("status", .str "fresh"), ("reason", .str "matching_run_complete")]
  else .null

private def publicStages (work : PublicWork) : List Stage :=
  let item : Fixture := { id := "", transitions := publicTransitions work }
  match (trace operations item).getLast? with
  | some observation => observation.stages.map Prod.fst
  | none => []

private def publicFixtureJson (item : PublicFixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("work", .str (publicWorkName item.work)),
  ("passes", .arr (item.passes.toArray.map fun session => Lean.Json.mkObj [
    ("session", .str (passName session)),
    ("expected", Lean.Json.mkObj [
      ("exit_code", Lean.toJson (exitCode item.work)),
      ("complete", Lean.toJson (exitCode item.work == 0)),
      ("stages", .arr ((publicStages item.work).toArray.map fun stage => .str (stageName stage))),
      ("discovered", Lean.toJson (discovered item.work)),
      ("executed", Lean.toJson (executed item.work session)),
      ("reported_mutants", Lean.toJson (discovered item.work)),
      ("reused_mutants", Lean.toJson (if eligibleToReuse item.work session then scheduled item.work else 0)),
      ("not_run_mutants", Lean.toJson (discovered item.work - scheduled item.work)),
      ("resume", resumeJson item.work session)])]))]

def publicCorpus : String :=
  String.join (publicFixtures.map fun item => (publicFixtureJson item).compress ++ "\n")

-- A resumed run visits the mutant stage to read stored outcomes even when all
-- process executions can be skipped.
example : (publicStages .limitedMutants).contains .mutants = true ∧
    executed .limitedMutants .resume = 0 := by decide

example : (publicStages .failedBaseline).contains .analysis = false ∧
    (publicStages .noCandidates).contains .mutants = false := by decide

private def closeDuringVerification (transition : Transition) : List Operation :=
  if transition.previous == .copy && transition.next == .materializationVerification then
    [.finish .copy]
  else operations transition

private def closeDuringRestoration (transition : Transition) : List Operation :=
  if transition.previous == .baseline && transition.next == .baselineRestoration then
    [.finish .baseline]
  else operations transition

private def deferPreflightUntilPhaseExit (transition : Transition) : List Operation :=
  if transition.signal == .preflightCompleted && transition.previous == transition.next then
    []
  else operations transition

private def closeCleanupTwice (transition : Transition) : List Operation :=
  if transition.signal == .cleanupFinished then
    .finish .cleanup :: operations transition
  else operations transition

def sensitivity : IO UInt32 := do
  for (name, candidate) in [("close_during_verification", closeDuringVerification),
      ("close_during_restoration", closeDuringRestoration),
      ("defer_preflight_until_phase_exit", deferPreflightUntilPhaseExit),
      ("close_cleanup_twice", closeCleanupTwice)] do
    match fixtures.find? (fun item => trace candidate item != trace operations item) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"stage metrics corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.StageMetrics
