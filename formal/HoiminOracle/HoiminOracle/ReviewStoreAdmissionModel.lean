namespace HoiminOracle.ReviewStoreAdmission

inductive Access | read | write
  deriving DecidableEq

inductive Objects | complete | unexpected | incomplete
  deriving DecidableEq

structure Schema where
  knownApplication : Bool := true
  knownVersion : Bool := true
  objects : Objects := .complete
  projectMatches : Bool := true
  deleteMode : Bool := true
  integrityOk : Access → Bool := fun _ => true
  pageSize : Nat := 4096
  pages : Nat := 4

inductive File
  | missing
  | nonregular
  | regular (size : Nat) (header : List UInt8) (schema : Option Schema)

inductive Result
  | admitted | absent | created | fileKindOrSize | journalMode | sqliteError
  | identity | unexpectedObject | incompleteSchema | project | integrity | pageSize | pageBudget
  deriving DecidableEq

def cap : Nat := 16 * 1024 * 1024

def sqliteMagic : List UInt8 := "SQLite format 3".toUTF8.toList ++ [0]

def headerRejects (data : List UInt8) : Bool :=
  let header := data.take 20
  sqliteMagic.isPrefixOf header && header.drop 18 != [1, 1]

def checkGeometry (size pages : Nat) : Result :=
  if size != 4096 then .pageSize
  else if size * pages > cap then .pageBudget
  else .admitted

def checkSchema (access : Access) (schema : Schema) : Result :=
  if !schema.knownApplication || !schema.knownVersion then .identity
  else if schema.objects = .unexpected then .unexpectedObject
  else if schema.objects = .incomplete then .incompleteSchema
  else if !schema.projectMatches then .project
  else if !schema.deleteMode then .journalMode
  else if !schema.integrityOk access then .integrity
  else checkGeometry schema.pageSize schema.pages

def admit : Access → File → Result
  | .read, .missing => .absent
  | .write, .missing => .created
  | _, .nonregular => .fileKindOrSize
  | access, .regular size header schema =>
    if size > cap then .fileKindOrSize
    else if headerRejects header then .journalMode
    else schema.elim .sqliteError (checkSchema access)

theorem oversized_file_preempts_every_later_check (access : Access) (size : Nat)
    (header : List UInt8) (schema : Option Schema) (exceeds : cap < size) :
    admit access (.regular size header schema) = .fileKindOrSize := by
  cases access <;> simp [admit, exceeds]

theorem header_rejection_preempts_sqlite_decoding (access : Access) (size : Nat)
    (header : List UInt8) (schema : Option Schema) (fits : size ≤ cap)
    (rejected : headerRejects header = true) :
    admit access (.regular size header schema) = .journalMode := by
  cases access <;> simp [admit, Nat.not_lt_of_ge fits, rejected]

theorem unexpected_objects_preempt_project_and_integrity (access : Access) (schema : Schema)
    (app : schema.knownApplication = true) (version : schema.knownVersion = true)
    (objects : schema.objects = .unexpected) :
    checkSchema access schema = .unexpectedObject := by
  simp [checkSchema, app, version, objects]

theorem configured_page_limit (pages : Nat) : checkGeometry 4096 pages = .admitted ↔ pages ≤ 4096 := by
  simp [checkGeometry, cap]
  omega

theorem page_size_preempts_page_budget (size pages : Nat) (wrong : size ≠ 4096) :
    checkGeometry size pages = .pageSize := by
  simp [checkGeometry, wrong]

end HoiminOracle.ReviewStoreAdmission
