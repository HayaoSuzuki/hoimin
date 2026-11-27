import HoiminOracle.SessionCatalogModel
import Lean.Data.Json

namespace HoiminOracle.SessionCatalog

open HoiminOracle.SchemaMigration (normalizeSchemaSql sqliteSchemaSpace)

-- Representatives include the precise fragments changed by the fixtures.
-- The Rust adapter retrieves complete statements from the production initializer.
private def reference : List Object :=
  [ ⟨"table", "fingerprints", "fingerprints",
      some "CREATE TABLE fingerprints (digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest) = 32), schema_version INTEGER NOT NULL)"⟩
  , ⟨"index", "sqlite_autoindex_fingerprints_1", "fingerprints", none⟩
  , ⟨"table", "runs", "runs",
      some "CREATE TABLE runs (fingerprint BLOB NOT NULL REFERENCES fingerprints(digest))"⟩
  , ⟨"index", "runs_resume", "runs",
      some "CREATE INDEX runs_resume ON runs(fingerprint, complete, id DESC)"⟩
  , ⟨"index", "diagnostics_result", "diagnostics",
      some "CREATE INDEX diagnostics_result ON diagnostics(run_id, mutant_id, id)"⟩ ]

private inductive Edit where
  | replace (name before after : String)
  | remove (name : String)
  | add (object : Object)
  | reverseCreation
  | shiftRootPages

private def transform (objects : List Object) : Edit → List Object
  | .replace name before after => objects.map fun object =>
      if object.name = name then { object with sql := object.sql.map (·.replace before after) } else object
  | .remove name => objects.filter (·.name != name)
  | .add object => objects ++ [object]
  | .reverseCreation => objects.reverse
  | .shiftRootPages => objects

private structure Case where
  id : String
  edits : List Edit
  wal : Bool

private def cases : List Case :=
  let replaceType := Edit.replace "fingerprints" "schema_version INTEGER"
  let fixtures : List (String × List Edit) :=
    [ ("canonical", [])
    , ("reverse-creation", [.reverseCreation])
    , ("different-root-pages", [.shiftRootPages])
    , ("tabs", [replaceType "schema_version\t\tINTEGER"])
    , ("line-breaks", [replaceType "schema_version\r\n    INTEGER"])
    , ("form-feed", [replaceType ("schema_version" ++ String.singleton (Char.ofNat 12) ++ "INTEGER")])
    , ("nbsp-in-identifier", [replaceType "schema_version  INTEGER"])
    , ("em-space-in-identifier", [replaceType "schema_version  INTEGER"])
    , ("lowercase-type", [replaceType "schema_version integer"])
    , ("quoted-identifier", [replaceType "\"schema_version\" INTEGER"])
    , ("space-after-parenthesis", [.replace "fingerprints" "CHECK(length(digest)" "CHECK( length(digest)"])
    , ("remove-not-null", [.replace "fingerprints" "schema_version INTEGER NOT NULL" "schema_version INTEGER"])
    , ("change-digest-check", [.replace "fingerprints" "length(digest) = 32" "length(digest) = 31"])
    , ("remove-foreign-key", [.replace "runs" "fingerprint BLOB NOT NULL REFERENCES fingerprints(digest)"
          "fingerprint BLOB NOT NULL"])
    , ("change-index-directions", [.replace "runs_resume" "id DESC" "id ASC"])
    , ("change-index-column-order", [.replace "runs_resume" "fingerprint, complete" "complete, fingerprint"])
    , ("remove-resume-index", [.remove "runs_resume"])
    , ("remove-diagnostic-index", [.remove "diagnostics_result"])
    , ("add-table", [.add ⟨"table", "extra", "extra", some "CREATE TABLE extra(value INTEGER)"⟩])
    , ("add-index", [.add ⟨"index", "extra_index", "runs", some "CREATE INDEX extra_index ON runs(id)"⟩])
    , ("add-view", [.add ⟨"view", "extra_view", "extra_view", some "CREATE VIEW extra_view AS SELECT 1 AS value"⟩])
    , ("add-trigger", [.add ⟨"trigger", "extra_trigger", "runs",
        some "CREATE TRIGGER extra_trigger AFTER INSERT ON runs BEGIN SELECT 1; END"⟩])
    , ("format-and-change-index", [replaceType "schema_version  INTEGER",
        .replace "runs_resume" "id DESC" "id ASC"]) ]
  [false, true].flatMap fun wal =>
    fixtures.map fun (id, edits) => ⟨id ++ (if wal then "-wal" else "-rollback"), edits, wal⟩

private def objectsOf (item : Case) := item.edits.foldl transform reference

private def editJson : Edit → Lean.Json
  | .replace name before after =>
    Lean.Json.mkObj [("kind", .str "replace"), ("name", .str name), ("before", .str before), ("after", .str after)]
  | .remove name => Lean.Json.mkObj [("kind", .str "remove"), ("name", .str name)]
  | .add object => Lean.Json.mkObj [("kind", .str "add"), ("sql", Lean.toJson object.sql)]
  | .reverseCreation => Lean.Json.mkObj [("kind", .str "reverse_creation")]
  | .shiftRootPages => Lean.Json.mkObj [("kind", .str "shift_root_pages")]

def corpus : String :=
  String.join (cases.map fun item =>
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("wal", .bool item.wal), ("edits", .arr (item.edits.map editJson).toArray)
      , ("admitted", .bool (decide (catalogMatches reference (objectsOf item)))) ]).compress ++ "\n")

private def compareAfter (rewrite : Object → Object) (candidate : List Object) : Bool :=
  decide (catalogMatches (reference.map rewrite) (candidate.map rewrite))

def sensitivity : IO UInt32 := do
  let variants : List (String × (List Object → Bool)) :=
    [ ("ignore_sql", compareAfter (fun object => { object with sql := none }))
    , ("fold_sql_case", compareAfter (fun object => { object with sql := object.sql.map String.toLower }))
    , ("split_on_unicode_spacing", compareAfter (fun object =>
        { object with sql := object.sql.map (fun sql =>
            String.ofList (normalizeSchemaSql
              (fun c => sqliteSchemaSpace c || c == Char.ofNat 160 || c == Char.ofNat 8195) sql.toList)) }))
    , ("ignore_indexes", fun candidate =>
        decide (catalogMatches (reference.filter (·.kind != "index")) (candidate.filter (·.kind != "index"))))
    , ("reject_different_creation_order", fun candidate =>
        decide (reference.map normalizeObject = candidate.map normalizeObject))
    , ("permit_extra_objects", fun candidate =>
        (normalizedCatalog reference).all fun object => (normalizedCatalog candidate).contains object) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        let objects := objectsOf item
        variant objects != decide (catalogMatches reference objects)) then
      IO.println s!"session catalog sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session catalog missed variant: {name}"
      return 1
  IO.println s!"session_catalog_cases={cases.length} accepted={(cases.filter fun c => decide (catalogMatches reference (objectsOf c))).length}"
  return 0

end HoiminOracle.SessionCatalog
