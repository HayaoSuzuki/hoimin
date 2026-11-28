import HoiminOracle.CgroupMemberModel
import Lean.Data.Json

namespace HoiminOracle.CgroupMember

private structure Case where
  pid : Nat
  opening : Option Errno
  membership : Membership
  sending : Option Errno

private def errors : List Errno := [.noSuchProcess, .permission, .unsupported, .tooManyFiles, .invalid, .io]

private def normalPids (pid : Nat) : List Case :=
  (errors.map fun error => ⟨pid, some error, .present, none⟩) ++
  (([.absent, .unreadable, .malformed, .invalidUtf8, .decimalOverflow] : List Membership).map
    fun membership => ⟨pid, none, membership, none⟩) ++
  ((none :: errors.map some).map fun error => ⟨pid, none, .present, error⟩)

private def cases : List Case :=
  [41, largestSignedPid].flatMap normalPids ++
    [⟨0, some .invalid, .present, none⟩, ⟨largestSignedPid + 1, none, .present, none⟩,
      ⟨4294967295, none, .present, none⟩]

private def errnoName : Errno → String
  | .noSuchProcess => "no_such_process"
  | .permission => "permission"
  | .unsupported => "unsupported"
  | .tooManyFiles => "too_many_files"
  | .invalid => "invalid"
  | .io => "io"

private def membershipName : Membership → String
  | .present => "present"
  | .absent => "absent"
  | .unreadable => "unreadable"
  | .malformed => "malformed"
  | .invalidUtf8 => "invalid_utf8"
  | .decimalOverflow => "decimal_overflow"

private def Case.id (item : Case) :=
  let openName := item.opening.elim "ok" errnoName
  let sendName := item.sending.elim "ok" errnoName
  s!"{item.pid}-{openName}-{membershipName item.membership}-{sendName}"

private def Case.handle (item : Case) : Nat := item.pid + 7

private def eventJson (event : Event) : Lean.Json :=
  let (kind, value) := match event with
    | .openHandle pid => ("open_handle", some pid)
    | .readMembership => ("read_membership", none)
    | .signal handle => ("signal", some handle)
    | .release handle => ("release", some handle)
  .mkObj (("kind", .str kind) :: (value.toList.map (fun number => ("value", Lean.toJson number))))

private def failureJson : Option Failure → Lean.Json
  | none => .null
  | some .pidOverflow => .mkObj [("kind", .str "pid_overflow")]
  | some .invalidMembership => .mkObj [("kind", .str "invalid_membership")]
  | some (.syscall error) => .mkObj [("kind", .str "syscall"), ("errno", .str (errnoName error))]

private def observe (item : Case) : Observation :=
  selectMember item.pid item.handle item.opening item.membership item.sending

private def optionalErrno (error : Option Errno) : Lean.Json := error.elim .null (.str ∘ errnoName)

def corpus : String := String.join (cases.map fun item =>
  let actual := observe item
  (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
    ("pid", Lean.toJson item.pid), ("handle", Lean.toJson item.handle), ("open_error", optionalErrno item.opening),
    ("membership", .str (membershipName item.membership)), ("signal_error", optionalErrno item.sending),
    ("trace", Lean.toJson (actual.trace.map eventJson)), ("failure", failureJson actual.failure)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("open_overflowing_pid", fun item =>
        if item.pid > largestSignedPid then observe { item with pid := 41 } else observe item)
    , ("signal_after_membership_lost", fun item =>
        if item.membership == .absent then observe { item with membership := .present } else observe item)
    , ("signal_numeric_pid_instead_of_acquired_handle", fun item =>
        let actual := observe item
        let trace := actual.trace.map fun event =>
          match event with | .signal _ => .signal item.pid | _ => event
        { actual with trace })
    , ("treat_invalid_membership_as_absent", fun item =>
        if item.membership == .malformed then observe { item with membership := .absent } else observe item)
    , ("fail_when_process_disappears_before_open", fun item =>
        if item.opening == some .noSuchProcess then observe { item with opening := some .invalid } else observe item)
    , ("fail_when_process_disappears_after_open", fun item =>
        if item.sending == some .noSuchProcess then observe { item with sending := some .invalid } else observe item)
    , ("read_membership_before_acquiring_handle", fun item =>
        let actual := observe item
        match actual.trace with
        | opened :: .readMembership :: tail => { actual with trace := .readMembership :: opened :: tail }
        | _ => actual)
    , ("leak_handle_on_failed_read", fun item =>
        let actual := observe item
        if item.membership == .unreadable then
          { actual with trace := actual.trace.filter (· != .release item.handle) }
        else actual) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"cgroup member sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"cgroup member missed variant: {name}"
      return 1
  IO.println s!"cgroup_member_cases={cases.length}"
  return 0

end HoiminOracle.CgroupMember
