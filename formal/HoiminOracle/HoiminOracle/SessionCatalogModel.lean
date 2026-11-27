import HoiminOracle.SchemaMigrationModel

namespace HoiminOracle.SessionCatalog

open HoiminOracle.SchemaMigration (normalizeSchemaSql schemaWords sqliteSchemaSpace)

structure Object where
  kind : String
  name : String
  tableName : String
  sql : Option String
  deriving DecidableEq, BEq

def normalizeSql (sql : String) : String :=
  String.ofList (normalizeSchemaSql sqliteSchemaSpace sql.toList)

def normalizeObject (object : Object) : Object :=
  { object with sql := object.sql.map normalizeSql }

def key (object : Object) : String × String := (object.kind, object.name)

def keyLe (left right : Object) : Bool :=
  match compare left.kind right.kind with
  | .lt => true
  | .eq => compare left.name right.name != .gt
  | .gt => false

def normalizedCatalog (objects : List Object) : List Object :=
  (objects.map normalizeObject).mergeSort keyLe

def catalogMatches (reference candidate : List Object) : Prop :=
  normalizedCatalog reference = normalizedCatalog candidate

instance (reference candidate : List Object) : Decidable (catalogMatches reference candidate) :=
  inferInstanceAs (Decidable (normalizedCatalog reference = normalizedCatalog candidate))

theorem same_words_have_same_normalization (left right : String)
    (words : schemaWords sqliteSchemaSpace left.toList = schemaWords sqliteSchemaSpace right.toList) :
    normalizeSql left = normalizeSql right := by
  simp [normalizeSql, normalizeSchemaSql, words]

theorem normalization_preserves_object_key (object : Object) :
    key (normalizeObject object) = key object := rfl

theorem normalization_preserves_table_name (object : Object) :
    (normalizeObject object).tableName = object.tableName := rfl

theorem null_sql_differs_from_empty_sql (object : Object) :
    normalizeObject { object with sql := none } ≠ normalizeObject { object with sql := some "" } := by
  intro equal
  have fields := congrArg Object.sql equal
  simp [normalizeObject] at fields

theorem catalog_match_preserves_object_count (reference candidate : List Object)
    (equal : catalogMatches reference candidate) : reference.length = candidate.length := by
  have sizes := congrArg List.length (show normalizedCatalog reference = normalizedCatalog candidate from equal)
  simpa [normalizedCatalog] using sizes

theorem added_object_prevents_match (reference : List Object) (extra : Object) :
    ¬ catalogMatches reference (extra :: reference) := by
  intro equal
  have sizes := catalog_match_preserves_object_count _ _ equal
  simp at sizes

end HoiminOracle.SessionCatalog
