import HoiminOracle.WorkspaceRestoreModel
import Lean.Data.Json

namespace HoiminOracle.WorkspaceRestore

private def saved : Snapshot :=
  { directories := ["empty", "pkg"], files := [("a.txt", ⟨"aaaa", false⟩), ("pkg/b.txt", ⟨"bbbb", true⟩)] }

private structure Case where
  id : String
  worker : List Entry
  fault : Option Failure := none
  unixOnly : Bool := false

private def extras : List Entry :=
  [⟨"added", .directory⟩, ⟨"added/sub", .directory⟩, ⟨"added/sub/extra.txt", .file ⟨"extra", false⟩⟩]

private def fileChoices (path : String) (state : FileState) : List (List Entry) :=
  [ [⟨path, .file state⟩], [], [⟨path, .directory⟩], [⟨path, .file ⟨"", state.readOnly⟩⟩]
  , [⟨path, .file ⟨"xxxx", state.readOnly⟩⟩], [⟨path, .file ⟨"longer", state.readOnly⟩⟩]
  , [⟨path, .file { state with readOnly := !state.readOnly }⟩] ]

private def bothChanged : List Entry :=
  [⟨"a.txt", .file ⟨"late", false⟩⟩, ⟨"pkg", .directory⟩, ⟨"pkg/b.txt", .file ⟨"late", false⟩⟩]

private def firstMatches : List Entry := ⟨"a.txt", .file ⟨"aaaa", false⟩⟩ :: bothChanged.drop 1

private def cases : List Case :=
  let first := fileChoices "a.txt" ⟨"aaaa", false⟩
  let second := fileChoices "pkg/b.txt" ⟨"bbbb", true⟩
  (first.zipIdx.flatMap fun (left, a) => second.zipIdx.map fun (right, b) =>
    { id := s!"files-{a}-{b}", worker := left ++ [⟨"pkg", .directory⟩] ++ right ++ extras }) ++
  [ { id := "missing-parent", worker := [⟨"a.txt", .file ⟨"aaaa", false⟩⟩] }
  , { id := "parent-is-file", worker := [⟨"pkg", .file ⟨"occupied", false⟩⟩, ⟨"empty", .file ⟨"occupied", false⟩⟩] }
  , { id := "target-is-nonempty-directory", worker := [⟨"a.txt", .directory⟩,
        ⟨"a.txt/child.txt", .file ⟨"child", false⟩⟩] }
  , { id := "links-to-file", worker := [⟨"a.txt", .link⟩, ⟨"pkg", .link⟩, ⟨"extra-link", .link⟩], unixOnly := true }
  , { id := "rewind-first", worker := bothChanged ++ extras, fault := some ⟨0, .rewind⟩ }
  , { id := "rewind-second", worker := bothChanged ++ extras, fault := some ⟨1, .rewind⟩ }
  , { id := "write-first", worker := bothChanged ++ extras, fault := some ⟨0, .write⟩ }
  , { id := "write-second", worker := bothChanged ++ extras, fault := some ⟨1, .write⟩ }
  , { id := "permissions-second", worker := bothChanged ++ extras, fault := some ⟨1, .permissions⟩ }
  , { id := "permissions-skips-matching", worker := firstMatches, fault := some ⟨0, .permissions⟩ }
  , { id := "rewind-skips-matching", worker := firstMatches, fault := some ⟨0, .rewind⟩ } ]

private def entryJson (entry : Entry) : Lean.Json :=
  Lean.Json.mkObj <| ("path", .str entry.path) :: match entry.kind with
  | .directory => [("kind", .str "directory")]
  | .link => [("kind", .str "link")]
  | .file state => [("kind", .str "file"), ("content", .str state.content), ("read_only", Lean.toJson state.readOnly)]

private def treeJson (entries : List Entry) : Lean.Json := .arr (entries.map entryJson).toArray

private def faultJson : Option Failure → Lean.Json
  | none => .null
  | some fault => Lean.Json.mkObj [("skip", Lean.toJson fault.skip),
    ("point", .str (match fault.point with
      | .rewind => "rewind"
      | .write => "write"
      | .permissions => "permissions"))]

private def observation (result : Progress) (tree : List Entry) : Lean.Json :=
  Lean.Json.mkObj [("failed", Lean.toJson result.failed), ("replaced", Lean.toJson result.replaced), ("tree", treeJson tree)]

def corpus : String :=
  let snapshot := saved.directories.map (fun path => ⟨path, .directory⟩) ++
    saved.files.map (fun (path, state) => ⟨path, .file state⟩)
  String.join (cases.map fun item =>
    let slots := fileSlots saved item.worker
    let first := restoreFiles item.fault slots
    let intermediate := inventory saved.directories slots first
    let retrySlots := fileSlots saved intermediate
    let retry := restoreFiles none retrySlots
    (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("id", .str item.id),
      ("unix_only", Lean.toJson item.unixOnly), ("snapshot", treeJson snapshot), ("worker", treeJson item.worker),
      ("fault", faultJson item.fault), ("first", observation first intermediate),
      ("retry", observation retry (inventory saved.directories retrySlots retry))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Progress)) :=
    [ ("compare_size_only", fun item => restoreFilesWith
        (fun slot => slot.worker.any fun state => state.content.length == slot.saved.content.length &&
          state.readOnly == slot.saved.readOnly) item.fault (fileSlots saved item.worker))
    , ("ignore_permissions", fun item => restoreFilesWith
        (fun slot => slot.worker.any fun state => state.content == slot.saved.content)
        item.fault (fileSlots saved item.worker))
    , ("always_rewrite", fun item => restoreFilesWith (fun _ => false) item.fault (fileSlots saved item.worker))
    , ("ignore_fault", fun item => restoreFiles none (fileSlots saved item.worker))
    , ("rewind_failure_replaces_target", fun item => restoreFiles
        (item.fault.map fun fault => if fault.point = .rewind then { fault with point := .write } else fault)
        (fileSlots saved item.worker))
    , ("write_failure_keeps_old_content", fun item => restoreFiles
        (item.fault.map fun fault => if fault.point = .write then { fault with point := .rewind } else fault)
        (fileSlots saved item.worker))
    , ("permissions_failure_leaves_empty_file", fun item => restoreFiles
        (item.fault.map fun fault => if fault.point = .permissions then { fault with point := .write } else fault)
        (fileSlots saved item.worker)) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ restoreFiles item.fault (fileSlots saved item.worker))) then
      IO.println s!"workspace restore sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"workspace restore missed variant: {name}"
      return 1
  IO.println s!"workspace_restore_cases={cases.length}"
  return 0

end HoiminOracle.WorkspaceRestore
