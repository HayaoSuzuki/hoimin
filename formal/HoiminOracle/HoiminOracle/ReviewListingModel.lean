namespace HoiminOracle.ReviewListing

inductive Source | fresh | stale | unavailable
  deriving DecidableEq

inductive Assessment | actionable | notUseful | needsInvestigation
  deriving DecidableEq

inductive Operator | booleanLiteral | integerLiteralNeighbor
  deriving DecidableEq

inductive Status | killed | survived
  deriving DecidableEq

inductive Execution | notLoaded | notInReport | result (status : Status)
  deriving DecidableEq

inductive Failure | storedIdentity | reportDescriptor
  deriving DecidableEq

structure Candidate where
  id : Nat
  operator : Operator
  sequence : Nat
  deriving DecidableEq

structure Planned where
  candidate : Candidate
  source : Source

structure Stored where
  candidate : Candidate
  descriptorMatches : Bool
  history : List Assessment

structure Reported where
  id : Nat
  descriptorMatches : Bool
  status : Status

structure Row where
  candidate : Candidate
  source : Source
  history : List Assessment
  review : Option Assessment
  execution : Execution
  deriving DecidableEq

def execution (reported : Option (List Reported)) (id : Nat) : Except Failure Execution := do
  let some entries := reported | return .notLoaded
  let some entry := entries.find? (fun row => row.id == id) | return .notInReport
  if entry.descriptorMatches then return .result entry.status
  else throw .reportDescriptor

def shownReview (source : Source) (history : List Assessment) : Option Assessment :=
  if source = .fresh then history.getLast? else none

def currentRow (snapshot : List Stored) (reported : Option (List Reported))
    (planned : Planned) : Except Failure Row := do
  let stored := snapshot.find? (fun row => row.candidate.id == planned.candidate.id)
  if let some row := stored then
    if !row.descriptorMatches then throw .storedIdentity
  let history := (stored.map Stored.history).getD []
  let executed ← execution reported planned.candidate.id
  return ⟨planned.candidate, planned.source, history, shownReview planned.source history, executed⟩

def retiredInputs (planned : List Planned) (snapshot : List Stored) : List Stored :=
  let current := planned.map (Candidate.id ∘ Planned.candidate)
  snapshot.filter fun row => !current.contains row.candidate.id

def retiredRow (reported : Option (List Reported)) (stored : Stored) : Except Failure Row := do
  let executed ← execution reported stored.candidate.id
  return ⟨{ stored.candidate with sequence := 0 }, .stale, stored.history, none, executed⟩

structure Counts where
  total : Nat := 0
  reviewed : Nat := 0
  unreviewed : Nat := 0
  stale : Nat := 0
  unavailable : Nat := 0
  assessments : List Assessment := []
  deriving DecidableEq

def tally : List Row → Counts
  | [] => {}
  | row :: rest =>
    let c := tally rest
    let c := { c with total := c.total + 1 }
    match row.source, row.review with
    | .stale, _ => { c with stale := c.stale + 1 }
    | .unavailable, _ => { c with unavailable := c.unavailable + 1 }
    | .fresh, none => { c with unreviewed := c.unreviewed + 1 }
    | .fresh, some review => { c with reviewed := c.reviewed + 1, assessments := review :: c.assessments }

structure Listing where
  current : List Row
  retired : List Row
  operators : List (Operator × Counts)
  deriving DecidableEq

def operatorCounts (current : List Row) : List (Operator × Counts) :=
  ([.booleanLiteral, .integerLiteralNeighbor] : List Operator).filterMap fun operator =>
    let rows := current.filter (fun row => row.candidate.operator == operator)
    if rows.isEmpty then none else some (operator, tally rows)

def listReviews (planned : List Planned) (snapshot : List Stored)
    (reported : Option (List Reported)) (includeRetired : Bool) : Except Failure Listing := do
  let current ← planned.mapM (currentRow snapshot reported)
  let retired ← if includeRetired then (retiredInputs planned snapshot).mapM (retiredRow reported) else pure []
  return ⟨current, retired, operatorCounts current⟩

theorem only_fresh_presents_latest_review (source : Source) (history : List Assessment)
    (inactive : source ≠ .fresh) : shownReview source history = none := by
  simp [shownReview, inactive]

theorem retired_input_iff_absent_from_plan (planned : List Planned) (snapshot : List Stored)
    (row : Stored) : row ∈ retiredInputs planned snapshot ↔
      row ∈ snapshot ∧ row.candidate.id ∉ planned.map (Candidate.id ∘ Planned.candidate) := by
  simp [retiredInputs, List.mem_filter]

theorem retired_row_keeps_history (reported : Option (List Reported)) (stored : Stored)
    (result : Execution) (admitted : execution reported stored.candidate.id = .ok result) :
    retiredRow reported stored =
      .ok ⟨{ stored.candidate with sequence := 0 }, .stale, stored.history, none, result⟩ := by
  simp [retiredRow, admitted] <;> rfl

theorem counts_partition_rows (rows : List Row) :
    let counts := tally rows
    counts.total = counts.reviewed + counts.unreviewed + counts.stale + counts.unavailable := by
  induction rows with
  | nil => rfl
  | cons row rest ih =>
    cases source : row.source <;> cases review : row.review <;> simp [tally, source, review] <;> omega

theorem one_assessment_per_reviewed_row (rows : List Row) :
    (tally rows).assessments.length = (tally rows).reviewed := by
  induction rows with
  | nil => rfl
  | cons row rest ih =>
    cases source : row.source <;> cases review : row.review <;> simp [tally, source, review, ih]

end HoiminOracle.ReviewListing
