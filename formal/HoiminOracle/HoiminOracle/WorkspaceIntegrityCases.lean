import HoiminOracle.WorkspaceIntegrityModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceIntegrity

private def initial : Tree :=
  [ ⟨"src/calc.py", .file "x = 1 + 2\n"⟩
  , ⟨"a.txt", .file "aaaa\n"⟩
  , ⟨"z.txt", .file "zzzz\n"⟩
  , ⟨"pkg.py", .file "top\n"⟩
  , ⟨"pkg/z.txt", .file "nested\n"⟩
  , ⟨"old-empty", .directory⟩
  , ⟨"ignored.txt", .file "ignored\n"⟩
  , ⟨"hidden-fixture/side.txt", .file "hidden\n"⟩
  ]

private structure Case where
  id : String
  input : Run
  live : Bool := false

private def comparisons : List Case :=
  let first : List (List Edit) :=
    [ [], [.write "a.txt" "bbbb\n"], [.remove "z.txt"], [.write "z.txt" "longer\n"]
    , [.makeDirectory "new-empty"], [.write "ignored.txt" "different\n"]
    , [.makeDirectory "hidden-fixture/child"] ]
  let second : List (List Edit) :=
    [ [], [.write "0-added.txt" "new\n"], [.write "pkg/z.txt" "updated\n"], [.remove "old-empty"]
    , [.write "hidden-fixture/side.txt" "modified\n"], [.makeDirectory "pkg/nested"] ]
  first.zipIdx.flatMap fun (left, a) => second.zipIdx.map fun (right, b) =>
    ⟨s!"compare-{a}-{b}", { postCopy := left ++ right }, false⟩

private def liveCases : List Case :=
  [ ⟨"run-clean", {}, true⟩
  , ⟨"run-excluded",
      { baseline := [.write "ignored.txt" "changed\n"]
        mutant := [.makeDirectory "hidden-fixture/new"] }, true⟩
  , ⟨"run-baseline-change", { baseline := [.write "a.txt" "bbbb\n"] }, true⟩
  , ⟨"run-baseline-failure", { baselineExit := 7 }, true⟩
  , ⟨"run-failed-baseline-and-change", { baselineExit := 7, baseline := [.remove "a.txt"] }, true⟩
  , ⟨"run-mutant-change", { mutant := [.write "0-added.txt" "added\n"] }, true⟩
  , ⟨"run-killed-with-change", { mutantExit := 1, mutant := [.write "a.txt" "bbbb\n"] }, true⟩
  , ⟨"run-killed-clean", { mutantExit := 1 }, true⟩
  , ⟨"run-restored-between-checks",
      { baseline := [.write "a.txt" "bbbb\n"]
        mutant := [.write "a.txt" "aaaa\n"] }, true⟩
  , ⟨"run-replaced-with-same-bytes", { mutant := [.remove "a.txt", .write "a.txt" "aaaa\n"] }, true⟩
  , ⟨"run-directory-precedes-file",
      { baseline := [.write "a.txt" "bbbb\n"]
        mutant := [.makeDirectory "new-empty"] }, true⟩
  , ⟨"run-saved-file-precedes-addition",
      { baseline := [.write "z.txt" "same\n"]
        mutant := [.write "0-added.txt" "added\n"] }, true⟩
  , ⟨"run-added-directory-ancestor", { mutant := [.write "new/sub/deep.txt" "added\n"] }, true⟩
  ]

private def cases : List Case := comparisons ++
  [ ⟨"component-order", { postCopy := [.write "pkg.py" "changed\n", .write "pkg/z.txt" "changed\n"] }, false⟩
  , ⟨"replace-file-with-directory", { postCopy := [.remove "a.txt", .makeDirectory "a.txt"] }, false⟩
  , ⟨"replace-directory-with-file", { postCopy := [.remove "old-empty", .write "old-empty" "file\n"] }, false⟩
  ] ++ liveCases

private def entryJson (entry : Entry) : Lean.Json :=
  Lean.Json.mkObj [("path", .str entry.path), ("content", match entry.kind with
    | .directory => .null
    | .file content => .str content)]

private def editJson : Edit → Lean.Json
  | .write path content => Lean.Json.mkObj [("kind", .str "write"), ("path", .str path), ("content", .str content)]
  | .makeDirectory path => Lean.Json.mkObj [("kind", .str "directory"), ("path", .str path)]
  | .remove path => Lean.Json.mkObj [("kind", .str "remove"), ("path", .str path)]

private def runJson (input : Run) : Lean.Json :=
  Lean.Json.mkObj [("post_copy", .arr (input.postCopy.map editJson).toArray),
    ("baseline", .arr (input.baseline.map editJson).toArray), ("mutant", .arr (input.mutant.map editJson).toArray),
    ("baseline_exit", Lean.toJson input.baselineExit), ("mutant_exit", Lean.toJson input.mutantExit)]

private def observation (result : Observation) : Lean.Json :=
  Lean.Json.mkObj [("baseline_exit", Lean.toJson result.baselineExit),
    ("mutant_status", Lean.toJson result.mutantStatus), ("changed", Lean.toJson result.changed),
    ("exit_code", Lean.toJson result.exitCode)]

private def finalTree (input : Run) (observation : Observation) : Tree :=
  let copied := applyEdits initial input.postCopy
  match observation.baselineExit with
  | none => copied
  | some 0 => applyEdits (applyEdits copied input.baseline) input.mutant
  | some _ => applyEdits copied input.baseline

def corpus : String :=
  String.join (cases.map fun item =>
    let result := run initial item.input
    (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("id", .str item.id), ("live", Lean.toJson item.live),
      ("initial", .arr (initial.map entryJson).toArray), ("input", runJson item.input),
      ("final", .arr ((finalTree item.input result).map entryJson).toArray), ("expected", observation result)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Run → Observation)) :=
    [ ("omit_original_check", fun input => runWith (fun _ _ => none) initial input)
    , ("prefer_new_file", fun input =>
        runWith (fun saved current => (firstNewFile saved current).orElse (fun _ => firstDifference saved current))
          initial input)
    , ("ignore_directory_changes", fun input =>
        runWith (fun saved current => firstDifference saved { current with directories := saved.directories })
          initial input)
    , ("forget_mutant_outcome_after_final_error", fun input =>
        let result := run initial input
        if result.changed.isSome then { result with mutantStatus := none } else result)
    , ("baseline_failure_overrides_final_error", fun input =>
        let result := run initial input
        if result.baselineExit.any (· != 0) then { result with exitCode := 3 } else result)
    , ("check_baseline_before_mutant", fun input =>
        let result := run initial input
        if input.baselineExit == 0 && result.baselineExit.isSome then
          let early := run initial { input with mutant := [] }
          if early.changed.isSome then { early with mutantStatus := none } else result
        else result)
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item.input ≠ run initial item.input)) then
      IO.println s!"workspace integrity sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace integrity missed variant: {name}"
      return 1
  IO.println s!"workspace_integrity_cases={cases.length} live={(cases.filter Case.live).length}"
  return 0

end HoiminOracle.WorkspaceIntegrity
