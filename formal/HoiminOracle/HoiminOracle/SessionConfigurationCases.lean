import HoiminOracle.SessionConfigurationModel
import Lean.Data.Json

namespace HoiminOracle.SessionConfiguration

open HoiminOracle.SessionAdmission

private structure Case where
  id : String
  before : Durable
  attempt : Attempt := {}
  temporary : Bool := false

private def cases : List Case :=
  let existing := initialized fresh 1
  [ ⟨"fresh-success", fresh, {}, false⟩
  , ⟨"fresh-out-of-pages", fresh, { fullDuringSchema := true }, false⟩
  , ⟨"initialization-error-precedes-journal-contention", fresh,
      { fullDuringSchema := true, journal := .busy }, false⟩
  , ⟨"fresh-committed-before-journal-contention", fresh, { journal := .busy }, false⟩
  , ⟨"current-in-rollback-mode", existing, {}, false⟩
  , ⟨"current-preserved-on-journal-contention", existing, { journal := .busy }, false⟩
  , ⟨"current-in-wal-mode", { existing with journal := .wal }, {}, false⟩
  , ⟨"unsupported-stops-before-journal", { existing with version := 99 }, { journal := .busy }, false⟩
  , ⟨"foreign-stops-before-journal", { fresh with objects := .foreign }, { journal := .busy }, false⟩
  , ⟨"peer-initializes-during-open", fresh, { peerInitializes := true }, false⟩
  , ⟨"peer-data-preserved-on-journal-contention", fresh,
      { peerInitializes := true, journal := .busy }, false⟩
  , ⟨"fresh-temporary-refuses-journal", fresh, { journal := .refused }, true⟩
  , ⟨"current-temporary-refuses-journal", existing, { journal := .refused }, true⟩ ]

private def durableJson (state : Durable) : Lean.Json :=
  let objects := match state.objects with
    | .empty => "empty"
    | .canonical => "canonical"
    | .foreign => "foreign"
  Lean.Json.mkObj
    [ ("version", Lean.toJson state.version), ("objects", .str objects), ("rows", Lean.toJson state.rows)
    , ("journal", .str (match state.journal with | .rollback => "rollback" | .wal => "wal")) ]

private def outcomeJson (outcome : Outcome) : Lean.Json :=
  Lean.Json.mkObj <| match outcome with
  | .unsupportedVersion version => [("kind", .str "unsupported_version"), ("version", Lean.toJson version)]
  | .opened => [("kind", .str "opened")]
  | .invalidIdentity => [("kind", .str "invalid_identity")]
  | .diskFull => [("kind", .str "disk_full")]
  | .busy => [("kind", .str "busy")]
  | .journalRefused => [("kind", .str "journal_refused")]

private def observedJson (observation : Observation) : Lean.Json :=
  Lean.Json.mkObj
    [ ("first_version", Lean.toJson observation.firstVersion), ("writer_version", Lean.toJson observation.writerVersion)
    , ("before_journal", (observation.beforeJournal.map durableJson).getD .null)
    , ("outcome", outcomeJson observation.outcome), ("after", durableJson observation.after) ]

private def reopen (item : Case) (state : Durable) : Observation :=
  run state { journal := if item.temporary then .refused else .available }

def corpus : String :=
  String.join (cases.map fun item =>
    let first := run item.before item.attempt
    let second := reopen item first.after
    let journal := match item.attempt.journal with
      | .available => "available"
      | .busy => "busy"
      | .refused => "refused"
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("before", durableJson item.before), ("temporary", .bool item.temporary)
      , ("peer_initializes", .bool item.attempt.peerInitializes)
      , ("full_during_schema", .bool item.attempt.fullDuringSchema), ("journal_reply", .str journal)
      , ("first", observedJson first)
      , ("second", Lean.Json.mkObj [("outcome", outcomeJson second.outcome), ("after", durableJson second.after)]) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation → Observation)) :=
    [ ("undo_commit_when_journal_fails", fun c result =>
        if result.beforeJournal.isSome && result.outcome != .opened then { result with after := c.before }
        else result)
    , ("attempt_journal_even_after_schema_rejection", fun c result =>
        if result.beforeJournal.isNone then { result with beforeJournal := some c.before } else result)
    , ("use_first_observation_when_peer_initialized", fun c _ =>
        run c.before { c.attempt with peerInitializes := false })
    , ("report_open_despite_failed_journal", fun _ result =>
        if result.beforeJournal.isSome then { result with outcome := .opened } else result)
    , ("retain_partial_schema_on_full_error", fun _ result =>
        if result.outcome == .diskFull then { result with after := initialized result.after 0 }
        else result)
    , ("clear_existing_rows_before_journal", fun _ result => { result with after := { result.after with rows := 0 } }) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        let observed := run item.before item.attempt
        decide (variant item observed ≠ observed)) then
      IO.println s!"session configuration sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session configuration missed variant: {name}"
      return 1
  IO.println s!"session_configuration_cases={cases.length}"
  return 0

end HoiminOracle.SessionConfiguration
