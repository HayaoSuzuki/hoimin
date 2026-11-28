import HoiminOracle.SessionArtifactsModel
import Lean.Data.Json

namespace HoiminOracle.SessionArtifacts

open HoiminOracle.WorkspaceSelection

private def root : Path := ["project"]
private def sibling : Path := ["project-backup"]
private def dir (path : Path) : Entry := ⟨path, .directory⟩
private def file (path : Path) (content : String) : Entry := ⟨path, .file content⟩
private def pathName (path : Path) := String.intercalate "/" path
private def link (path target : Path) : Entry := ⟨path, .link (pathName target)⟩

private def layout : List Entry :=
  [ dir root, dir sibling, dir (root ++ ["nested"])
  , file (root ++ ["calc.py"]) "value = True\n", file (root ++ ["ordinary.db"]) "ordinary database"
  , file (root ++ ["ordinary.db-wal"]) "ordinary companion", file (root ++ [".gitignore"]) "ignored.txt\n"
  , file (root ++ ["ignored.txt"]) "available through explicit include" ]

private def artifactFiles (db : DatabasePath) : List Entry :=
  db.files.map fun path => file path ("session artifact " ++ pathName path)

private def lockContents (path : Path) : List Entry :=
  [dir path, dir (path ++ ["sub"]), file (path ++ ["sub", "active.lock"]) "ownership record"]

private def similarlyNamed (db : DatabasePath) : List Entry :=
  let otherDb := db.parent ++ [db.name ++ "-wal-backup"]
  let otherLock := db.parent ++ ["." ++ db.name ++ ".hoimin-locks-backup"]
  [file otherDb "keep similar file", dir otherLock, file (otherLock ++ ["keep"]) "keep similar directory"]

private structure Case where
  id : String
  userPath : Option Path
  resolution : SessionResolution
  initial : List Entry := []
  afterPreparation : List Entry := []
  includeAll : Bool := true
  foldCase : Bool := false

private def placements : List (String × Path) :=
  [("inside", root), ("nested", root ++ ["nested"]), ("outside-sibling", sibling)]

private def matrix : List Case :=
  [false, true].flatMap fun foldCase =>
    let platform := if foldCase then "windows" else "unix"
    placements.flatMap fun (placement, parent) =>
      [false, true].flatMap fun includeAll =>
        let db : DatabasePath := ⟨parent, "session[1].db"⟩
        let control := similarlyNamed db
        let decoys := if parent = sibling then
          let inProject : DatabasePath := ⟨root, db.name⟩
          artifactFiles inProject ++ lockContents inProject.lexicalLock
          else []
        let walk := if includeAll then "include" else "normal"
        let name := s!"{platform}-{placement}-{walk}"
        let common : Case := {
          id := name, userPath := some db.path, resolution := .resolved db (.existing db.lexicalLock),
          foldCase, includeAll, initial := control ++ decoys }
        let late := artifactFiles db ++ lockContents db.lexicalLock
        let conflictingName := db.parent ++ [db.name ++ "-shm"]
        [ { common with id := name ++ "-existing", initial := common.initial ++ late }
        , { common with
            id := name ++ "-created-later"
            resolution := .resolved db .missing
            afterPreparation := late }
        , { common with
            id := name ++ "-type-boundaries"
            initial := common.initial ++ (artifactFiles db).filter (fun entry => entry.path ≠ conflictingName)
              ++ [dir conflictingName, file (conflictingName ++ ["keep"]) "ordinary child",
                  file db.lexicalLock "ordinary file at reserved tree name"] } ]

private def portableBoundaries (foldCase : Bool) : List Case :=
  let platform := if foldCase then "windows" else "unix"
  let db : DatabasePath := ⟨root, "Case.DB"⟩
  [ { id := platform ++ "-disabled", userPath := none, resolution := .disabled, foldCase,
      initial := artifactFiles db ++ lockContents db.lexicalLock }
  , { id := platform ++ "-companion-case", userPath := some db.path, resolution := .resolved db .missing,
      foldCase, initial := [file (root ++ ["CASE.DB-WAL"]) "alternate case companion"] }
  , { id := platform ++ "-missing-db-parent", userPath := some (root ++ ["missing", "session.db"]),
      resolution := .databaseFailed, foldCase }
  , { id := platform ++ "-database-is-directory", userPath := some db.path,
      resolution := .databaseFailed, initial := [dir db.path], foldCase } ]

private def aliases : List Case :=
  let inside : DatabasePath := ⟨root, "session.db"⟩
  let outside : DatabasePath := ⟨sibling, "session.db"⟩
  let physical := root ++ ["real-locks"]
  let external := sibling ++ ["real-locks"]
  let mk (id : String) (db : DatabasePath) (lock : LockResolution) (entries : List Entry) : Case :=
    { id, userPath := some db.path, resolution := .resolved db lock, initial := entries }
  [ mk "alias-within-project" inside (.existing physical) (lockContents physical ++ [link inside.lexicalLock physical])
  , mk "alias-outside-project" inside (.existing external) (lockContents external ++ [link inside.lexicalLock external])
  , mk "alias-enters-project" outside (.existing physical) (lockContents physical ++ [link outside.lexicalLock physical])
  , mk "alias-to-project-root" outside (.existing root) [link outside.lexicalLock root]
  , mk "missing-lock-alias-target" inside .missing [link inside.lexicalLock physical]
  , mk "loop-in-lock-path" inside .failed [link inside.lexicalLock inside.lexicalLock]
  , { id := "database-alias", userPath := some (root ++ ["alias.db"]),
      resolution := .resolved inside (.existing inside.lexicalLock),
      initial := artifactFiles inside ++ lockContents inside.lexicalLock ++
        [link (root ++ ["alias.db"]) inside.path, file (root ++ ["alias.db-wal"]) "ordinary at alias spelling"] }
  , { id := "missing-db-in-aliased-parent", userPath := some (root ++ ["alias", inside.name]),
      resolution := .resolved outside .missing, initial := [link (root ++ ["alias"]) sibling],
      afterPreparation := artifactFiles outside ++ lockContents outside.lexicalLock }
  , { id := "dangling-database", userPath := some inside.path, resolution := .databaseFailed,
      initial := [link inside.path outside.path] } ]

