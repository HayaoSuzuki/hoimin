import HoiminOracle.WorkspaceCapacityModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceCapacity

private structure ArithmeticCase where
  id : String
  logical : Nat
  workers : Nat
  limits : Limits

private def arithmeticCases : List ArithmeticCase :=
  let matrix := [0, 1, 5].flatMap fun logical => [0, 1, 3].flatMap fun workers =>
    let copy := logical * workers
    let owned := logical * (workers + 1)
    let limits : List Limits :=
      [ {}, { copy := some copy }, { copy := some (copy - 1) }, { owned := some owned }
      , { owned := some (owned + 1) }, { copy := some copy, owned := some owned }
      , { copy := some (copy - 1), owned := some (owned - 1) } ]
    limits.zipIdx.map fun (caps, index) => ⟨s!"bounds-{logical}-{workers}-{index}", logical, workers, caps⟩
  matrix ++
    [ ⟨"workers-overflow", maximumBytes / 2 + 1, 2, {}⟩
    , ⟨"owned-overflow-without-cap", maximumBytes, 1, {}⟩
    , ⟨"copy-limit-before-owned-overflow", maximumBytes, 1, { copy := some (maximumBytes - 1) }⟩
    , ⟨"owned-overflow-with-cap", maximumBytes, 1, { copy := some maximumBytes, owned := some maximumBytes }⟩
    , ⟨"last-owned-representable", maximumBytes / 3, 2, {}⟩
    , ⟨"first-owned-overflow", maximumBytes / 3 + 1, 2, {}⟩
    , ⟨"maximum-worker-count", 1, 2^32 - 1, { owned := some (2^32) }⟩
    , ⟨"maximum-worker-count-owned-overflow", 2^32, 2^32 - 1, {}⟩
    , ⟨"worker-overflow-before-zero-cap", maximumBytes, 2, { copy := some 0, owned := some 0 }⟩ ]

private def liveCases : List (String × Run) :=
  [ ("zero-workers", { workers := 0, initial := 5 })
  , ("empty", { initial := 0 })
  , ("empty-copy-zero", { initial := 0, limits := { copy := some 0, owned := some 1 } })
  , ("empty-owned-zero", { initial := 0, limits := { copy := some 0, owned := some 0 } })
  , ("copy-exact", { initial := 5, workers := 3, limits := { copy := some 15, owned := some 21 } })
  , ("copy-exact-duplicates",
      { initial := 5, duplicate := true, workers := 3, limits := { copy := some 15, owned := some 21 } })
  , ("inventory-copy-first", { initial := 5, workers := 2, limits := { copy := some 9, owned := some 15 } })
  , ("inventory-owned-exact", { initial := 5, workers := 2, limits := { copy := some 10, owned := some 15 } })
  , ("inventory-precedes-shrink", { initial := 5, limits := { copy := some 4 }, edit := some (.beforeSnapshot 3) })
  , ("before-snapshot-grow-copy",
      { initial := 4, workers := 2, limits := { copy := some 8, owned := some 12 }, edit := some (.beforeSnapshot 5) })
  , ("before-snapshot-grow-owned",
      { initial := 4, limits := { copy := some 5, owned := some 10 }, edit := some (.beforeSnapshot 5) })
  , ("before-snapshot-grow-fits",
      { initial := 4, limits := { copy := some 5, owned := some 11 }, edit := some (.beforeSnapshot 5) })
  , ("before-snapshot-shrink",
      { initial := 5, workers := 2, limits := { copy := some 10, owned := some 16 }, edit := some (.beforeSnapshot 2) })
  , ("added-crosses-copy", { initial := 4, limits := { copy := some 7 }, edit := some .addFile })
  , ("added-crosses-owned", { initial := 4, limits := { copy := some 8, owned := some 16 }, edit := some .addFile })
  , ("added-fits", { initial := 4, limits := { copy := some 8, owned := some 17 }, edit := some .addFile })
  , ("before-descriptor-copy-first-chunk",
      { initial := 1, limits := { copy := some 3 }, edit := some (.beforeDescriptor 5) })
  , ("before-descriptor-copy-second-chunk",
      { initial := 1, workers := 2, limits := { copy := some (2 * blockBytes) }
        edit := some (.beforeDescriptor (blockBytes + 1)) })
  , ("before-descriptor-owned-second-chunk",
      { initial := 1, limits := { owned := some (2 * (blockBytes + 1)) }
        edit := some (.beforeDescriptor (blockBytes + 1)) })
  , ("before-descriptor-grow-fits", { initial := 1, limits := { copy := some 5 }, edit := some (.beforeDescriptor 5) })
  , ("before-descriptor-shrink-fits", { initial := 5, limits := { owned := some 11 }, edit := some (.beforeDescriptor 1) })
  , ("after-descriptor-grow", { initial := 1, limits := { copy := some 1 }, edit := some (.afterDescriptor 5) })
  , ("after-descriptor-grow-empty", { initial := 0, edit := some (.afterDescriptor 1) })
  , ("after-descriptor-grow-two-chunks", { initial := blockBytes + 1, edit := some (.afterDescriptor (blockBytes + 2)) })
  , ("after-descriptor-shrink", { initial := 5, edit := some (.afterDescriptor 2) })
  , ("after-descriptor-shrink-empty", { initial := 1, edit := some (.afterDescriptor 0) })
  , ("after-descriptor-shrink-two-chunks", { initial := blockBytes + 1, edit := some (.afterDescriptor blockBytes) })
  , ("validation-grow", { initial := 5, limits := { copy := some 5, owned := some 11 }, edit := some (.validation 6) })
  , ("validation-shrink", { initial := 5, edit := some (.validation 3) })
  , ("validation-same-size", { initial := 5, edit := some (.validation 5) }) ]

