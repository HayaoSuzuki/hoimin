import HoiminOracle.SessionWalRetryModel
import Lean.Data.Json

namespace HoiminOracle.SessionWalRetry

private def Fault.name : Fault → String
  | .busy => "busy"
  | .busyRecovery => "busy_recovery"
  | .locked => "locked"
  | .lockedSharedCache => "locked_shared_cache"
  | .io => "io"
  | .invalidQuery => "invalid_query"

private def steady (reply : Reply) : Attempt := ⟨reply, 0, 0, 0⟩
private def enabled : Attempt := steady (.mode "WaL")
private def busy : Attempt := steady (.fault .busy)
private def locked : Attempt := steady (.fault .locked)

private structure Case where
  id : String
  timeout : Nat
  attempts : List Attempt

private def terminalMatrix : List Case :=
  let faults : List Fault := [.busy, .busyRecovery, .locked, .lockedSharedCache, .io, .invalidQuery]
  let replies := (faults.map fun fault => (fault.name, Reply.fault fault)) ++
    (["WaL", "MeMoRy", "delete", "truncate", " wal", "WÄL", ""].map fun name => (name, Reply.mode name))
  replies.flatMap fun (name, reply) =>
    [0, 4, 5, 6].map fun timeout =>
      ⟨s!"{name}-probe-finishes-at-5-timeout-{timeout}", timeout, [⟨reply, 5, 0, 0⟩, enabled]⟩

private def timedMatrix : List Case :=
  [1, 4, 5, 6, 9, 10, 11].flatMap fun timeout =>
    [0, 2].flatMap fun clockAdvance =>
      [0, 3].map fun oversleep =>
        ⟨s!"delay-timeout-{timeout}-advance-{clockAdvance}-oversleep-{oversleep}", timeout,
          [⟨.fault .busyRecovery, 1, clockAdvance, oversleep⟩, locked, busy, enabled]⟩

private def boundaries : List Case :=
  [ ⟨"check-and-sleep-at-different-instants", 3, [⟨.fault .busy, 2, 1, 0⟩, enabled]⟩
  , ⟨"clock-already-past-deadline-when-sleep-is-calculated", 3, [⟨.fault .busy, 2, 3, 0⟩, locked, enabled]⟩
  , ⟨"last-contention-code-is-returned", 9, [busy, locked, steady (.fault .busyRecovery), enabled]⟩
  , ⟨"permanent-error-after-eligible-contention", 50, [busy, locked, steady (.fault .io), enabled]⟩
  , ⟨"sqlite-refuses-after-eligible-contention", 50, [busy, steady (.mode "DELETE"), enabled]⟩ ]

private def cases := terminalMatrix ++ timedMatrix ++ boundaries

private def replyJson : Reply → Lean.Json
  | .mode name => Lean.Json.mkObj [("kind", .str "mode"), ("name", .str name)]
  | .fault fault => Lean.Json.mkObj [("kind", .str "fault"), ("fault", .str fault.name)]

private def attemptJson (attempt : Attempt) : Lean.Json :=
  Lean.Json.mkObj
    [ ("reply", replyJson attempt.reply), ("query_time", Lean.toJson attempt.queryTime)
    , ("clock_advance", Lean.toJson attempt.clockAdvance), ("oversleep", Lean.toJson attempt.oversleep) ]

private def outcomeJson : Option Outcome → Lean.Json
  | none => .null
  | some .enabled => Lean.Json.mkObj [("kind", .str "enabled")]
  | some (.refused mode) => Lean.Json.mkObj [("kind", .str "refused"), ("mode", .str mode)]
  | some (.failed fault) => Lean.Json.mkObj [("kind", .str "failed"), ("fault", .str fault.name)]

private def observeJson (observation : Observation) : Lean.Json :=
  Lean.Json.mkObj
    [ ("outcome", outcomeJson observation.outcome), ("query_starts", Lean.toJson observation.queryStarts)
    , ("sleeps", Lean.toJson observation.sleeps) ]

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("timeout", Lean.toJson item.timeout), ("attempts", .arr (item.attempts.map attemptJson).toArray)
      , ("expected", observeJson (run item.timeout 0 item.attempts)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Nat → Nat → Nat → Reply → Decision)) :=
    [ ("allow_contention_retry_on_deadline", fun deadline observed sleepAt reply =>
        match reply with
        | .fault fault => if fault.isContention && observed == deadline then .wait 0
            else classify deadline observed sleepAt reply
        | .mode _ => classify deadline observed sleepAt reply)
    , ("ignore_sleep_cap", fun deadline observed sleepAt reply =>
        match classify deadline observed sleepAt reply with
        | .wait _ => .wait delayLimit
        | finished => finished)
    , ("reuse_old_clock_sample", fun deadline observed _ reply => classify deadline observed observed reply)
    , ("retry_permanent_errors", fun deadline observed sleepAt reply =>
        match reply with
        | .fault fault => if observed < deadline then .wait (min delayLimit (deadline - sleepAt))
            else .finish (.failed fault)
        | .mode _ => classify deadline observed sleepAt reply)
    , ("discard_extended_contention_code", fun deadline observed sleepAt reply =>
        match reply with
        | .fault .busyRecovery => .finish (.failed .busyRecovery)
        | .fault .lockedSharedCache => .finish (.failed .lockedSharedCache)
        | _ => classify deadline observed sleepAt reply)
    , ("refuse_memory", fun deadline observed sleepAt reply =>
        match reply with
        | .mode name => if name.toLower = "memory" then .finish (.refused name)
            else classify deadline observed sleepAt reply
        | .fault _ => classify deadline observed sleepAt reply) ]
  unless cases.all (fun item => (run item.timeout 0 item.attempts).outcome.isSome) do
    IO.eprintln "session WAL retry corpus contains a non-terminal trace"
    return 1
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        decide (runWith variant item.timeout 0 item.attempts ≠ run item.timeout 0 item.attempts)) then
      IO.println s!"session WAL retry sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session WAL retry missed variant: {name}"
      return 1
  IO.println s!"session_wal_retry_cases={cases.length} max_queries={(cases.map (fun c => c.attempts.length)).foldl max 0}"
  return 0

end HoiminOracle.SessionWalRetry
