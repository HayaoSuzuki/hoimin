import HoiminOracle.WorkspaceMutationModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceMutation

private structure Fixture where
  id : String
  snapshot : Snapshot
  request : Request

private structure Case where
  id : String
  snapshot : Snapshot
  request : Request
  file : Option File
  fault : Option Fault := none
  resize : Option Nat := none
  shared : Bool := false

private def text (s : String) : Bytes := s.toUTF8.data.toList.map UInt8.toNat

private def request (start length : Nat) (original replacement : String) (line column : Nat) : Request :=
  { start, length, original, replacement, line, column }

private def utfFixture : Fixture :=
  ⟨"utf8", ⟨text "# éx\n", .utf8, false⟩, request 2 2 "é" "顔" 1 2⟩

private def bomFixture : Fixture :=
  ⟨"bom", ⟨[239, 187, 191] ++ text "# old\n", .utf8, true⟩, request 5 3 "old" "longer" 1 2⟩

private def latinFixture : Fixture :=
  ⟨"latin1", ⟨text "# coding: latin-1\n# " ++ [233, 120, 10], .latin1, false⟩, request 20 1 "é" "àà" 2 2⟩

private def asciiFixture : Fixture :=
  ⟨"ascii", ⟨text "# coding: ascii\n# q\n", .ascii, false⟩, request 18 1 "q" "R" 2 2⟩

private def fixtures : List Fixture :=
  [ ⟨"plain", ⟨text "# abc\n", .utf8, false⟩, request 2 3 "abc" "z" 1 2⟩
  , utfFixture, bomFixture, latinFixture, asciiFixture
  , ⟨"invalid", ⟨text "x=1\n" ++ [255, 10], .invalid, false⟩, request 2 1 "1" "2" 1 2⟩ ]

private def base (fixture : Fixture) (suffix : String) : Case :=
  { id := s!"{fixture.id}-{suffix}", snapshot := fixture.snapshot, request := fixture.request,
    file := some ⟨fixture.snapshot.bytes, true⟩ }

private def matrix (fixture : Fixture) : List Case :=
  let normal := base fixture
  let req := fixture.request
  let bytes := fixture.snapshot.bytes
  let overflow := { req with start := 2^64 - 1, length := 2 }
  [ normal "read-only"
  , { normal "writable" with file := some ⟨bytes, false⟩ }
  , { normal "outside-manifest" with request := { req with tracked := false, currentHash := false } }
  , { normal "missing-open" with request := { req with currentHash := false }, file := none }
  , { normal "wrong-hash-before-span" with request := { overflow with currentHash := false } }
  , { normal "same-length-edited-before-span" with request := overflow, file := some ⟨32 :: bytes.drop 1, true⟩ }
  , { normal "short-before-reserve" with file := some ⟨bytes.take (bytes.length - 1), true⟩, fault := some .reserve }
  , { normal "overflow" with request := overflow }
  , { normal "original" with request := { req with original := "wrong" } }
  , { normal "no-op" with request := { req with replacement := req.original } }
  , { normal "location" with request := { req with line := req.line + 1 } }
  , { normal "id" with request := { req with currentId := false } }
  , { normal "empty-operator" with request := { req with operator := "", original := "wrong" } }
  , { normal "reserve" with fault := some .reserve }
  , { normal "before-repair" with fault := some .beforeRepair }
  , { normal "after-repair" with fault := some .afterRepair } ]

private def extraCases : List Case :=
  let utf := base utfFixture
  let bom := base bomFixture
  let latin := base latinFixture
  let ascii := base asciiFixture
  let u := utfFixture.request
  let l := latinFixture.request
  let a := asciiFixture.request
  let bytes := utfFixture.snapshot.bytes
  [ { latin "unrepresentable-replacement" with request := { l with replacement := "顔" } }
  , { ascii "unrepresentable-replacement" with request := { a with replacement := "é" } }
  , { latin "unrepresentable-original" with request := { l with original := "顔" } }
  , { utf "insert-split-scalar" with request := request 3 0 "" "z" 1 2 }
  , { utf "insert-at-end" with request := request bytes.length 0 "" "z\n" 2 0 }
  , { latin "delete" with request := { l with replacement := "" } }
  , { utf "shared-read-only" with shared := true }
  , { utf "shared-invalid-original" with shared := true, request := { u with original := "z" }, file := some ⟨bytes, false⟩ }
  , { utf "shared-first-links-fault" with shared := true, fault := some .beforeRepair, file := some ⟨bytes, false⟩ }
  , { utf "grows-after-budget" with resize := some (bytes.length + 5), fault := some .afterRepair, file := some ⟨bytes, false⟩ }
  , { utf "shrinks-after-budget" with resize := some 2, file := some ⟨bytes, false⟩ }
  , { utf "stable-after-budget" with resize := some bytes.length, file := some ⟨bytes, false⟩ }
  , { bom "insert-after-bom" with request := request 3 0 "" "# " 1 0 }
  , { utf "untracked-missing" with request := { u with tracked := false }, file := none }
  , { utf "shared-last-links-fault" with shared := true, fault := some .afterRepair, file := some ⟨bytes, false⟩ } ]

