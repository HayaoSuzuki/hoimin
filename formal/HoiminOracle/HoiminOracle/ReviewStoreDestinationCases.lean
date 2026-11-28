import HoiminOracle.ReviewStoreDestinationModel
import Lean.Data.Json

namespace HoiminOracle.ReviewStoreDestination

private inductive Input | plan | report | source | fingerprint
  deriving DecidableEq

private def Input.identity : Input → Nat
  | .plan => 1
  | .report => 2
  | .source => 3
  | .fingerprint => 4

private def Input.name : Input → String
  | .plan => "plan"
  | .report => "report"
  | .source => "source"
  | .fingerprint => "fingerprint"

private inductive Placement | regular | directory | hardLink (input : Input) | copyPlan | symlinkPlan

private inductive ReportPath | present | omitted | missing | directory | ioError

private structure Case where
  id : String
  placements : List (Slot × Placement) := []
  reportPath : ReportPath := .present
  sourceMissing : Bool := false
  fingerprintLink : Option Slot := none
  unixOnly : Bool := false

private def destinationId : Slot → Nat
  | .database => 10
  | .journal => 11
  | .wal => 12
  | .shm => 13

private def destination (item : Case) (slot : Slot) : Entry :=
  match item.placements.find? (fun (key, _) => key == slot) with
  | none => .missing
  | some (_, .directory) | some (_, .symlinkPlan) => .nonregular
  | some (_, .hardLink input) => .file input.identity
  | some (_, .regular) | some (_, .copyPlan) => .file (destinationId slot)

private def inputs (item : Case) : List Entry :=
  let report := match item.reportPath with
    | .present => [.file Input.report.identity]
    | .omitted => []
    | .missing => [.missing]
    | .directory => [.nonregular]
    | .ioError => [.ioError]
  let source := if item.sourceMissing then Entry.missing else .file Input.source.identity
  let fingerprint := item.fingerprintLink.elim (.file Input.fingerprint.identity) (destination item)
  [.file Input.plan.identity] ++ report ++ [source, fingerprint]

private def slotName (slot : Slot) : String :=
  if slot == .database then "database" else slot.suffix.drop 1 |>.toString

private def cases : List Case :=
  let singles := order.flatMap fun slot =>
    let name := slotName slot
    let plain := [("regular", Placement.regular), ("directory", .directory), ("copy", .copyPlan)].map
      fun (kind, placement) => { id := s!"{name}-{kind}", placements := [(slot, placement)] }
    let linked := ([.plan, .report, .source, .fingerprint] : List Input).map fun input =>
      { id := s!"{name}-alias-{input.name}", placements := [(slot, .hardLink input)] }
    plain ++ linked
  let pairs := order.zipIdx.flatMap fun (earlier, i) =>
    (order.drop (i + 1)).flatMap fun later =>
      [ { id := s!"{slotName earlier}-alias-before-{slotName later}-directory",
          placements := [(earlier, .hardLink .plan), (later, .directory)] }
      , { id := s!"{slotName earlier}-directory-before-{slotName later}-alias",
          placements := [(earlier, .directory), (later, .hardLink .plan)] }
      ]
  let controls :=
    [ { id := "all-destinations-missing" }
    , { id := "all-destinations-distinct", placements := order.map fun slot => (slot, .regular) }
    , { id := "absent-report-option-skips-alias", reportPath := .omitted,
        placements := [(.database, .hardLink .report)] }
    , { id := "missing-report-skips-to-fingerprint", reportPath := .missing,
        placements := [(.shm, .hardLink .fingerprint)] }
    , { id := "missing-source-skips-to-fingerprint", sourceMissing := true,
        placements := [(.wal, .hardLink .fingerprint)] }
    , { id := "missing-source-and-report-keep-distinct-store", reportPath := .missing,
        sourceMissing := true, placements := [(.database, .regular)] }
    ]
  let unix := order.flatMap fun slot =>
    [ { id := s!"{slotName slot}-symlink-to-plan", unixOnly := true,
        placements := [(slot, .symlinkPlan)] }
    , { id := s!"fingerprint-symlink-to-{slotName slot}", unixOnly := true,
        placements := [(slot, .regular)], fingerprintLink := some slot }
    ]
  singles ++ pairs ++ controls ++ unix ++
    [ { id := "skip-directory-report-before-source-alias", unixOnly := true, reportPath := .directory,
        placements := [(.database, .hardLink .source)] }
    , { id := "report-open-error-before-source-alias", unixOnly := true, reportPath := .ioError,
        placements := [(.database, .hardLink .source)] }
    , { id := "plan-alias-before-report-open-error", unixOnly := true, reportPath := .ioError,
        placements := [(.journal, .hardLink .plan)] }
    , { id := "missing-destinations-skip-report-open-error", unixOnly := true, reportPath := .ioError }
    , { id := "report-open-error-before-later-directory", unixOnly := true, reportPath := .ioError,
        placements := [(.journal, .regular), (.wal, .directory)] }
    ]

