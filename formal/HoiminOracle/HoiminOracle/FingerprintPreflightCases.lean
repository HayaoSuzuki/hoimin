import HoiminOracle.FingerprintPreflightModel
import Lean.Data.Json

namespace HoiminOracle.FingerprintPreflight

private def prepared : Tree
  | .base => some "base before"
  | .extra => none
  | .settings => some "enabled = true\n"
  | .source => some "def calc():\n    return 1 + 2\n"
  | .notes => some "original notes"

private structure Case where
  id : String
  rules : Rules
  history : History

private def variants : List (String × Edit) :=
  [ ("base-write", ⟨.base, some "base after"⟩)
  , ("base-remove", ⟨.base, none⟩)
  , ("extra-add", ⟨.extra, some "new config"⟩)
  , ("settings-write", ⟨.settings, some "enabled = false\n"⟩)
  , ("settings-remove", ⟨.settings, none⟩)
  , ("notes-write", ⟨.notes, some "updated notes"⟩)
  , ("notes-remove", ⟨.notes, none⟩)
  , ("source-remove", ⟨.source, none⟩) ]

private def scopes : List (String × Rules) :=
  [ ("all-copied", ⟨[], false⟩)
  , ("excluded-inputs", ⟨[.base, .extra, .settings], false⟩)
  , ("source-also-protected", ⟨[], true⟩) ]

private def matrix : List Case :=
  scopes.flatMap fun (scope, rules) =>
    ⟨s!"{scope}-unchanged", rules, {}⟩ ::
    variants.flatMap fun (name, edit) =>
      let restore : Edit := ⟨edit.file, prepared edit.file⟩
      [ ⟨s!"{scope}-{name}-stays", rules, { beforeCopy := [edit] }⟩
      , ⟨s!"{scope}-{name}-restored-during-validation", rules,
          { beforeCopy := [edit], beforeValidation := [restore] }⟩
      , ⟨s!"{scope}-{name}-changes-after-validation", rules, { afterValidation := [edit] }⟩ ]

private def boundaries : List Case :=
  [ ⟨"glob-error-before-missing-exact", ⟨[], false⟩,
      { beforeCopy := [⟨.settings, none⟩, ⟨.base, none⟩] }⟩
  , ⟨"live-drift-before-snapshot-drift", ⟨[], false⟩,
      { beforeCopy := [⟨.base, some "old snapshot"⟩], beforeValidation := [⟨.extra, some "new glob"⟩] }⟩
  , ⟨"snapshot-drift-before-missing-source", ⟨[], false⟩,
      { beforeCopy := [⟨.settings, none⟩, ⟨.source, none⟩],
        beforeValidation := [⟨.settings, prepared .settings⟩] }⟩
  , ⟨"unprotected-copy-aba", ⟨[], false⟩,
      { beforeCopy := [⟨.notes, some "copied"⟩], beforeValidation := [⟨.notes, prepared .notes⟩],
        afterValidation := [⟨.notes, some "copied"⟩] }⟩ ]

private def cases := matrix ++ boundaries

private def expectedRecords (rules : Rules) : Records :=
  (records prepared).filter fun entry => matchesInput rules entry.1

private def recordsJson (entries : Records) : Lean.Json :=
  Lean.Json.mkObj (entries.map fun (file, content) => (file.path, .str content))

private def editsJson (edits : List Edit) : Lean.Json :=
  .arr (edits.map fun edit => Lean.Json.mkObj
    [("path", .str edit.file.path), ("content", Lean.toJson edit.content)]).toArray

private def outcome : Except Failure Unit → String
  | .ok () => "published"
  | .error .globUnmatched => "glob_unmatched"
  | .error .exactMissing => "exact_missing"
  | .error .liveRecordsChanged => "live_records_changed"
  | .error .snapshotChanged => "snapshot_changed"
  | .error .sourceMissing => "source_missing"
  | .error .originalsChanged => "originals_changed"

def corpus : String :=
  String.join (cases.map fun item =>
    let expected := expectedRecords item.rules
    let (snapshot, result) := runWith {} item.rules expected prepared item.history
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("prepared", recordsJson (records prepared))
      , ("excludes", Lean.toJson (item.rules.excluded.map File.path))
      , ("glob", .str "*.cfg")
      , ("exact", Lean.toJson (["./settings.toml"] ++
          if item.rules.protectSource then [File.source.path] else []))
      , ("expected_records", recordsJson expected)
      , ("copied_at_preparation", Lean.toJson ((copiedAtPreparation item.rules expected).map File.path))
      , ("before_copy", editsJson item.history.beforeCopy)
      , ("before_validation", editsJson item.history.beforeValidation)
      , ("after_validation", editsJson item.history.afterValidation)
      , ("snapshot", recordsJson snapshot), ("outcome", .str (outcome result)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × Checks) :=
    [ ("skip_live", { live := false })
    , ("skip_snapshot_records", { snapshotRecords := false })
    , ("skip_prepared_membership", { requiredPresence := false })
    , ("require_every_input_in_copy", { requireAllInputs := true })
    , ("skip_source", { source := false })
    , ("skip_final_comparison", { originals := false }) ]
  for (name, checks) in variants do
    let result (item : Case) (checks : Checks) :=
      outcome (runWith checks item.rules (expectedRecords item.rules) prepared item.history).2
    if let some item := cases.find? (fun item => decide (result item checks ≠ result item {})) then
      IO.println s!"fingerprint preflight sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"fingerprint preflight missed variant: {name}"
      return 1
  IO.println s!"fingerprint_preflight_cases={cases.length}"
  return 0

end HoiminOracle.FingerprintPreflight