private def cases : List Case := fixtures.flatMap matrix ++ extraCases

private def faultName : Fault → String
  | .reserve => "reserve"
  | .beforeRepair => "before_repair"
  | .afterRepair => "after_repair"

private def outcomeName : Outcome → String
  | .applied => "applied"
  | .notSelected => "not_selected"
  | .missing => "missing"
  | .hash => "hash"
  | .original => "original"
  | .span => "span"
  | .shared => "shared"
  | .reserveFault => "reserve_fault"
  | .beforeRepairFault => "before_repair_fault"
  | .afterRepairFault => "after_repair_fault"

private def requestJson (req : Request) : Lean.Json :=
  Lean.Json.mkObj
    [ ("tracked", Lean.toJson req.tracked), ("current_hash", Lean.toJson req.currentHash), ("current_id", Lean.toJson req.currentId)
    , ("start", Lean.toJson req.start), ("length", Lean.toJson req.length), ("original", .str req.original)
    , ("replacement", .str req.replacement), ("operator", .str req.operator)
    , ("line", Lean.toJson req.line), ("column", Lean.toJson req.column) ]

private def fileJson : Option File → Lean.Json
  | none => .null
  | some file => Lean.Json.mkObj [("bytes", Lean.toJson file.bytes), ("read_only", Lean.toJson file.readOnly)]

private def resultJson (result : Result) : Lean.Json :=
  Lean.Json.mkObj [("outcome", .str (outcomeName result.outcome)), ("file", fileJson result.file),
    ("bytes_read", Lean.toJson result.bytesRead), ("fault_triggered", Lean.toJson result.faultTriggered)]

private def applyCase (item : Case) : Result :=
  run item.snapshot item.request item.file item.shared item.fault item.resize

def corpus : String :=
  String.join (cases.map fun item =>
    let first := applyCase item
    let retry := run item.snapshot item.request first.file item.shared none none
    let unixOnly := item.shared && item.file.any File.readOnly
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("snapshot", Lean.toJson item.snapshot.bytes), ("request", requestJson item.request), ("worker", fileJson item.file)
      , ("fault", Lean.toJson (item.fault.map faultName)), ("resize", Lean.toJson item.resize)
      , ("shared", Lean.toJson item.shared), ("unix_only", Lean.toJson unixOnly)
      , ("first", resultJson first), ("retry", resultJson retry) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Result)) :=
    [ ("skip_candidate_hash", fun item => applyCase { item with request := { item.request with currentHash := true } })
    , ("validate_before_worker_hash", fun item =>
        let result := applyCase item
        match result.outcome, validate item.snapshot item.request with
        | .hash, .rejected outcome => { result with outcome }
        | _, _ => result)
    , ("skip_id", fun item => applyCase { item with request := { item.request with currentId := true } })
    , ("use_utf8_for_all_replacements", fun item =>
        runWith (fun snapshot req => match validate snapshot req with
          | .accepted _ => .accepted (text req.replacement)
          | .rejected outcome => .rejected outcome)
          item.snapshot item.request item.file item.shared item.fault item.resize)
    , ("ignore_hard_links", fun item => applyCase { item with shared := false })
    , ("repair_permissions_before_validation", fun item =>
        let result := applyCase item
        if result.outcome = .span then { result with file := result.file.map fun file => { file with readOnly := false } }
        else result)
    , ("forget_permission_repair_after_final_link_fault", fun item =>
        let result := applyCase item
        if result.outcome = .afterRepairFault then { result with file := item.file } else result)
    , ("read_past_growth_probe", fun item =>
        let result := applyCase item
        if result.bytesRead.isSome && item.resize.isSome then { result with bytesRead := item.resize } else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ applyCase item)) then
      IO.println s!"workspace mutation sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace mutation missed variant: {name}"
      return 1
  IO.println s!"workspace_mutation_cases={cases.length} unix_only={(cases.filter fun item => item.shared && item.file.any File.readOnly).length}"
  return 0

end HoiminOracle.WorkspaceMutation
