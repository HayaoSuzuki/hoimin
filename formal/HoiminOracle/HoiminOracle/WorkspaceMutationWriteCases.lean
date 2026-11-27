import HoiminOracle.WorkspaceMutationWriteModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceMutationWrite

open HoiminOracle.WorkspaceMutation (Bytes File splice)

private def text (value : String) : Bytes := value.toUTF8.data.toList.map UInt8.toNat

private def source := "original\n"
private def snapshot := text source

private structure Candidate where
  start : Nat
  original : String
  replacement : String
  line : Nat

private def Candidate.mutation (candidate : Candidate) : Mutation :=
  ⟨candidate.start, (text candidate.original).length, text candidate.replacement⟩

private def equal : Candidate := ⟨0, "original", "mutated!", 1⟩
private def shorter : Candidate := ⟨0, "original", "x", 1⟩
private def erase : Candidate := ⟨0, source, "", 1⟩
private def longer : Candidate := ⟨0, "original", "replacement", 1⟩
private def append : Candidate := ⟨snapshot.length, "", "#more\n", 2⟩

private structure Case where
  id : String
  candidate : Candidate
  readOnly : Bool
  actions : List Action

private def matrix : List Case :=
  let candidates := [("equal", equal), ("shorter", shorter), ("erase", erase), ("longer", longer), ("append", append)]
  candidates.flatMap fun (name, candidate) =>
    let mutation := candidate.mutation
    let payload := splice snapshot mutation.start mutation.length mutation.replacement
    let caps := none :: (List.range (max snapshot.length payload.length + 2)).map some
    [false, true].flatMap fun readOnly => caps.map fun cap =>
      let limitName := (cap.map toString).getD "unlimited"
      ⟨s!"{name}-{readOnly}-{limitName}", candidate, readOnly,
        [.apply cap, .apply none, .reset none, .apply none]⟩

private def recoveries : List Case :=
  [ ⟨"discard-after-capped-restore", longer, true,
      [.apply (some 3), .reset (some 8), .apply none, .reset none, .create, .apply none, .reset none]⟩
  , ⟨"equal-length-prefix-mismatch", longer, false,
      [.apply (some 9), .apply none, .reset (some 9), .apply none]⟩
  , ⟨"append-prefix-matches-reset-skips-write", append, true,
      [.apply (some 9), .reset (some 0), .apply none, .reset (some 9)]⟩
  , ⟨"read-only-identical-bytes-need-restore", erase, true,
      [.reset (some 0), .create, .apply (some 0), .reset (some 0), .create]⟩
  , ⟨"matching-reset-with-zero-limit", shorter, false,
      [.reset (some 0), .create, .apply (some 1), .reset (some 9)]⟩
  , ⟨"writable-repair-restore-at-limit", equal, true,
      [.reset (some 9), .apply (some 0), .reset (some 9), .apply none]⟩ ]

private def cases := matrix ++ recoveries

private def outcomeName : Outcome → String
  | .applied => "applied"
  | .writeFailed => "write_failed"
  | .hashMismatch => "hash_mismatch"
  | .reset => "reset"
  | .restoreFailed => "restore_failed"
  | .created => "created"
  | .missing => "missing"

private def actionJson : Action → Lean.Json
  | .create => Lean.Json.mkObj [("kind", .str "create")]
  | .apply limit => Lean.Json.mkObj [("kind", .str "apply"), ("limit", Lean.toJson limit)]
  | .reset limit => Lean.Json.mkObj [("kind", .str "reset"), ("limit", Lean.toJson limit)]

private def view (registered : Bool) (state : State) : Lean.Json :=
  let generation := state.worker.map Worker.generation
  Lean.Json.mkObj
    [ ("registered", Lean.toJson (if registered then generation else none))
    , ("live_root", Lean.toJson generation), ("slots", Lean.toJson generation.toList.length)
    , ("copy_bytes", Lean.toJson (state.charge snapshot)), ("pending", Lean.toJson (0 : Nat))
    , ("file", match state.worker with
      | none => .null
      | some worker => Lean.Json.mkObj [("bytes", Lean.toJson worker.file.bytes), ("read_only", Lean.toJson worker.file.readOnly)]) ]

private def observation (state : State) (action : Action) (result : Result) : Lean.Json :=
  let admitted := match action with | .create => state.worker.isNone | _ => state.worker.isSome
  Lean.Json.mkObj
    [ ("outcome", .str (outcomeName result.outcome)), ("accepted", view true result.state)
    , ("prepared", if admitted then view false state else .null)
    , ("executed", if admitted then view false result.state else .null) ]

private def observe (mutation : Mutation) (state : State) : List Action → List Lean.Json
  | [] => []
  | action :: rest =>
    let result := step snapshot mutation state action
    observation state action result :: observe mutation result.state rest

def corpus : String :=
  String.join (cases.map fun item =>
    let candidate := item.candidate
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("snapshot", .str source), ("read_only", Lean.toJson item.readOnly)
      , ("candidate", Lean.Json.mkObj [("start", Lean.toJson candidate.start), ("line", Lean.toJson candidate.line),
          ("original", .str candidate.original), ("replacement", .str candidate.replacement)])
      , ("actions", .arr (item.actions.map actionJson).toArray)
      , ("expected", .arr (observe candidate.mutation (initial snapshot item.readOnly) item.actions).toArray) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Mutation → State → Action → Result)) :=
    [ ("pretend_write_failure_is_atomic", fun mutation state action =>
        let result := step snapshot mutation state action
        if result.outcome = .writeFailed then { result with state } else result)
    , ("drop_partial_prefix", fun mutation state action =>
        let result := step snapshot mutation state action
        if result.outcome = .writeFailed then
          { result with state := { result.state with worker := result.state.worker.map fun worker =>
              { worker with file := { worker.file with bytes := [] } } } }
        else result)
    , ("discard_worker_on_apply_failure", fun mutation state action =>
        let result := step snapshot mutation state action
        if result.outcome = .writeFailed then { result with state := { result.state with worker := none } } else result)
    , ("retry_trusts_length", fun mutation state action => match action with
        | .apply limit => applyWith (fun actual expected => actual.length == expected.length) snapshot mutation state limit
        | _ => step snapshot mutation state action)
    , ("hold_worker_after_failed_restore", fun mutation state action =>
        let result := step snapshot mutation state action
        if result.outcome = .restoreFailed then { result with state } else result)
    , ("rewrite_matching_file_on_reset", fun mutation state action => match action with
        | .reset (some cap) =>
          let result := step snapshot mutation state action
          if cap < snapshot.length && result.outcome = .reset then
            ⟨{ state with worker := none }, .restoreFailed⟩
          else result
        | _ => step snapshot mutation state action) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        let mutation := item.candidate.mutation
        let start := initial snapshot item.readOnly
        decide (history (variant mutation) start item.actions ≠ history (step snapshot mutation) start item.actions)) then
      IO.println s!"workspace mutation write sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace mutation write missed variant: {name}"
      return 1
  IO.println s!"workspace_mutation_write_cases={cases.length} transitions={(cases.map fun item => item.actions.length).sum}"
  return 0

end HoiminOracle.WorkspaceMutationWrite
