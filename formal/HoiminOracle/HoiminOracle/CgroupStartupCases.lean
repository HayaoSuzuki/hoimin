import HoiminOracle.CgroupStartupModel
import Lean.Data.Json

namespace HoiminOracle.CgroupStartup

private inductive Case where
  | prepare (closed : Bool) (fault : PrepareFault)
  | attach (pid : Pid) (registered : Bool) (timing : Timing) (fault : AttachFault)

private def cases : List Case :=
  ([false, true].flatMap fun closed =>
    ([.none, .validate, .accounting, .create, .memory, .pids, .oom, .initialCounters] : List PrepareFault).map
      (Case.prepare closed)) ++
  (([.missing, .overflow, .valid] : List Pid).flatMap fun pid =>
    [false, true].flatMap fun registered =>
      ([.openRun, .alreadyClosed, .closeDuringWait] : List Timing).flatMap fun timing =>
        ([.none, .wait, .accounting, .writeMembership, .resume] : List AttachFault).map
          (Case.attach pid registered timing))

private def observe : Case → Observation
  | .prepare closed fault => prepare closed fault
  | .attach pid registered timing fault => attach pid registered timing fault

private def fileName : FileStage → String
  | .missing => "missing"
  | .directory => "directory"
  | .memory => "memory"
  | .pids => "pids"
  | .oom => "oom"

private def registrationName : Registration → String
  | .absent => "absent"
  | .inactive => "inactive"
  | .active => "active"

private def callName : Call → String
  | .validate => "validate"
  | .create => "create"
  | .memory => "memory"
  | .pids => "pids"
  | .oom => "oom"
  | .wait => "wait"
  | .writeMembership => "write_membership"
  | .resume => "resume"

private def failureName : Failure → String
  | .closed => "closed"
  | .accounting => "accounting"
  | .initialCounters => "initial_counters"
  | .missingPid => "missing_pid"
  | .pidOverflow => "pid_overflow"
  | .missingRoot => "missing_root"
  | .validate => callName .validate
  | .create => callName .create
  | .memory => callName .memory
  | .pids => callName .pids
  | .oom => callName .oom
  | .wait => callName .wait
  | .writeMembership => callName .writeMembership
  | .resume => callName .resume

private def prepareFaultName : PrepareFault → String
  | .none => "none"
  | .validate => failureName .validate
  | .accounting => failureName .accounting
  | .create => failureName .create
  | .memory => failureName .memory
  | .pids => failureName .pids
  | .oom => failureName .oom
  | .initialCounters => failureName .initialCounters

private def attachFaultName : AttachFault → String
  | .none => "none"
  | .wait => failureName .wait
  | .accounting => failureName .accounting
  | .writeMembership => failureName .writeMembership
  | .resume => failureName .resume

private def timingName : Timing → String
  | .openRun => "open_run"
  | .alreadyClosed => "already_closed"
  | .closeDuringWait => "close_during_wait"

private def pidValue : Pid → Option Nat
  | .missing => none
  | .overflow => some 2147483648
  | .valid => some 41

private def actionJson : Case → Lean.Json
  | .prepare closed fault => .mkObj [("kind", .str "prepare"), ("closed", .bool closed),
    ("fault", .str (prepareFaultName fault))]
  | .attach pid registered timing fault => .mkObj [("kind", .str "attach"),
    ("pid", Lean.toJson (pidValue pid)), ("registered", .bool registered),
    ("timing", .str (timingName timing)), ("fault", .str (attachFaultName fault))]

private def stateJson (state : State) : Lean.Json :=
  .mkObj [("files", .str (fileName state.files)), ("registration", .str (registrationName state.registration)),
    ("member_written", .bool state.memberWritten), ("closed", .bool state.closed), ("cleaned", .bool state.cleaned)]

def corpus : String := String.join (cases.zipIdx.map fun (item, index) =>
  let outcome := observe item
  (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str s!"startup-{index}"), ("action", actionJson item),
    ("after", stateJson outcome.after), ("calls", Lean.toJson (outcome.calls.map callName)),
    ("failure", outcome.failure.elim .null (.str ∘ failureName)),
    ("cleanup_calls", Lean.toJson (cleanupCalls outcome.after))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("prepare_after_close", fun item =>
        match item with | .prepare _ fault => prepare false fault | _ => observe item)
    , ("publish_before_reading_initial_counters", fun item =>
        let result := observe item
        if result.failure == some .initialCounters then
          { result with after := { result.after with registration := .inactive } }
        else result)
    , ("skip_stop_wait_if_closed", fun item =>
        let result := observe item
        if result.failure == some .closed then { result with calls := [] } else result)
    , ("ignore_close_while_waiting", fun item =>
        match item with
        | .attach pid registered .closeDuringWait fault => attach pid registered .openRun fault
        | _ => observe item)
    , ("resume_before_writing_membership", fun item =>
        let result := observe item
        if result.calls.contains .resume then { result with calls := [.wait, .resume, .writeMembership] }
        else result)
    , ("allow_resume_for_unregistered_root", fun item =>
        match item with
        | .attach pid false timing fault => attach pid true timing fault
        | _ => observe item)
    , ("forget_registration_after_failed_resume", fun item =>
        let result := observe item
        if result.failure == some .resume then
          { result with after := { result.after with registration := .absent } }
        else result) ]
  for (name, variant) in variants do
    if cases.any (fun item => decide (variant item ≠ observe item))
    then IO.println s!"cgroup startup sensitivity: {name}"
    else
      IO.eprintln s!"cgroup startup missed variant: {name}"
      return 1
  IO.println s!"cgroup_startup_cases={cases.length}"
  return 0

end HoiminOracle.CgroupStartup