private def placementJson : Slot × Placement → Lean.Json
  | (slot, placement) =>
    let (kind, input) := match placement with
      | .regular => ("regular", [])
      | .directory => ("directory", [])
      | .hardLink input => ("hard_link", [("input", Lean.Json.str input.name)])
      | .copyPlan => ("copy_plan", [])
      | .symlinkPlan => ("symlink_plan", [])
    Lean.Json.mkObj ([("suffix", .str slot.suffix), ("kind", .str kind)] ++ input)

private def verdict : Option (Slot × Failure) → String
  | none => "allowed"
  | some (_, .destinationKind) => "destination_kind"
  | some (_, .aliasesInput) => "aliases_input"
  | some (_, .ioError) => "io_error"

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("mode", .str "internal-fixture"),
      ("id", .str item.id),
      ("unix_only", Lean.toJson item.unixOnly),
      ("placements", .arr (item.placements.toArray.map placementJson)),
      ("report_path", .str (match item.reportPath with
        | .present => "present" | .omitted => "omitted" | .missing => "missing"
        | .directory => "directory" | .ioError => "io_error")),
      ("source_missing", Lean.toJson item.sourceMissing),
      ("fingerprint_link", Lean.toJson (item.fingerprintLink.map Slot.suffix)),
      ("expected", .str (verdict (validate (destination item) (inputs item))))
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Option (Slot × Failure))) :=
    [ ("skip_journal", fun item =>
        validate (fun slot => if slot == .journal then .missing else destination item slot) (inputs item))
    , ("skip_wal", fun item =>
        validate (fun slot => if slot == .wal then .missing else destination item slot) (inputs item))
    , ("skip_shm", fun item =>
        validate (fun slot => if slot == .shm then .missing else destination item slot) (inputs item))
    , ("reverse_destination_order", fun item =>
        validateInOrder (order.reverse.map fun slot => (slot, destination item slot)) (inputs item))
    , ("ignore_file_identity", fun item =>
        validate (destination item) ((inputs item).map fun | .file _ => .file 99 | entry => entry))
    , ("omit_fingerprint_protection", fun item =>
        validate (destination item) ((inputs item).dropLast))
    , ("ignore_protected_open_failure", fun item =>
        validate (destination item) ((inputs item).map fun | .ioError => .missing | entry => entry))
    , ("check_inputs_for_missing_destination", fun item =>
        validate (fun slot => if destination item slot == .missing then .file 99 else destination item slot)
          (inputs item))
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        verdict (variant item) != verdict (validate (destination item) (inputs item))) then
      IO.println s!"review store destination sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"review store destination missed variant: {name}"
      return 1
  IO.println s!"review_store_destination_cases={cases.length} unix_only={(cases.filter Case.unixOnly).length}"
  return 0

end HoiminOracle.ReviewStoreDestination
