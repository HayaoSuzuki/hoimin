import HoiminOracle.CgroupCleanupModel
import Lean.Data.Json

namespace HoiminOracle.CgroupCleanup

private structure Case where
  start : Start
  accountingFailed : Bool
  scriptName : String
  rounds : List Round

private def populated : Round := ⟨.populated, true, true⟩
private def empty : Round := ⟨.empty, true, true⟩
private def unreadable : Round := ⟨.unreadable, false, false⟩
private def membersFail : Round := ⟨.populated, false, true⟩
private def waitFails : Round := ⟨.populated, true, false⟩

private def scripts : List (String × List Round) :=
  [("implicit_empty", []), ("explicit_empty", [empty]), ("empty_before_error", [empty, unreadable]),
    ("first_inspection_error", [unreadable]), ("one_round", [populated]),
    ("two_rounds", [populated, populated, empty]), ("member_scan_error", [membersFail, populated]),
    ("wait_error", [waitFails, unreadable]),
    ("scan_and_wait_errors", [⟨.populated, false, false⟩, unreadable]),
    ("later_scan_error", [populated, membersFail, empty]), ("later_wait_error", [populated, waitFails]),
    ("later_inspection_error", [populated, unreadable, empty])]

private def cases : List Case :=
  [false, true].flatMap fun kernel =>
    [false, true].flatMap fun live =>
      [false, true].flatMap fun group =>
        [false, true].flatMap fun members =>
          [false, true].flatMap fun accounting =>
            scripts.map fun (name, rounds) => ⟨⟨kernel, live, group, members⟩, accounting, name, rounds⟩

private def Case.id (item : Case) :=
  s!"{item.start.kernelOk}-{item.start.liveRoot}-{item.start.groupOk}-{item.start.membersOk}-{item.accountingFailed}-{item.scriptName}"

private def treeName : Tree → String
  | .empty => "empty"
  | .populated => "populated"
  | .unreadable => "unreadable"

private def callName : Call → String
  | .kernelKill => "kernel_kill"
  | .groupSignal => "group_signal"
  | .memberScan => "member_scan"
  | .inspect tree => "inspect_" ++ treeName tree
  | .wait => "wait"

private def strategyName : Strategy → String
  | .kernel => "kernel"
  | .liveGroup => "live_group"
  | .membership => "membership"

private def failureName : Failure → String
  | .groupSignal => "group_signal"
  | .memberScan => "member_scan"
  | .inspect => "inspect"
  | .wait => "wait"

private def observe (item : Case) : Observation := run item.start item.rounds item.accountingFailed

private def startJson (start : Start) : Lean.Json :=
  .mkObj [("kernel_ok", .bool start.kernelOk), ("live_root", .bool start.liveRoot),
    ("group_ok", .bool start.groupOk), ("members_ok", .bool start.membersOk)]

private def roundJson (round : Round) : Lean.Json :=
  .mkObj [("tree", .str (treeName round.tree)), ("members_ok", .bool round.membersOk),
    ("wait_ok", .bool round.waitOk)]

private def observeJson (observation : Observation) : Lean.Json :=
  .mkObj [("strategy", observation.strategy.elim .null (.str ∘ strategyName)),
    ("calls", Lean.toJson (observation.cleanup.calls.map callName)),
    ("cleanup_error", observation.cleanup.failure.elim .null (.str ∘ failureName)),
    ("accounting_error", .bool observation.accountingFailed)]

def corpus : String := String.join (cases.map fun item =>
  (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("start", startJson item.start), ("accounting_failed", .bool item.accountingFailed),
    ("rounds", Lean.toJson (item.rounds.map roundJson)), ("expected", observeJson (observe item))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("ignore_kernel_success", fun item =>
        observe { item with start := { item.start with kernelOk := false } })
    , ("signal_group_after_root_reap", fun item =>
        observe { item with start := { item.start with liveRoot := true } })
    , ("skip_member_retries", fun item =>
        let result := observe item
        let startup := beginCleanup item.start
        let later := result.cleanup.calls.drop startup.calls.length
        let calls := startup.calls ++ later.filter (· != .memberScan)
        { result with cleanup := { result.cleanup with calls } })
    , ("treat_population_as_empty", fun item =>
        let rounds := item.rounds.map fun round =>
          if round.tree == .populated then { round with tree := .empty } else round
        observe { item with rounds })
    , ("wait_after_inspection_fails", fun item =>
        let result := observe item
        let calls := result.cleanup.calls ++ [.wait]
        if result.cleanup.failure == some .inspect
        then { result with cleanup := { result.cleanup with calls } } else result)
    , ("wait_after_member_signal_fails", fun item =>
        let result := observe item
        let calls := result.cleanup.calls ++ [.wait]
        if result.cleanup.failure == some .memberScan
        then { result with cleanup := { result.cleanup with calls } } else result)
    , ("skip_cleanup_when_accounting_fails", fun item =>
        if item.accountingFailed then ⟨none, ⟨[], none⟩, true⟩ else observe item)
    , ("discard_cleanup_error_when_accounting_also_fails", fun item =>
        let result := observe item
        if item.accountingFailed then { result with cleanup := { result.cleanup with failure := none } }
        else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"cgroup cleanup sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"cgroup cleanup missed variant: {name}"
      return 1
  IO.println s!"cgroup_cleanup_cases={cases.length}"
  return 0

end HoiminOracle.CgroupCleanup