private def cases := matrix ++ portableBoundaries false ++ portableBoundaries true ++ aliases

private def entryJson (entry : Entry) : Lean.Json :=
  Lean.Json.mkObj <| ("path", .str (pathName entry.path)) :: match entry.kind with
  | .directory => [("kind", .str "directory")]
  | .file content => [("kind", .str "file"), ("content", .str content)]
  | .link target => [("kind", .str "link"), ("target", .str target)]

private def literalJson (literal : Literal) : Lean.Json :=
  Lean.Json.mkObj [("path", .str (pathName literal.path)),
    ("kind", .str (match literal with | .file _ => "file" | .tree _ => "tree"))]

private def snapshot (item : Case) (exclusions : List Literal) : Result :=
  let tree := (layout ++ item.initial ++ item.afterPreparation).filterMap fun entry =>
    (relativeInside false root entry.path).filter (!·.isEmpty) |>.map fun path => { entry with path }
  let rules : Rules := {
    caseInsensitive := item.foldCase
    includes := if item.includeAll then [.all] else []
    ignores := [.exact ["ignored.txt"]]
    literals := exclusions
  }
  run rules tree

private def resultJson : Result → Lean.Json
  | .invalidLiteral => Lean.Json.mkObj [("kind", .str "invalid_workspace_path")]
  | .invalidGlob => Lean.Json.mkObj [("kind", .str "invalid_glob")]
  | .manifest manifest => Lean.Json.mkObj
      [ ("kind", .str "copied"), ("directories", Lean.toJson (manifest.directories.map pathName))
      , ("files", Lean.Json.mkObj (manifest.files.map fun (path, content) => (pathName path, .str content)))
      , ("symlinks", Lean.toJson (manifest.symlinks.map pathName)) ]

private def observe (item : Case) (preparation : Option Prepared) (ignoreExclusions : Bool := false) :
    Lean.Json :=
  match preparation with
  | none => Lean.Json.mkObj [("kind", .str "session_path_error")]
  | some prepared => Lean.Json.mkObj
      [ ("kind", .str "prepared"), ("database", Lean.toJson (prepared.database.map pathName))
      , ("exclusions", .arr (prepared.exclusions.map literalJson).toArray)
      , ("copy", resultJson (snapshot item (if ignoreExclusions then [] else prepared.exclusions))) ]

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("platform", .str (if item.foldCase then "windows" else "unix"))
      , ("database", Lean.toJson (item.userPath.map pathName))
      , ("initial", .arr ((layout ++ item.initial).map entryJson).toArray)
      , ("after_preparation", .arr (item.afterPreparation.map entryJson).toArray)
      , ("include_all", Lean.toJson item.includeAll)
      , ("expected", observe item (prepare item.foldCase root item.resolution)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Lean.Json)) :=
    [ ("drop_companions", fun item => observe item
        ((prepare item.foldCase root item.resolution).map fun prepared =>
          { prepared with exclusions := prepared.exclusions.filter fun exclusion =>
              !(["-wal", "-shm", "-journal"].any fun suffix => (pathName exclusion.path).endsWith suffix) }))
    , ("drop_missing_lock", fun item => observe item ((prepare item.foldCase root item.resolution).map fun prepared =>
            match item.resolution with
            | .resolved _ .missing => { prepared with exclusions := prepared.exclusions.filter fun
                | .tree _ => false
                | .file _ => true }
            | _ => prepared))
    , ("drop_canonical_lock", fun item => observe item (prepare item.foldCase root (match item.resolution with
        | .resolved db (.existing _) => .resolved db .missing
        | other => other)))
    , ("ignore_lock_resolution_error", fun item => observe item (prepare item.foldCase root (match item.resolution with
        | .resolved db .failed => .resolved db .missing
        | other => other)))
    , ("include_sibling_exclusions", fun item =>
        let neighbor := (prepare item.foldCase sibling item.resolution).map Prepared.exclusions
        let active := (prepare item.foldCase root item.resolution).map fun prepared =>
          { prepared with exclusions := prepared.exclusions ++ neighbor.getD [] }
        observe item active)
    , ("ignore_copy_protection", fun item => observe item (prepare item.foldCase root item.resolution) true) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        (variant item).compress != (observe item (prepare item.foldCase root item.resolution)).compress) then
      IO.println s!"session artifacts sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session artifacts missed variant: {name}"
      return 1
  IO.println s!"session_artifacts_cases={cases.length} unix={(cases.filter fun c => !c.foldCase).length}"
  return 0

end HoiminOracle.SessionArtifacts
