import HoiminOracle.ProcessGroupModel
import Lean.Data.Json

namespace HoiminOracle.ProcessGroup

private def results : List OsResult := [.ok, .absent, .denied, .ioError]
private def faults := [false, true]
private def commands : List Command :=
  [.reap] ++ results.flatMap (fun response =>
    (faults.map fun fault => .terminateLive response fault) ++
    (faults.map fun fault => .terminateReaped response fault) ++ [.refresh response])

private structure Node where
  state : State
  history : List Command

private def grow (nodes : List Node) : List Node :=
  nodes.foldl (fun seen node =>
    commands.foldl (fun seen command =>
      let next := (step node.state command).after
      if seen.any (fun other => other.state == next) then seen
      else seen ++ [⟨next, node.history ++ [command]⟩]) seen) nodes

private def reachable (attached : Bool) : List Node :=
  -- There are three authority states and two Boolean flags.
  (List.range 12).foldl (fun nodes _ => grow nodes) [⟨initial attached, []⟩]

private def closed (nodes : List Node) : Bool :=
  nodes.all fun node => commands.all fun command =>
    nodes.any fun other => other.state == (step node.state command).after

private structure Case where
  id : String
  attached : Bool
  history : List Command
  dropSignal : OsResult
  dropFailure : Bool

private def cases : List Case :=
  [false, true].flatMap fun attached =>
    (reachable attached).zipIdx.flatMap fun (node, nodeIndex) =>
      let base := s!"{attached}-{nodeIndex}"
      (commands.zipIdx.map fun (command, index) =>
        ⟨s!"{base}-step-{index}", attached, node.history ++ [command], .ok, false⟩) ++
      ((results.flatMap fun signal => faults.map fun fail => (signal, fail)).zipIdx.map
        fun ((signal, fail), index) => ⟨s!"{base}-drop-{index}", attached, node.history, signal, fail⟩)

private def authorityName : Authority → String
  | .missing => "missing"
  | .signalable => "signalable"
  | .probeOnly => "probe_only"

private def osName : OsResult → String
  | .ok => "ok"
  | .absent => "absent"
  | .denied => "denied"
  | .ioError => "io_error"

private def callJson : Option Call → Lean.Json
  | none => .null
  | some .probe => .str "probe"
  | some .signal => .str "signal"

private def replyName : Reply → String
  | .noReply => "no_reply"
  | .settled false => "not_quiescent"
  | .settled true => "quiescent"
  | .probeError => "probe_error"
  | .signalError => "signal_error"
  | .terminationError => "termination_error"

private def commandJson : Command → Lean.Json
  | .reap => .mkObj [("operation", .str "reap")]
  | .refresh probe => .mkObj [("operation", .str "refresh"), ("os", .str (osName probe))]
  | .terminateLive signal fail =>
    .mkObj [("operation", .str "terminate_live"), ("os", .str (osName signal)), ("fail", .bool fail)]
  | .terminateReaped probe fail =>
    .mkObj [("operation", .str "terminate_reaped"), ("os", .str (osName probe)), ("fail", .bool fail)]

private def stateJson (state : State) : Lean.Json :=
  .mkObj [("authority", .str (authorityName state.authority)), ("terminated", .bool state.terminated),
    ("quiescent", .bool state.quiescent)]

private def lastState (item : Case) : State :=
  item.history.foldl (fun state command => (step state command).after) (initial item.attached)

private def encode (item : Case) : Lean.Json := Id.run do
  let mut state := initial item.attached
  let mut steps : Array Lean.Json := #[]
  for command in item.history do
    let actual := step state command
    steps := steps.push (.mkObj [("command", commandJson command), ("after", stateJson actual.after),
      ("reply", .str (replyName actual.reply)), ("call", callJson actual.call)])
    state := actual.after
  let drop := step state (.terminateLive item.dropSignal item.dropFailure)
  return .mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("attached", .bool item.attached), ("steps", .arr steps),
    ("drop", .mkObj [("os", .str (osName item.dropSignal)), ("fail", .bool item.dropFailure),
      ("call", callJson drop.call)])]

def corpus : String := String.join (cases.map fun item => (encode item).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let starts := [false, true].map reachable
  if !(starts.all closed) then
    IO.eprintln "process group exploration did not reach closure"
    return 1
  let transitions := ([false, true].flatMap reachable).flatMap fun node =>
    commands.map fun command => (node.state, command)
  let variants : List (String × (State → Command → Observation)) :=
    [ ("signal_after_reap", fun state command =>
        let actual := step state command
        match command with
        | .terminateLive _ false =>
          if state.authority == .probeOnly && !state.terminated
          then { actual with call := some .signal } else actual
        | _ => actual)
    , ("rearm_after_probe_failure", fun state command =>
        let actual := step state command
        if actual.reply == .probeError then
          { actual with after := { actual.after with authority := .signalable } }
        else actual)
    , ("permission_denied_proves_absence", fun state command =>
        match command with
        | .refresh .denied => step state (.refresh .absent)
        | _ => step state command)
    , ("missing_identifier_proves_absence", fun state command =>
        let actual := step state command
        match command with
        | .refresh _ =>
          if state.authority == .missing && !state.quiescent
          then { actual with reply := .settled true } else actual
        | _ => actual)
    , ("signal_success_proves_absence", fun state command =>
        let actual := step state command
        if actual.call == some .signal && actual.reply == .settled false
        then { actual with reply := .settled true } else actual)
    , ("probe_skipped_before_injected_termination_failure", fun state command =>
        let actual := step state command
        match command with
        | .terminateReaped _ true =>
          if actual.reply == .terminationError then { actual with call := none } else actual
        | _ => actual)
    , ("signal_again_after_termination", fun state command =>
        match command with
        | .terminateLive .ok false =>
          if state.terminated then step { state with terminated := false } command else step state command
        | _ => step state command) ]
  for (name, variant) in variants do
    if transitions.any (fun (state, command) => decide (variant state command ≠ step state command))
    then IO.println s!"process group sensitivity: {name}"
    else
      IO.eprintln s!"process group missed variant: {name}"
      return 1
  let dropWouldSignal := cases.any fun item =>
    let state := lastState item
    state.authority == .probeOnly && !state.terminated && !item.dropFailure &&
      (step state (.terminateLive item.dropSignal item.dropFailure)).call == none
  if !dropWouldSignal then
    IO.eprintln "process group corpus missed disarmed drop"
    return 1
  IO.println "process group sensitivity: drop_signals_after_reap"
  IO.println s!"process_group_states={starts.map List.length}; cases={cases.length}"
  return 0

end HoiminOracle.ProcessGroup
