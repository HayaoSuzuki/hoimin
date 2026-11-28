import HoiminOracle.SnapshotReadModel
import Lean.Data.Json

namespace HoiminOracle.SnapshotRead

private def alphabet : List (String × Call) :=
  [ ("empty", ⟨0, .available 8⟩)
  , ("empty-error", ⟨0, .ioError⟩)
  , ("narrow", ⟨1, .available 8⟩)
  , ("short", ⟨4, .available 1⟩)
  , ("wide", ⟨8, .available 16⟩)
  , ("eof", ⟨4, .available 0⟩)
  , ("interrupted", ⟨4, .interrupted⟩)
  , ("io-error", ⟨4, .ioError⟩)
  ]

private structure Case where
  id : String
  length : Nat
  calls : List Call

private def cases : List Case :=
  let matrix := [0, 1, 4, 5, 2^64 - 1].flatMap fun length =>
    let singles := alphabet.map fun (name, call) => ⟨s!"{length}-{name}", length, [call]⟩
    let pairs := alphabet.flatMap fun (first, a) => alphabet.map fun (second, b) =>
      ⟨s!"{length}-{first}-then-{second}", length, [a, b]⟩
    singles ++ pairs
  matrix ++
    [ ⟨"finish-after-short-and-interrupted", 5,
        [⟨4, .available 1⟩, ⟨4, .interrupted⟩, ⟨4, .available 4⟩, ⟨4, .available 0⟩]⟩
    , ⟨"retry-interrupted-probe", 4,
        [⟨4, .available 4⟩, ⟨8, .interrupted⟩, ⟨8, .available 0⟩]⟩
    , ⟨"short-read-then-shrink", 4,
        [⟨4, .available 1⟩, ⟨0, .available 0⟩, ⟨4, .available 0⟩]⟩
    , ⟨"grow-after-empty-request-at-boundary", 4,
        [⟨8, .available 4⟩, ⟨0, .available 16⟩, ⟨8, .available 16⟩]⟩
    ]

private def trace (step : Nat → Call → Observation) (remaining : Nat) :
    List Call → List Observation
  | [] => []
  | call :: rest =>
    let observation := step remaining call
    observation :: trace step observation.remaining rest

private def callJson (call : Call) : Lean.Json :=
  let (kind, bytes) := match call.reply with
    | .available bytes => ("available", some bytes)
    | .interrupted => ("interrupted", none)
    | .ioError => ("io_error", none)
  Lean.Json.mkObj [
    ("capacity", Lean.toJson call.capacity),
    ("reply", Lean.Json.mkObj [("kind", .str kind), ("bytes", Lean.toJson bytes)])]

private def outcomeName : Outcome → String
  | .emptyRequest => "empty_request"
  | .data _ => "data"
  | .eof => "eof"
  | .grew => "grew"
  | .shrank => "shrank"
  | .interrupted => "interrupted"
  | .ioError => "io_error"

private def observationJson (observation : Observation) : Lean.Json :=
  Lean.Json.mkObj [
    ("requested", Lean.toJson observation.requested),
    ("consumed", Lean.toJson observation.consumed),
    ("remaining", Lean.toJson observation.remaining),
    ("outcome", .str (outcomeName observation.outcome)),
    ("delivered", Lean.toJson observation.outcome.delivered)]

def corpus : String :=
  String.join (cases.map fun item => (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)),
    ("mode", .str "internal-fixture"),
    ("id", .str item.id),
    ("length", Lean.toJson item.length),
    ("calls", .arr (item.calls.toArray.map callJson)),
    ("expected", .arr ((trace read item.length item.calls).toArray.map observationJson))
  ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Nat → Call → Observation)) :=
    [ ("empty_request_reads", readWith fun remaining capacity => requested remaining (max 1 capacity))
    , ("request_ignores_remaining_budget", readWith fun remaining capacity =>
        if remaining = 0 then requested remaining capacity
        else if capacity = 0 then none else some capacity)
    , ("probe_reads_full_buffer", readWith fun remaining capacity =>
        if remaining = 0 && capacity != 0 then some capacity else requested remaining capacity)
    , ("probe_skipped", fun remaining call =>
        if remaining = 0 && call.capacity != 0 then ⟨none, 0, 0, .eof⟩ else read remaining call)
    , ("premature_eof_accepted", fun remaining call =>
        let obs := read remaining call
        if obs.outcome == .shrank then { obs with outcome := .eof } else obs)
    , ("short_read_spends_requested_count", fun remaining call =>
        let obs := read remaining call
        match obs.outcome with
        | .data _ => { obs with remaining := remaining - obs.requested.getD 0 }
        | _ => obs)
    , ("failed_read_spends_budget", fun remaining call =>
        let obs := read remaining call
        if obs.outcome == .ioError || obs.outcome == .interrupted then
          { obs with remaining := remaining - 1 }
        else obs)
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        trace variant item.length item.calls != trace read item.length item.calls) then
      IO.println s!"snapshot read sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"snapshot read missed variant: {name}"
      return 1
  IO.println s!"snapshot_read_cases={cases.length}"
  return 0

end HoiminOracle.SnapshotRead
