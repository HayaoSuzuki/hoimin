import HoiminOracle.RunFingerprintModel
import Lean.Data.Json

namespace HoiminOracle.RunFingerprint

private def labelHash (digit : Char) : String := String.ofList (List.replicate 64 digit)

private def sampleTargets : List Target :=
  [ { path := "m.py", lines := [⟨256, 270⟩, ⟨1, 8⟩, ⟨1, 2⟩], symbols := ["z", "é", "a"] }
  , { path := "pkg/a.py", symbols := ["entry"] }
  ]

private def sample : Input :=
  { sources := [⟨"l/α.py", List.range 32⟩, ⟨"z.py", List.replicate 32 255⟩]
    targets := sampleTargets
    operators := ["z", "compare_eq_ne", "binary_add_sub"]
    argv := [.unix [112, 121], .windows [0x1234, 0xd800, 0xffff], .unix [255, 0, 128], .unix []]
    inputFiles := [⟨"p.py", labelHash 'a'⟩, ⟨"p/a.py", labelHash 'b'⟩, ⟨"p/a.py", labelHash '3'⟩]
    importRoots := ["first", "second"]
    includes := ["src/**", "!src/no.py"]
    excludes := ["build/**", "!build/special"]
    sourceRoots := [".", "lib"]
    environment := .captured ["A", "B_1"] (labelHash 'c') }

private structure Case where
  id : String
  input : Input
  changed : List Nat := []

private def cases : List Case :=
  let one := fun (id : String) (input : Input) (tag : Nat) => (⟨id, input, [tag]⟩ : Case)
  let reordered := sampleTargets.map fun target =>
    { target with lines := target.lines.reverse, symbols := target.symbols ++ target.symbols.reverse }
  let repeatedLine := sampleTargets.map fun target =>
    { target with lines := target.lines ++ target.lines.take 1 }
  [ ⟨"reference", sample, []⟩
  , ⟨"all-empty-collections", {}, [1, 2, 3, 4, 8, 9, 10, 11, 12, 13]⟩
  , ⟨"source-order-and-duplicates", { sample with sources := sample.sources.reverse ++ sample.sources }, []⟩
  , ⟨"target-order-and-canonical-inner-fields", { sample with targets := reordered.reverse ++ sampleTargets }, []⟩
  , ⟨"operator-order-and-duplicates", { sample with operators := sample.operators.reverse ++ sample.operators }, []⟩
  , ⟨"input-file-order-and-duplicates", { sample with inputFiles := sample.inputFiles.reverse ++ sample.inputFiles }, []⟩
  , ⟨"jobs-excluded", { sample with limits := { sample.limits with jobs := 4 } }, []⟩
  , ⟨"max-mutants-excluded", { sample with limits := { sample.limits with maxMutants := 120 } }, []⟩
  , ⟨"output-limit-excluded", { sample with limits := { sample.limits with maxOutput := 2 } }, []⟩
  , one "source-hash" { sample with sources := sample.sources.map fun entry =>
      { entry with hash := entry.hash.reverse } } 1
  , one "source-path" { sample with sources := sample.sources.map fun entry =>
      { entry with path := "other/" ++ entry.path } } 1
  , one "line-multiplicity" { sample with targets := repeatedLine } 2
  , one "symbol-change" { sample with targets := sampleTargets.map fun target =>
      { target with symbols := "new" :: target.symbols } } 2
  , one "operator-membership" { sample with operators := sample.operators.drop 1 } 3
  , one "argv-order" { sample with argv := sample.argv.reverse } 4
  , one "unix-to-windows" { sample with argv := .windows [112, 121] :: sample.argv.drop 1 } 4
  , one "argv-byte" { sample with argv := .unix [112, 122] :: sample.argv.drop 1 } 4
  , one "argv-no-empty-tail" { sample with argv := sample.argv.take 3 } 4
  , one "candidate-limit" { sample with limits := { sample.limits with maxCandidates := 9000 } } 5
  , one "baseline-time-nanos" { sample with limits := { sample.limits with baseline := ⟨60, 1⟩ } } 5
  , one "fixed-mutant-time" { sample with limits := { sample.limits with mutant := some ⟨121, 999999999⟩ } } 5
  , one "total-time" { sample with limits := { sample.limits with total := ⟨301, 0⟩ } } 5
  , one "memory-u64-maximum" { sample with limits := { sample.limits with maxMemory := 18446744073709551615 } } 5
  , one "copy-size" { sample with limits := { sample.limits with maxCopy := 2147483648 } } 5
  , one "workspace-size" { sample with limits := { sample.limits with maxWorkspace := 9663676416 } } 5
  , one "free-space" { sample with limits := { sample.limits with minFree := 21474836480 } } 5
  , one "process-count" { sample with limits := { sample.limits with maxProcesses := 65 } } 5
  , one "hard-resource-mode" { sample with resourceMode := .hard } 6
  , one "focused-profile" { sample with profile := .focused } 7
  , one "keep-hashes-for-same-input-path" { sample with inputFiles := sample.inputFiles.take 2 } 8
  , one "import-root-order" { sample with importRoots := sample.importRoots.reverse } 9
  , one "import-root-multiplicity" { sample with importRoots := sample.importRoots ++ ["first"] } 9
  , one "include-precedence" { sample with includes := sample.includes.reverse } 10
  , one "exclude-precedence" { sample with excludes := sample.excludes.reverse } 11
  , one "source-root-order" { sample with sourceRoots := sample.sourceRoots.reverse } 12
  , one "environment-untracked" { sample with environment := .untracked } 13
  , one "environment-digest" { sample with environment := .captured ["A", "B_1"] (labelHash 'a') } 13
  , one "environment-names" { sample with environment := .captured ["A", "C"] (labelHash 'c') } 13
  ]

