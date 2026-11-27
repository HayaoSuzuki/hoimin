import HoiminOracle.ReviewStoreAdmissionModel
import Lean.Data.Json

namespace HoiminOracle.ReviewStoreAdmission

private inductive Fixture
  | sqlite | missing | directory | symlink | empty | opaque | closedWal
  | truncatedHeader | mixedReadVersion | mixedWriteVersion
  deriving DecidableEq

private inductive Shape | standard | extraTable | extraIndex | incomplete | changedSql
  deriving DecidableEq

private inductive ProjectRows | matching | wrong | empty | multiple
  deriving DecidableEq

private inductive Integrity | valid | unusedPage | invalidRevision
  deriving DecidableEq

private structure Case where
  id : String
  fixture : Fixture := .sqlite
  size : Nat := 4096
  knownApplication : Bool := true
  knownVersion : Bool := true
  shape : Shape := .standard
  project : ProjectRows := .matching
  integrity : Integrity := .valid
  pageSize : Nat := 4096
  pages : Nat := 4
  deleteMode : Bool := true
  modelOnly : Bool := false

private def toFile (item : Case) : File :=
  match item.fixture with
  | .missing => .missing
  | .directory | .symlink => .nonregular
  | .opaque => .regular item.size [110, 111] none
  | .empty => .regular 0 [] (some { knownApplication := false, knownVersion := false })
  | .closedWal => .regular item.size (sqliteMagic ++ [16, 0, 2, 2]) none
  | .truncatedHeader => .regular item.size (sqliteMagic ++ [16, 0, 1]) none
  | .mixedReadVersion => .regular item.size (sqliteMagic ++ [16, 0, 1, 2]) none
  | .mixedWriteVersion => .regular item.size (sqliteMagic ++ [16, 0, 2, 1]) none
  | .sqlite =>
    let objects := match item.shape with
      | .standard => Objects.complete
      | .incomplete => .incomplete
      | .extraTable | .extraIndex | .changedSql => .unexpected
    let schema : Schema :=
      { knownApplication := item.knownApplication
        knownVersion := item.knownVersion
        objects
        projectMatches := item.project == .matching
        deleteMode := item.deleteMode
        integrityOk := fun access =>
          match item.integrity with
          | .valid => true
          | .unusedPage => false
          | .invalidRevision => access == .read
        pageSize := item.pageSize
        pages := item.pages }
    .regular item.size (sqliteMagic ++ [16, 0, 1, 1]) (some schema)

private def shapeName : Shape → String
  | .standard => "standard"
  | .extraTable => "extra_table"
  | .extraIndex => "extra_index"
  | .incomplete => "incomplete"
  | .changedSql => "changed_sql"

private def projectName : ProjectRows → String
  | .matching => "matching"
  | .wrong => "wrong"
  | .empty => "empty"
  | .multiple => "multiple"

private def integrityName : Integrity → String
  | .valid => "valid"
  | .unusedPage => "unused_page"
  | .invalidRevision => "invalid_revision"

private def cases : List Case :=
  let shapes := [Shape.standard, .extraTable, .extraIndex, .incomplete, .changedSql]
  let projects := [ProjectRows.matching, .wrong, .empty, .multiple]
  let matrix := [true, false].flatMap fun knownApplication =>
    [true, false].flatMap fun knownVersion => shapes.flatMap fun shape => projects.map fun project =>
      { id := s!"schema-{knownApplication}-{knownVersion}-{shapeName shape}-{projectName project}"
        knownApplication, knownVersion, shape, project }
  let geometry := [4096, 8192].flatMap fun pageSize =>
    [Integrity.valid, .unusedPage, .invalidRevision].flatMap fun integrity =>
    [ProjectRows.matching, .wrong].map fun project =>
      { id := s!"geometry-{pageSize}-{integrityName integrity}-{projectName project}"
        integrity, pageSize, project }
  matrix ++ geometry ++
    [ { id := "extra-object-before-broken-check", shape := .extraTable, integrity := .unusedPage }
    , { id := "absent", fixture := .missing }
    , { id := "directory", fixture := .directory }
    , { id := "symlink", fixture := .symlink }
    , { id := "opaque", fixture := .opaque }
    , { id := "empty", fixture := .empty }
    , { id := "closed-wal", fixture := .closedWal }
    , { id := "truncated-magic", fixture := .truncatedHeader, size := 19 }
    , { id := "mixed-read-version", fixture := .mixedReadVersion, size := 20 }
    , { id := "mixed-write-version", fixture := .mixedWriteVersion, size := 20 }
    , { id := "exact-cap-reaches-header", fixture := .mixedReadVersion, size := cap }
    , { id := "above-cap-preempts-header", fixture := .mixedReadVersion, size := cap + 1 }
    , { id := "sqlite-mode-guard", deleteMode := false, integrity := .unusedPage, modelOnly := true }
    , { id := "sqlite-page-budget", pages := 4097, modelOnly := true }
    ]