private def limitsJson (limits : Limits) : Lean.Json :=
  Lean.Json.mkObj [("copy", Lean.toJson limits.copy), ("owned", Lean.toJson limits.owned)]

private def failureJson (failure : Failure) : Lean.Json :=
  let (kind, fields) := match failure with
    | .zeroWorkers => ("zero_workers", [])
    | .overflow => ("overflow", [])
    | .copyLimit amount limit => ("copy_limit", [("requested", Lean.toJson amount), ("limit", Lean.toJson limit)])
    | .ownedLimit amount limit => ("owned_limit", [("requested", Lean.toJson amount), ("limit", Lean.toJson limit)])
    | .grew => ("grew", [])
    | .shrank => ("shrank", [])
    | .originalChanged => ("original_changed", [])
  Lean.Json.mkObj (("kind", .str kind) :: fields)

private def admissionJson : Admission → Lean.Json
  | .admit aggregate => Lean.Json.mkObj [("kind", .str "admit"), ("aggregate", Lean.toJson aggregate)]
  | .reject failure => failureJson failure

private def editJson : Option Edit → Lean.Json
  | none => .null
  | some edit =>
    let (kind, fields) := match edit with
      | .beforeSnapshot bytes => ("before_snapshot", [("bytes", Lean.toJson bytes)])
      | .addFile => ("add_file", [])
      | .beforeDescriptor bytes => ("before_descriptor", [("bytes", Lean.toJson bytes)])
      | .afterDescriptor bytes => ("after_descriptor", [("bytes", Lean.toJson bytes)])
      | .validation bytes => ("validation", [("bytes", Lean.toJson bytes)])
    Lean.Json.mkObj (("kind", .str kind) :: fields)

private def outputJson (observation : Observation) : Lean.Json :=
  Lean.Json.mkObj
    [ ("admission", admissionJson observation.admission), ("snapshot_started", Lean.toJson observation.snapshotStarted)
    , ("writes", Lean.toJson observation.count)
    , ("written_bytes", Lean.toJson observation.writtenBytes), ("validated_bytes", Lean.toJson observation.validation)
    , ("source_bytes", Lean.toJson observation.sourceBytes), ("added", Lean.toJson observation.added) ]

def corpus : String :=
  let line (id kind : String) (fields : List (String × Lean.Json)) :=
    (Lean.Json.mkObj ([("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
      ("phase", .str kind)] ++ fields)).compress ++ "\n"
  String.join ((arithmeticCases.map fun item => line item.id "arithmetic"
    [ ("logical", Lean.toJson item.logical), ("workers", Lean.toJson item.workers), ("limits", limitsJson item.limits)
    , ("expected", admissionJson (check item.limits item.logical item.workers)) ]) ++
    (liveCases.map fun (id, input) => line id "live"
    [ ("initial", Lean.toJson input.initial), ("workers", Lean.toJson input.workers), ("limits", limitsJson input.limits)
    , ("duplicate", Lean.toJson input.duplicate), ("edit", editJson input.edit)
    , ("expected", outputJson (run input)) ]))

def sensitivity : IO UInt32 := do
  let arithmeticVariants : List (String × (ArithmeticCase → Admission)) :=
    [ ("reject_copy_equality", fun item =>
        let amount := item.logical * item.workers
        if item.limits.copy = some amount then .reject (.copyLimit amount amount)
        else check item.limits item.logical item.workers)
    , ("permit_owned_equality", fun item =>
        let result := check item.limits item.logical item.workers
        match result with
        | .reject (.ownedLimit amount limit) => if amount = limit then .admit (item.logical * item.workers) else result
        | _ => result)
    , ("owned_limit_first", fun item =>
        match check { item.limits with copy := none } item.logical item.workers with
        | .reject (.ownedLimit amount limit) => .reject (.ownedLimit amount limit)
        | _ => check item.limits item.logical item.workers)
    , ("ignore_owned_arithmetic_without_limit", fun item =>
        let result := check item.limits item.logical item.workers
        let copied := item.logical * item.workers
        if item.limits.owned.isNone && copied ≤ item.limits.copy.getD maximumBytes && copied ≤ maximumBytes then
          .admit copied else result) ]
  for (name, variant) in arithmeticVariants do
    if let some item := arithmeticCases.find? (fun item =>
        decide (variant item ≠ check item.limits item.logical item.workers)) then
      IO.println s!"workspace capacity sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace capacity missed variant: {name}"
      return 1
  let liveVariants : List (String × (Run → Observation)) :=
    [ ("skip_inventory", fun input => runWith check check input false)
    , ("skip_chunk_admission", fun input => runWith check (fun _ bytes workers => .admit (bytes * workers)) input)
    , ("double_count_explicit_include", fun input =>
        if input.duplicate then { run { input with edit := some .addFile } with added := false }
        else run input)
    , ("follow_append_beyond_descriptor", fun input => match input.edit with
        | some (.afterDescriptor bytes) => run { input with edit := some (.beforeDescriptor bytes) }
        | _ => run input)
    , ("publish_after_validation_edit", fun input =>
        let result := run input
        match result.admission, result.validation with
        | .reject .originalChanged, some old => { result with admission := .admit (old * input.workers) }
        | _, _ => result) ]
  for (name, variant) in liveVariants do
    if let some (id, _) := liveCases.find? (fun (_, input) => decide (variant input ≠ run input)) then
      IO.println s!"workspace capacity sensitivity: {name}: {id}"
    else
      IO.eprintln s!"workspace capacity missed variant: {name}"
      return 1
  IO.println s!"workspace_capacity_arithmetic={arithmeticCases.length} live={liveCases.length}"
  return 0

end HoiminOracle.WorkspaceCapacity
