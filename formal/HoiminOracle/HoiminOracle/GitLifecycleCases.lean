import HoiminOracle.GitLifecycleModel
import Lean.Data.Json

namespace HoiminOracle.GitLifecycle

private structure Fixture where
  id : String
  events : List Event

private def childName : Child → String
  | .first => "a"
  | .second => "b"

private def single : List Fixture :=
  let choices := [false, true]
  choices.flatMap fun primary => choices.flatMap fun kill => (List.range 4).flatMap fun errors =>
    choices.map fun abandon => {
      id := s!"single-{primary}-{kill}-{errors}-{abandon}"
      events := .start .first primary kill ::
        (List.range errors).map (Event.failedWait .first) ++
        [if abandon then .abandoned .first else .reaped .first]
    }

private def interleave : Nat → List Event → List Event → List (List Event)
  | 0, _, _ => []
  | _ + 1, [], rest | _ + 1, rest, [] => [rest]
  | fuel + 1, a :: as, b :: bs =>
    (interleave fuel as (b :: bs)).map (a :: ·) ++
      (interleave fuel (a :: as) bs).map (b :: ·)

private def dual : List Fixture :=
  let choices := [false, true]
  let last (child : Child) (abandon : Bool) := if abandon then Event.abandoned child else .reaped child
  choices.flatMap fun first => choices.flatMap fun second =>
    (interleave 9
      [.start .first true false, .failedWait .first 1, .failedWait .first 2, last .first first]
      [.start .second false true, .failedWait .second 3, .failedWait .second 4, last .second second]).zipIdx
      |>.map (fun (events, index) => ⟨s!"dual-{first}-{second}-{index}", events⟩)

private def fixtures : List Fixture := single ++ dual ++ [
  ⟨"two-clean-owners", [.start .first false false, .start .second false false,
    .reaped .first, .reaped .second]⟩]

private def eventJson (event : Event) : Lean.Json :=
  let fields (child : Child) (kind : String) := [("kind", .str kind), ("child", .str (childName child))]
  match event with
  | .start child primary kill => .mkObj (fields child "start" ++
    [("primary", Lean.toJson primary), ("kill_failed", Lean.toJson kill)])
  | .failedWait child code => .mkObj (fields child "failed_wait" ++ [("code", Lean.toJson code)])
  | .reaped child => .mkObj (fields child "reaped")
  | .abandoned child => .mkObj (fields child "abandoned")

private def message : Failure → String
  | .primary child => s!"target.git.lifecycle: {childName child} capture"
  | .kill child => s!"target.git.cleanup: termination failed: {childName child} kill"
  | .reap child code => s!"target.git.cleanup: reap failed: {childName child} wait {code}"
  | .missingEvidence => "target.git.lifecycle: owned Git lifecycle ended without reap/pipe proof"
  | .deadline => "target.git.cleanup: Git lifecycle exceeded shutdown budget"

private def messages (failures : List Failure) : Lean.Json :=
  if failures.isEmpty then .null else .str (String.intercalate "; " (failures.map message))

private def observations (transition : State → Event → State) (observeDrain : State → List Failure)
    (initial : State) : List Event → List Lean.Json
  | [] => []
  | event :: rest =>
    let state := transition initial event
    Lean.Json.mkObj [("event", eventJson event), ("active", Lean.toJson (activeCount state)),
      ("drain", messages (observeDrain state)), ("cleanup", messages (cleanup initial event))] ::
      observations transition observeDrain state rest

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("steps", .arr ((observations step drain {} item.events).toArray))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def releaseOnWaitFailure (state : State) (event : Event) : State :=
  match event with
  | .failedWait child _ => step (step state event) (.reaped child)
  | _ => step state event

private def recordEveryWaitFailure (state : State) (event : Event) : State :=
  match event with
  | .failedWait child code =>
    if (state.owners child).firstReap.isSome then record state [.reap child code] else step state event
  | _ => step state event

private def loseHistoryAfterRelease (state : State) (event : Event) : State :=
  let updated := step state event
  if activeCount updated = 0 then { updated with history := [] } else updated

def sensitivity : IO UInt32 := do
  let normal := observations step drain {}
  for (name, broken) in [
    ("git_lifecycle_release_on_wait_error", observations releaseOnWaitFailure drain {}),
    ("git_lifecycle_report_every_wait_error", observations recordEveryWaitFailure drain {}),
    ("git_lifecycle_clear_reported_failures", observations loseHistoryAfterRelease drain {}),
    ("git_lifecycle_hide_unproven_drop", observations (fun state event =>
      match event with
      | .abandoned child => step state (.reaped child)
      | _ => step state event) drain {}),
    ("git_lifecycle_ignore_active_owner_at_deadline", observations step State.history {})] do
    match fixtures.find? (fun item =>
      (broken item.events).map Lean.Json.compress != (normal item.events).map Lean.Json.compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git lifecycle corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitLifecycle
