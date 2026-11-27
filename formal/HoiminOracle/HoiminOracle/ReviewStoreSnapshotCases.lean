import HoiminOracle.ReviewStoreSnapshotModel
import Lean.Data.Json

namespace HoiminOracle.ReviewStoreSnapshot

private inductive Key | valid | malformed | foreign | conflict
  deriving DecidableEq

private inductive Revision | blob (number : Nat) | short | text

private inductive Draft | valid | malformed | artifact | reason | evidence

private structure EventFixture where
  revision : Revision
  draft : Draft := .valid

private inductive Events
  | explicit (events : List EventFixture)
  | consecutive (count : Nat)

private structure CandidateFixture where
  key : Key := .valid
  events : Events := .consecutive 1

private structure Case where
  id : String
  candidates : List (Nat × CandidateFixture)
  orphanCount : Nat := 0

private def event (fixture : EventFixture) : Event :=
  let revision := match fixture.revision with
    | .blob number => .ok number
    | .short => .error .revisionWidth
    | .text => .error .revisionType
  let draft := match fixture.draft with
    | .valid => .ok ()
    | .malformed => .error .draftJson
    | .artifact => .error .draftArtifact
    | .reason => .error .draftReason
    | .evidence => .error .draftEvidence
  ⟨revision, draft⟩

private def candidate (fixture : Nat × CandidateFixture) : Candidate :=
  let (id, fixture) := fixture
  let key := match fixture.key with
    | .valid => .ok ()
    | .malformed => .error .keyJson
    | .foreign => .error .keyInvalid
    | .conflict => .error .keyConflict
  let events := match fixture.events with
    | .explicit values => values.map event
    | .consecutive count => (List.range' 1 count).map fun number => ⟨.ok number, .ok ()⟩
  ⟨id, key, events⟩

private def annotated (draft : Draft) : CandidateFixture :=
  { events := .explicit [⟨.blob 1, .valid⟩, ⟨.blob 2, draft⟩] }

private def choices : List (String × Option CandidateFixture) :=
  [ ("absent", none)
  , ("three", some { events := .consecutive 3 })
  , ("single", some {})
  , ("empty", some { events := .consecutive 0 })
  , ("key-json", some { key := .malformed })
  , ("project", some { key := .foreign })
  , ("conflict", some { key := .conflict })
  , ("revision-zero", some { events := .explicit [⟨.blob 0, .valid⟩] })
  , ("revision-gap", some { events := .explicit [⟨.blob 1, .valid⟩, ⟨.blob 3, .valid⟩] })
  , ("revision-width", some { events := .explicit [⟨.short, .valid⟩] })
  , ("revision-type", some { events := .explicit [⟨.text, .valid⟩] })
  , ("draft-json", some (annotated .malformed))
  , ("artifact", some (annotated .artifact))
  , ("reason", some (annotated .reason))
  , ("evidence", some (annotated .evidence))
  ]

private def cases : List Case :=
  let matrix := choices.flatMap fun (firstName, first) =>
    choices.map fun (secondName, second) =>
      { id := s!"rows-{firstName}-{secondName}"
        candidates := [(0, first), (1, second)].filterMap fun (id, row) =>
          row.map (id, ·) }
  matrix ++
    [ ⟨"orphan-alone", [], 1⟩
    , ⟨"orphan-after-valid", [(0, {}), (1, {})], 1⟩
    , ⟨"draft-before-orphan", [(0, {}), (1, annotated .evidence)], 1⟩
    , ⟨"key-before-empty-history", [(0, { key := .foreign, events := .consecutive 0 })], 0⟩
    , ⟨"revision-before-draft", [(1, { events := .explicit [⟨.blob 2, .malformed⟩] })], 0⟩
    , ⟨"bad-width-before-draft", [(1, { events := .explicit [⟨.short, .malformed⟩] })], 0⟩
    , ⟨"event-limit-inclusive", [(0, { events := .consecutive 9999 }), (1, {})], 0⟩
    , ⟨"event-limit-before-key", [(0, { key := .malformed }), (1, { events := .consecutive 10000 })], 0⟩
    ]

private def eventCount (item : Case) (candidates : List Candidate) : Nat :=
  item.orphanCount + (candidates.map (fun row => row.events.length)).sum

private def access : List (Option Nat) := [none, some 0, some 1, some 2]

private def failureName : Failure → String
  | .eventLimit => "event_limit"
  | .keyJson => "key_json"
  | .keyInvalid => "key_invalid"
  | .keyConflict => "key_conflict"
  | .noHistory => "no_history"
  | .revisionWidth => "revision_width"
  | .revisionType => "revision_type"
  | .revisionOrder => "revision_order"
  | .draftJson => "draft_json"
  | .draftArtifact => "draft_artifact"
  | .draftReason => "draft_reason"
  | .draftEvidence => "draft_evidence"
  | .orphans => "orphans"

private def keyName : Key → String
  | .valid => "valid"
  | .malformed => "malformed"
  | .foreign => "foreign"
  | .conflict => "conflict"

private def draftName : Draft → String
  | .valid => "valid"
  | .malformed => "malformed"
  | .artifact => "artifact"
  | .reason => "reason"
  | .evidence => "evidence"

private def eventJson (fixture : EventFixture) : Lean.Json :=
  let revision := match fixture.revision with
    | .blob number => Lean.toJson number
    | .short => .str "short"
    | .text => .str "text"
  Lean.Json.mkObj [("revision", revision), ("draft", .str (draftName fixture.draft))]

private def candidateJson (entry : Nat × CandidateFixture) : Lean.Json :=
  let (slot, fixture) := entry
  let events := match fixture.events with
    | .explicit values => .arr (values.map eventJson).toArray
    | .consecutive count => Lean.toJson count
  Lean.Json.mkObj [("slot", Lean.toJson slot), ("key", .str (keyName fixture.key)), ("events", events)]

private def outcome : Except Failure (List History) → Lean.Json
  | .error failure => .str (failureName failure)
  | .ok histories => .arr (histories.map fun history =>
      Lean.Json.mkObj [("slot", Lean.toJson history.id), ("length", Lean.toJson history.length)]).toArray

def corpus : String :=
  String.join (cases.map fun item =>
    let candidates := item.candidates.map candidate
    let count := eventCount item candidates
    let reads := access.map fun selected =>
      Lean.Json.mkObj [("selected", Lean.toJson selected), ("expected", outcome (read count candidates selected))]
    (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("id", .str item.id),
      ("candidates", .arr (item.candidates.map candidateJson).toArray), ("orphans", Lean.toJson item.orphanCount),
      ("reads", .arr reads.toArray)]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Nat → List Candidate → Option Nat → Except Failure (List History))) :=
    [ ("validate_only_selected", fun count rows selected =>
        let chosen := rows.filter (fun row => selected.all (· == row.id))
        let unselected := rows.filter (fun row => !(selected.all (· == row.id)))
        read (count - (unselected.map (fun row => row.events.length)).sum) chosen selected)
    , ("ignore_orphans", fun _ rows selected =>
        read (rows.map (fun row => row.events.length)).sum rows selected)
    , ("scan_in_reverse", fun count rows selected => read count rows.reverse selected)
    , ("exclude_exact_event_limit", fun count rows selected =>
        read (if count = eventLimit then count + 1 else count) rows selected)
    , ("check_cap_after_scan", fun count rows selected =>
        match rows.mapM validateCandidate with
        | .error failure => .error failure
        | .ok _ => read count rows selected)
    , ("ignore_key", fun count rows selected =>
        read count (rows.map fun row => { row with key := .ok () }) selected)
    , ("renumber_revisions", fun count rows selected =>
        let renumber := fun row : Candidate =>
          { row with events := row.events.zipIdx.map fun (entry, index) =>
              { entry with revision := .ok (index + 1) } }
        read count (rows.map renumber) selected)
    , ("ignore_drafts", fun count rows selected =>
        let clear := fun row : Candidate =>
          { row with events := row.events.map fun entry => { entry with draft := .ok () } }
        read count (rows.map clear) selected)
    , ("leak_other_histories", fun count rows _ => read count rows none)
    ]
  for (name, variant) in variants do
    let witness := cases.find? fun item =>
      let rows := item.candidates.map candidate
      let count := eventCount item rows
      access.any fun selected =>
        (outcome (variant count rows selected)).compress != (outcome (read count rows selected)).compress
    if let some item := witness then
      IO.println s!"review store snapshot sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"review store snapshot missed variant: {name}"
      return 1
  IO.println s!"review_store_snapshot_cases={cases.length} reads={cases.length * access.length}"
  return 0

end HoiminOracle.ReviewStoreSnapshot
