namespace HoiminOracle.ReviewStoreSnapshot

inductive Failure
  | eventLimit | keyJson | keyInvalid | keyConflict | noHistory
  | revisionWidth | revisionType | revisionOrder
  | draftJson | draftArtifact | draftReason | draftEvidence | orphans
  deriving DecidableEq

structure Event where
  revision : Except Failure Nat
  draft : Except Failure Unit := .ok ()

structure Candidate where
  id : Nat
  key : Except Failure Unit := .ok ()
  events : List Event

structure History where
  id : Nat
  length : Nat
  deriving DecidableEq

def eventLimit : Nat := 10000

def validateEvents : Nat → List Event → Except Failure Unit
  | _, [] => .ok ()
  | expected, event :: rest =>
    match event.revision with
    | .error failure => .error failure
    | .ok revision =>
      if revision != expected then .error .revisionOrder
      else match event.draft with
        | .error failure => .error failure
        | .ok () => validateEvents (expected + 1) rest

def validateCandidate (candidate : Candidate) : Except Failure History := do
  candidate.key
  if candidate.events.isEmpty then throw .noHistory
  validateEvents 1 candidate.events
  return ⟨candidate.id, candidate.events.length⟩

def validate (count : Nat) (candidates : List Candidate) : Except Failure (List History) := do
  if count > eventLimit then throw .eventLimit
  let histories ← candidates.mapM validateCandidate
  if count != (histories.map History.length).sum then throw .orphans
  return histories

def retained (selected : Option Nat) (history : History) : Bool :=
  selected.all (· == history.id)

def read (count : Nat) (candidates : List Candidate) (selected : Option Nat) :
    Except Failure (List History) :=
  (validate count candidates).map (List.filter (retained selected))

@[simp] private theorem retain_all (rows : List History) : rows.filter (retained none) = rows := by
  induction rows with
  | nil => rfl
  | cons head tail ih => simp [retained, Option.all, ih]

theorem selection_preserves_failure (count : Nat) (candidates : List Candidate)
    (selection : Option Nat) (failure : Failure) (invalid : validate count candidates = .error failure) :
    read count candidates selection = .error failure := by
  simp [read, invalid, Except.map]

theorem selected_history_is_filtered_snapshot (count : Nat) (candidates : List Candidate)
    (snapshot : List History) (target : Nat) (valid : read count candidates none = .ok snapshot) :
    read count candidates (some target) = .ok (snapshot.filter (retained (some target))) := by
  cases result : validate count candidates with
  | error failure => simp [read, result, Except.map] at valid
  | ok histories =>
    have same : histories = snapshot := by
      simpa [read, result, Except.map] using valid
    simp [read, result, same, Except.map]

theorem excessive_count_precedes_scan (count : Nat) (candidates : List Candidate)
    (selection : Option Nat) (over : eventLimit < count) :
    read count candidates selection = .error .eventLimit := by
  simp [read, validate, over, Except.map]
  rfl

theorem revision_error_precedes_draft (expected : Nat) (event : Event) (rest : List Event)
    (actual : Nat) (decoded : event.revision = .ok actual) (wrong : actual ≠ expected) :
    validateEvents expected (event :: rest) = .error .revisionOrder := by
  simp [validateEvents, decoded, wrong]

theorem admitted_revisions_are_consecutive (expected : Nat) (events : List Event)
    (valid : validateEvents expected events = .ok ()) :
    events.map Event.revision = (List.range' expected events.length).map Except.ok := by
  induction events generalizing expected with
  | nil => rfl
  | cons event tail ih =>
    cases revision : event.revision with
    | error failure => simp [validateEvents, revision] at valid
    | ok actual =>
      by_cases same : actual = expected
      · subst actual
        cases draft : event.draft with
        | error failure => simp [validateEvents, revision, draft] at valid
        | ok value =>
          have next : validateEvents (expected + 1) tail = .ok () := by
            simpa [validateEvents, revision, draft] using valid
          simp [List.range'_succ, revision, ih _ next]
      · simp [validateEvents, revision, same] at valid

end HoiminOracle.ReviewStoreSnapshot