private def jsonObj := Lean.Json.mkObj
private def jsonArray (convert : α → Lean.Json) (rows : List α) : Lean.Json :=
  .arr (rows.map convert).toArray

private def targetJson (item : Target) : Lean.Json :=
  jsonObj [("path", .str item.path), ("symbols", Lean.toJson item.symbols),
    ("lines", jsonArray (fun line => jsonObj [("start", Lean.toJson line.start), ("end", Lean.toJson line.endLine)])
      item.lines)]

private def argJson : Arg → Lean.Json
  | .unix bytes => jsonObj [("unix", Lean.toJson bytes)]
  | .windows units => jsonObj [("windows", Lean.toJson units)]

private def durationJson (time : Duration) : Lean.Json :=
  jsonObj [("secs", Lean.toJson time.secs), ("nanos", Lean.toJson time.nanos)]

private def limitsJson (item : Limits) : Lean.Json :=
  jsonObj ([
      ("jobs", item.jobs), ("max_mutants", item.maxMutants), ("max_candidates", item.maxCandidates),
      ("max_memory", item.maxMemory), ("max_output", item.maxOutput), ("max_copy_size", item.maxCopy),
      ("max_workspace_size", item.maxWorkspace), ("min_free_space", item.minFree), ("max_processes", item.maxProcesses)
    ].map (fun (key, count) => (key, Lean.toJson count)) ++
    [ ("baseline_timeout", durationJson item.baseline), ("total_timeout", durationJson item.total),
      ("mutant_timeout", match item.mutant with
        | none => .str "auto"
        | some time => jsonObj [("fixed", durationJson time)]) ])

private def environmentJson : Environment → Lean.Json
  | .untracked => jsonObj [("state", .str "untracked")]
  | .captured names hash =>
    jsonObj [("state", .str "captured"), ("names", Lean.toJson names), ("digest", .str hash)]

private def inputJson (input : Input) : Lean.Json :=
  jsonObj ([
      ("operators", input.operators), ("import_roots", input.importRoots),
      ("source_roots", input.sourceRoots), ("includes", input.includes), ("excludes", input.excludes)
    ].map (fun (key, items) => (key, Lean.toJson items)) ++
    [ ("sources", jsonArray (fun file => jsonObj [("path", .str file.path), ("hash", Lean.toJson file.hash)]) input.sources)
    , ("targets", jsonArray targetJson input.targets), ("test_argv", jsonArray argJson input.argv)
    , ("limits", limitsJson input.limits), ("environment", environmentJson input.environment)
    , ("resource_mode", .str (match input.resourceMode with | .hard => "hard" | .bestEffort => "best_effort"))
    , ("profile", .str (match input.profile with | .full => "full" | .focused => "focused"))
    , ("fingerprint_inputs", jsonArray (fun file => jsonObj [("path", .str file.path), ("hash", .str file.hash)])
        input.inputFiles)
    ])

private def hex (data : Bytes) : String :=
  let digit := fun n => Char.ofNat (if n < 10 then 48 + n else 87 + n)
  String.ofList (data.flatMap fun byte => [digit (byte / 16), digit (byte % 16)])

def corpus : String :=
  String.join (cases.map fun item => (jsonObj [("schema", Lean.toJson (1 : Nat)), ("id", .str item.id),
    ("input", inputJson item.input), ("preimage_hex", .str (hex (preimage item.input)))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for item in cases do
    let actual := ((sections sample).zip (sections item.input)).filterMap fun (before, after) =>
      if before.2 != after.2 then some before.1 else none
    if actual != item.changed then
      IO.eprintln s!"run fingerprint {item.id}: expected changed tags {item.changed}, found {actual}"
      return 1
  let variants : List (String × (Input → Bytes)) :=
    [ ("previous_schema_byte", fun input => 12 :: frameStream (sections input))
    , ("omit_lengths", fun input => schemaVersion :: (sections input).flatMap (fun (tag, bytes) => tag :: bytes))
    , ("omit_field_tags", fun input => schemaVersion :: (sections input).flatMap (fun (_, bytes) => sized bytes))
    , ("sort_import_roots", fun input => preimage { input with importRoots := input.importRoots.mergeSort (· ≤ ·) })
    , ("sort_includes", fun input => preimage { input with includes := input.includes.mergeSort (· ≤ ·) })
    , ("sort_excludes", fun input => preimage { input with excludes := input.excludes.mergeSort (· ≤ ·) })
    , ("sort_source_roots", fun input => preimage { input with sourceRoots := input.sourceRoots.mergeSort (· ≤ ·) })
    , ("deduplicate_line_ranges", fun input => preimage { input with
          targets := input.targets.map fun target => { target with lines := target.lines.eraseDups } })
    , ("flatten_windows_arguments", fun input => preimage { input with argv := input.argv.map fun
          | .windows units => .unix (units.map (· % 256))
          | other => other })
    , ("include_max_mutants", fun input => preimage input ++ littleEndian 8 input.limits.maxMutants)
    , ("omit_environment_digest", fun input => preimage { input with environment := match input.environment with
          | .untracked => .untracked
          | .captured names _ => .captured names "" })
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => variant item.input != preimage item.input) then
      IO.println s!"run fingerprint sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"run fingerprint missed variant: {name}"
      return 1
  IO.println s!"run_fingerprint_cases={cases.length} tagged_fields={(sections sample).length}"
  return 0

end HoiminOracle.RunFingerprint
