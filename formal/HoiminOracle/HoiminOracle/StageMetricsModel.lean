import HoiminOracle.WorkerMetricsModel

namespace HoiminOracle.StageMetrics

inductive Stage
  | analysis | baseline | cleanup | copy | mutants | preflight | targets
  deriving DecidableEq, BEq

def stages : List Stage := [.analysis, .baseline, .cleanup, .copy, .mutants, .preflight, .targets]

inductive Phase
  | validate | preflight | copy | materializationVerification | baseline | baselineRestoration
  | budgetCheck | analyze | mutants | finalize | cleaning | finished
  deriving DecidableEq, BEq

inductive Signal
  | ordinary | targetsResolved | preflightCompleted | cleanupFinished
  deriving DecidableEq, BEq

structure Transition where
  atNanos : Nat
  previous : Phase
  next : Phase
  signal : Signal := .ordinary

inductive Operation
  | begin (stage : Stage)
  | finish (stage : Stage)
  deriving DecidableEq, BEq

def Phase.stage : Phase → Option Stage
  | .copy | .materializationVerification => some .copy
  | .baseline | .baselineRestoration => some .baseline
  | .analyze => some .analysis
  | .mutants => some .mutants
  | .cleaning => some .cleanup
  | _ => none

def operations (transition : Transition) : List Operation :=
  let opening := match transition.signal with
    | .targetsResolved => [.finish .targets, .begin .preflight]
    | .preflightCompleted => [.finish .preflight]
    | _ => []
  let before := transition.previous.stage
  let after := transition.next.stage
  let changing :=
    if before == after then []
    else
      let leaving := before.toList.filter fun stage =>
        !(stage == .cleanup && transition.signal == .cleanupFinished)
      leaving.map Operation.finish ++ after.toList.map Operation.begin
  let closing := if transition.signal == .cleanupFinished then [.finish .cleanup] else []
  opening ++ changing ++ closing

inductive Progress
  | absent | running (sinceNanos : Nat) | finished (elapsedMillis : Nat)
  deriving DecidableEq, BEq

inductive Warning
  | alreadyStarted (stage : Stage)
  | notStarted (stage : Stage)
  | alreadyFinished (stage : Stage)
  deriving DecidableEq, BEq

abbrev State := Stage → Progress

def initial (started : List Stage) : State :=
  fun stage => if started.contains stage then .running 0 else .absent

private def put (state : State) (stage : Stage) (progress : Progress) : State :=
  fun selected => if selected = stage then progress else state selected

def apply (state : State) (time : Nat) : Operation → Except Warning State
  | .begin stage =>
      match state stage with
      | .absent => .ok (put state stage (.running time))
      | _ => .error (.alreadyStarted stage)
  | .finish stage =>
      match state stage with
      | .absent => .error (.notStarted stage)
      | .running since => .ok (put state stage (.finished (WorkerMetrics.millis (time - since))))
      | .finished _ => .error (.alreadyFinished stage)

def perform (state : State) (time : Nat) : List Operation → State × Option Warning
  | [] => (state, none)
  | operation :: rest =>
      match apply state time operation with
      | .ok updated => perform updated time rest
      | .error warning => (state, some warning)

def published (state : State) : List (Stage × Nat) :=
  stages.filterMap fun stage => match state stage with
    | .finished elapsed => some (stage, elapsed)
    | _ => none

theorem copy_verification_continues_same_interval (time : Nat) :
    operations ⟨time, .copy, .materializationVerification, .ordinary⟩ = [] := rfl

theorem baseline_restoration_continues_same_interval (time : Nat) :
    operations ⟨time, .baseline, .baselineRestoration, .ordinary⟩ = [] := rfl

theorem cleanup_completion_has_one_finish (time : Nat) :
    operations ⟨time, .cleaning, .finalize, .cleanupFinished⟩ = [.finish .cleanup] := rfl

private theorem apply_preserves_finished (state updated : State) (time : Nat) (op : Operation)
    (stage : Stage) (elapsed : Nat) (closed : state stage = .finished elapsed)
    (accepted : apply state time op = .ok updated) : updated stage = .finished elapsed := by
  cases op with
  | begin target =>
    by_cases same : stage = target
    · subst target
      simp [apply, closed] at accepted
    · cases status : state target <;> simp_all [apply]
      rw [← accepted]
      simp [put, same, closed]
  | finish target =>
    by_cases same : stage = target
    · subst target
      simp [apply, closed] at accepted
    · cases status : state target <;> simp_all [apply]
      rw [← accepted]
      simp [put, same, closed]

theorem finished_stage_cannot_be_reopened (state : State) (time : Nat) (ops : List Operation)
    (stage : Stage) (elapsed : Nat) (closed : state stage = .finished elapsed) :
    (perform state time ops).1 stage = .finished elapsed := by
  induction ops generalizing state with
  | nil => exact closed
  | cons op rest ih =>
    cases accepted : apply state time op with
    | ok updated =>
      simpa [perform, accepted] using
        ih updated (apply_preserves_finished state updated time op stage elapsed closed accepted)
    | error warning => simpa [perform, accepted] using closed

end HoiminOracle.StageMetrics
