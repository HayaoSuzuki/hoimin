import HoiminOracle.WorkerRecoveryModel
import Lean.Data.Json

namespace HoiminOracle.WorkerRecovery

private def operationName : Operation → String
  | .reset => "reset"
  | .restoreFault => "restore_fault"
  | .discardFault => "discard_fault"
  | .depthFault => "depth_fault"
  | .create => "create"
  | .foreign => "foreign"
  | .foreignWithCleanupFault => "foreign_with_cleanup_fault"
  | .retryCleanupFault => "retry_cleanup_fault"
  | .copyFault => "copy_fault"
  | .importFault => "import_fault"

private def outcomeName : Outcome → String
  | .reset => "reset"
  | .created => "created"
  | .workerMissing => "workspace.worker.missing"
  | .restore => "workspace.restore"
  | .depth => "workspace.path.depth"
  | .foreign => "workspace.preflight.mismatch"
  | .io => "workspace.io"
  | .importRoot => "workspace.import_root.unavailable"

private def diagnosticName : Diagnostic → String
  | .restore => "restore"
  | .discard => "discard"
  | .depth => "depth"
  | .cleanup => "cleanup"
  | .copy => "copy"

private def operations (state : State) : List Operation :=
  match state.owner with
  | .active _ => [.reset, .restoreFault, .discardFault, .depthFault, .create, .foreign]
  | .absent => [.reset, .create, .foreign, .copyFault, .importFault]
  | .pending _ => [.reset, .create, .foreign, .foreignWithCleanupFault, .retryCleanupFault, .copyFault, .importFault]

private def schedulesOfLength : Nat → State → List (List Operation)
  | 0, _ => [[]]
  | depth + 1, state => (operations state).flatMap fun op =>
      (schedulesOfLength depth (step state op).state).map (op :: ·)

private def schedules : List (List Operation) :=
  (schedulesOfLength 1 initial) ++ (schedulesOfLength 2 initial) ++
  [ [.discardFault, .foreignWithCleanupFault, .retryCleanupFault, .create]
  , [.discardFault, .copyFault, .create, .reset]
  , [.depthFault, .retryCleanupFault, .create, .depthFault]
  , [.restoreFault, .importFault, .foreignWithCleanupFault, .create]
  , [.restoreFault, .copyFault, .copyFault, .create]
  ]

private def view (logicalBytes : Nat) (registered materialized : Owner) : Lean.Json :=
  let retained := materialized.generation.toList
  Lean.Json.mkObj
    [ ("active", Lean.toJson (match registered with | .active n => some n | _ => none))
    , ("pending", Lean.toJson (match registered with | .pending n => some n | _ => none))
    , ("slots", Lean.toJson materialized.slots)
    , ("bytes", Lean.toJson (materialized.copyBytes logicalBytes))
    , ("live_roots", Lean.toJson retained) ]

private def observation (logicalBytes : Nat) (before : State) (result : Result) : Lean.Json :=
  let after := result.state.owner
  Lean.Json.mkObj
    [ ("outcome", .str (outcomeName result.outcome))
    , ("diagnostics", Lean.toJson (result.diagnostics.map diagnosticName))
    , ("fault_consumed", Lean.toJson result.consumedFault)
    , ("prepared", if result.admitted then view logicalBytes .absent before.owner else .null)
    , ("executed", if result.admitted then view logicalBytes .absent after else .null)
    , ("accepted", view logicalBytes after after) ]

private def observe (logicalBytes : Nat) (state : State) : List Result → List Lean.Json
  | [] => []
  | result :: rest => observation logicalBytes state result :: observe logicalBytes result.state rest

def corpus : String :=
  let content := "original\n"
  String.join (schedules.map fun schedule =>
    let names := schedule.map operationName
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture")
      , ("id", .str (String.intercalate "/" names)), ("original", .str content)
      , ("schedule", Lean.toJson names)
      , ("expected", .arr ((observe content.utf8ByteSize initial (runWith step initial schedule)).toArray)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (State → Operation → Result)) :=
    [ ("cleanup_before_grant_validation", fun state op =>
        if op = .foreignWithCleanupFault then step state .retryCleanupFault else step state op)
    , ("release_failed_discard", fun state op =>
        if op = .discardFault then step state .restoreFault else step state op)
    , ("discard_diagnostic_first", fun state op =>
        let result := step state op
        if op = .discardFault then { result with diagnostics := result.diagnostics.reverse } else result)
    , ("discard_error_overrides_depth", fun state op =>
        if op = .depthFault then step state .discardFault else step state op)
    , ("retain_old_slot_after_copy_fault", fun state op =>
        let result := step state op
        if op = .copyFault then { result with state } else result)
    , ("publish_rejected_import_root", fun state op =>
        if op = .importFault then step state .create else step state op)
    , ("reuse_generation_on_retry", fun state op =>
        let result := step state op
        match op, state.owner, result.state.owner with
        | .create, .pending old, .active _ => { result with state := { result.state with owner := .active old } }
        | _, _, _ => result) ]
  for (name, variant) in variants do
    if let some witness := schedules.find? (fun schedule =>
        decide (runWith variant initial schedule ≠ runWith step initial schedule)) then
      IO.println s!"worker recovery sensitivity: {name}: {String.intercalate "/" (witness.map operationName)}"
    else
      IO.eprintln s!"worker recovery missed variant: {name}"
      return 1
  IO.println s!"worker_recovery_cases={schedules.length} transitions={(schedules.map List.length).sum} max_depth=4 alphabet=10"
  return 0

end HoiminOracle.WorkerRecovery