private def fixtureName : Fixture → String
  | .sqlite => "sqlite"
  | .missing => "missing"
  | .directory => "directory"
  | .symlink => "symlink"
  | .empty => "empty"
  | .opaque => "opaque"
  | .closedWal => "closed_wal"
  | .truncatedHeader => "truncated_header"
  | .mixedReadVersion => "mixed_read_version"
  | .mixedWriteVersion => "mixed_write_version"

private def resultName : Result → String
  | .admitted => "admitted"
  | .absent => "absent"
  | .created => "created"
  | .fileKindOrSize => "file_kind_or_size"
  | .journalMode => "journal_mode"
  | .sqliteError => "sqlite_error"
  | .identity => "identity"
  | .unexpectedObject => "unexpected_object"
  | .incompleteSchema => "incomplete_schema"
  | .project => "project"
  | .integrity => "integrity"
  | .pageSize => "page_size"
  | .pageBudget => "page_budget"

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)), ("id", .str item.id),
      ("mode", .str (if item.modelOnly then "model-only" else "internal-fixture")),
      ("fixture", .str (fixtureName item.fixture)), ("size", Lean.toJson item.size),
      ("known_application", Lean.toJson item.knownApplication), ("known_version", Lean.toJson item.knownVersion),
      ("shape", .str (shapeName item.shape)), ("project", .str (projectName item.project)),
      ("integrity", .str (integrityName item.integrity)), ("page_size", Lean.toJson item.pageSize),
      ("read_expected", .str (resultName (admit .read (toFile item)))),
      ("write_expected", .str (resultName (admit .write (toFile item))))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let evaluate := fun access item => admit access (toFile item)
  let variants : List (String × (Access → Case → Result)) :=
    [ ("ignore_header_format", fun access item =>
        match toFile item with
        | .regular size header schema =>
          if size ≤ cap && headerRejects header then schema.elim .sqliteError (checkSchema access)
          else evaluate access item
        | _ => evaluate access item)
    , ("reject_exact_size", fun access item => if item.size = cap then .fileKindOrSize else evaluate access item)
    , ("skip_application_and_version", fun access item =>
        evaluate access { item with knownApplication := true, knownVersion := true })
    , ("ignore_schema_objects", fun access item => evaluate access { item with shape := .standard })
    , ("ignore_project_rows", fun access item => evaluate access { item with project := .matching })
    , ("check_integrity_before_project", fun access item =>
        if item.fixture == .sqlite && item.integrity == .unusedPage then .integrity else evaluate access item)
    , ("ignore_integrity_check", fun access item => evaluate access { item with integrity := .valid })
    , ("assume_check_constraint_reported_on_read", fun access item =>
        if item.integrity == .invalidRevision then evaluate .write item else evaluate access item)
    , ("allow_other_page_size", fun access item => evaluate access { item with pageSize := 4096 })
    , ("ignore_page_budget", fun access item => evaluate access { item with pages := 4 })
    , ("ignore_sqlite_journal_mode", fun access item => evaluate access { item with deleteMode := true })
    ]
  let observations := cases.flatMap fun item => [(Access.read, item), (.write, item)]
  for (name, variant) in variants do
    if let some (_, item) := observations.find? (fun (access, item) => variant access item != evaluate access item) then
      IO.println s!"review store admission sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"review store admission missed variant: {name}"
      return 1
  IO.println s!"review_store_admission_cases={cases.length} model_only={(cases.filter Case.modelOnly).length}"
  return 0

end HoiminOracle.ReviewStoreAdmission
