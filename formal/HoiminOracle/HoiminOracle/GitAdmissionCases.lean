import HoiminOracle.GitAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.GitAdmission

private structure Fixture where
  id : String
  deadline : Option Nat
  events : List Event

private def eventJson : Event → Lean.Json
  | .stopRequest => .mkObj [("kind", .str "stop_request")]
  | .stopLifecycle => .mkObj [("kind", .str "stop_lifecycle")]
  | .advance ticks => .mkObj [("kind", .str "advance"), ("ticks", Lean.toJson ticks)]
  | .trySpawn => .mkObj [("kind", .str "try_spawn")]

private def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => ([.stopRequest, .stopLifecycle, .advance 1, .trySpawn] : List Event).flatMap
      fun event => (traces n).map (event :: ·)

private def fixtures : List Fixture :=
  let sequences := (List.range 4).flatMap traces
  ([("standalone", none), ("bounded", some 2)] : List (String × Option Nat)).flatMap fun (label, deadline) =>
    sequences.zipIdx |>.map (fun (events, n) => ⟨s!"{label}-{n}", deadline, events ++ [.trySpawn]⟩)

private def stages (canSpawn : State → Bool) (state : State) : List Event → List Lean.Json
  | [] => []
  | event :: rest =>
    let result := match event with
      | .trySpawn => .str (if canSpawn state then "spawn_failure" else "cancelled")
      | _ => .null
    .mkObj [("event", eventJson event), ("outcome", result)] ::
      stages canSpawn (step state event) rest

private def fixtureJson (fixture : Fixture) : Lean.Json :=
  .mkObj [
    ("id", .str fixture.id), ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("deadline", Lean.toJson fixture.deadline),
    ("steps", .arr ((stages allowed { deadline := fixture.deadline } fixture.events).toArray))]

def corpus : String := String.join (fixtures.map fun fixture => (fixtureJson fixture).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
    ("git_admission_ignore_request_stop", fun state : State => allowed { state with requestStopped := false }),
    ("git_admission_ignore_lifecycle_stop", fun state => allowed { state with lifecycleStopped := false }),
    ("git_admission_include_deadline", fun state =>
      allowed { state with deadline := state.deadline.map (· + 1) }),
    ("git_admission_apply_run_gate_to_standalone", fun state => !state.requestStopped && allowed state)] do
    match fixtures.find? (fun item =>
      let initial : State := { deadline := item.deadline }
      (stages allowed initial item.events).map Lean.Json.compress !=
        (stages broken initial item.events).map Lean.Json.compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git admission corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitAdmission
