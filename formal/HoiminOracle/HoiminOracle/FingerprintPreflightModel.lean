namespace HoiminOracle.FingerprintPreflight

inductive File where
  | base | extra | settings | source | notes
  deriving DecidableEq, BEq, Repr

def File.path : File → String
  | .base => "base.cfg"
  | .extra => "extra.cfg"
  | .settings => "settings.toml"
  | .source => "src/calc.py"
  | .notes => "notes.txt"

def files : List File := [.base, .extra, .notes, .settings, .source]

abbrev Tree := File → Option String
abbrev Records := List (File × String)

structure Edit where
  file : File
  content : Option String

def apply (tree : Tree) (edits : List Edit) : Tree :=
  edits.foldl (fun current edit file =>
    if file = edit.file then edit.content else current file) tree

structure Rules where
  excluded : List File
  protectSource : Bool

def matchesGlob (file : File) : Bool := file == .base || file == .extra

def matchesInput (rules : Rules) (file : File) : Bool :=
  matchesGlob file || file == .settings || (rules.protectSource && file == .source)

def records (tree : Tree) : Records :=
  files.filterMap fun file => (tree file).map fun content => (file, content)

def copied (rules : Rules) (tree : Tree) : Records :=
  (records tree).filter fun (file, _) => !rules.excluded.contains file

def lookup (entries : Records) (file : File) : Option String :=
  (entries.find? fun entry => entry.1 == file).map Prod.snd

inductive Failure where
  | globUnmatched | exactMissing | liveRecordsChanged | snapshotChanged
  | sourceMissing | originalsChanged
  deriving DecidableEq, BEq, Repr

def resolve (rules : Rules) (tree : Tree) : Except Failure Records :=
  let entries := records tree
  if !(entries.any fun entry => matchesGlob entry.1) then .error .globUnmatched
  else if (tree .settings).isNone || (rules.protectSource && (tree .source).isNone) then
    .error .exactMissing
  else .ok (entries.filter fun entry => matchesInput rules entry.1)

def copiedAtPreparation (rules : Rules) (expected : Records) : List File :=
  (expected.map Prod.fst).filter fun file => !rules.excluded.contains file

structure Checks where
  live : Bool := true
  snapshotRecords : Bool := true
  requiredPresence : Bool := true
  requireAllInputs : Bool := false
  source : Bool := true
  originals : Bool := true

private def validateCopy (checks : Checks) (rules : Rules) (expected snapshot final : Records) :
    Except Failure Unit :=
  let required := if checks.requireAllInputs then expected.map Prod.fst
    else copiedAtPreparation rules expected
  if checks.snapshotRecords && snapshot.any (fun (file, content) =>
    matchesInput rules file && lookup expected file != some content) then .error .snapshotChanged
  else if checks.requiredPresence && required.any (fun file => (lookup snapshot file).isNone) then
    .error .snapshotChanged
  else if checks.source && (lookup snapshot .source).isNone then .error .sourceMissing
  else if checks.originals && snapshot ≠ final then .error .originalsChanged
  else .ok ()

def validateWith (checks : Checks) (rules : Rules) (expected : Records)
    (snapshot : Records) (live : Tree) (final : Records) : Except Failure Unit :=
  match if checks.live then resolve rules live else .ok expected with
  | .error failure => .error failure
  | .ok found =>
    if found = expected then validateCopy checks rules expected snapshot final
    else .error .liveRecordsChanged

def validate := validateWith {}

theorem live_resolution_failure_precedes_snapshot (rules : Rules) (expected snapshot final : Records)
    (live : Tree) (failure : Failure) (h : resolve rules live = .error failure) :
    validate rules expected snapshot live final = .error failure := by
  simp [validate, validateWith, h]

theorem live_record_drift_precedes_snapshot (rules : Rules) (expected found snapshot final : Records)
    (live : Tree) (h : resolve rules live = .ok found) (hne : found ≠ expected) :
    validate rules expected snapshot live final = .error .liveRecordsChanged := by
  simp [validate, validateWith, h, hne]

theorem publication_requires_consistent_inputs (rules : Rules) (expected snapshot final : Records)
    (live : Tree) (h : validate rules expected snapshot live final = .ok ()) :
    resolve rules live = .ok expected ∧
    (∀ file content, (file, content) ∈ snapshot → matchesInput rules file = true →
      lookup expected file = some content) ∧
    (∀ file, file ∈ copiedAtPreparation rules expected → (lookup snapshot file).isSome = true) ∧
    (lookup snapshot .source).isSome = true ∧ snapshot = final := by
  cases hr : resolve rules live with
  | error failure => simp [validate, validateWith, hr] at h
  | ok found =>
    by_cases hf : found = expected
    · subst found
      have hrecords : (snapshot.any fun (file, content) =>
          matchesInput rules file && lookup expected file != some content) = false := by
        cases he : snapshot.any (fun (file, content) =>
          matchesInput rules file && lookup expected file != some content)
        · rfl
        · simp [validate, validateWith, validateCopy, hr, he] at h
      have hpresence : ((copiedAtPreparation rules expected).any fun file =>
          (lookup snapshot file).isNone) = false := by
        cases he : (copiedAtPreparation rules expected).any (fun file => (lookup snapshot file).isNone)
        · rfl
        · simp [validate, validateWith, validateCopy, hr, hrecords, he] at h
      have hsource : (lookup snapshot .source).isSome = true := by
        cases he : lookup snapshot .source
        · simp [validate, validateWith, validateCopy, hr, hrecords, hpresence, he] at h
        · rfl
      have hfinal : snapshot = final := by
        simpa [validate, validateWith, validateCopy, hr, hrecords, hpresence,
          Option.isSome_iff_ne_none.mp hsource] using h
      refine ⟨rfl, ?_, ?_, hsource, hfinal⟩
      · intro file content hm hp
        have hmem := List.any_eq_false.mp hrecords (file, content) hm
        simpa [hp] using hmem
      · intro file hm
        have hmem := List.any_eq_false.mp hpresence file hm
        apply Option.isSome_iff_ne_none.mpr
        simpa using hmem
    · simp [validate, validateWith, hr, hf] at h

structure History where
  beforeCopy : List Edit := []
  beforeValidation : List Edit := []
  afterValidation : List Edit := []

def runWith (checks : Checks) (rules : Rules) (expected : Records)
    (prepared : Tree) (history : History) : Records × Except Failure Unit :=
  let atCopy := apply prepared history.beforeCopy
  let snapshot := copied rules atCopy
  let live := apply atCopy history.beforeValidation
  let final := copied rules (apply live history.afterValidation)
  (snapshot, validateWith checks rules expected snapshot live final)

end HoiminOracle.FingerprintPreflight
