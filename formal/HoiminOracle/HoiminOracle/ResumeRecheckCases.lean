import HoiminOracle.ResumeRecheckModel
import Lean.Data.Json

namespace HoiminOracle.ResumeRecheck

private structure Case where
  owner : Owner
  write : Write
  edit : Edit

private def ownerName : Owner → String
  | .free => "free"
  | .loader => "loader"
  | .peer => "peer"

private def writeName : Write → String
  | .normal => "normal"
  | .ignore => "ignore"
  | .abort => "abort"

private def editName : Edit → String
  | .none => "none"
  | .raiseBudget => "raise_budget"
  | .restoreBudget => "restore_budget"
  | .changeFingerprint => "change_fingerprint"
  | .finish => "finish"
  | .complete => "complete"
  | .delete => "delete"
  | .insertNewer => "insert_newer"
  | .replaceWithSameKey => "replace_with_same_key"

private def outcomeName : Outcome → String
  | .resumed => "resumed"
  | .changed => "candidate_changed"
  | .active => "session.resume.active"
  | .updateError => "session.read"

private def Case.id (item : Case) :=
  s!"{ownerName item.owner}-{writeName item.write}-{editName item.edit}"

private def cases : List Case :=
  ([.free, .loader, .peer] : List Owner).flatMap fun owner =>
    ([.none, .raiseBudget, .restoreBudget, .changeFingerprint, .finish, .complete,
      .delete, .insertNewer, .replaceWithSameKey] : List Edit).map (fun edit => ⟨owner, .normal, edit⟩) ++
    ([.ignore, .abort] : List Write).flatMap fun write =>
      ([.none, .raiseBudget, .delete] : List Edit).map fun edit => ⟨owner, write, edit⟩

private def observe (item : Case) : Observation := recheck item.owner item.write (changeRows item.edit)

private def rowJson (row : Row) : Lean.Json :=
  Lean.Json.mkObj [("name", .str row.name), ("ordinal", Lean.toJson row.ordinal),
    ("fingerprint", Lean.toJson row.fingerprint), ("budget", Lean.toJson row.budget),
    ("finished", .bool row.finished), ("complete", .bool row.complete)]

def corpus : String :=
  String.join (cases.map fun item =>
    let actual := observe item
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("owner", .str (ownerName item.owner)), ("write", .str (writeName item.write))
      , ("edit", .str (editName item.edit)), ("requested", Lean.toJson requested)
      , ("outcome", .str (outcomeName actual.outcome)), ("after_owner", .str (ownerName actual.owner))
      , ("rows", Lean.toJson (actual.rows.map rowJson)) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("ignore_original_budget", fun item =>
        recheck item.owner item.write (editChosen (changeRows item.edit) ({ · with budget := chosen.budget })))
    , ("ignore_fingerprint_change", fun item =>
        recheck item.owner item.write (editChosen (changeRows item.edit) ({ · with fingerprint := chosen.fingerprint })))
    , ("ignore_completion", fun item =>
        recheck item.owner item.write (editChosen (changeRows item.edit) ({ · with complete := false })))
    , ("release_prior_ownership", fun item =>
        let result := observe item
        if item.owner == .loader && result.outcome != .resumed then { result with owner := .free } else result)
    , ("retain_unpublished_ownership", fun item =>
        let result := observe item
        if item.owner == .free && result.outcome != .resumed then { result with owner := .loader } else result)
    , ("ignore_database_error", fun item =>
        let result := observe item
        if result.outcome == .updateError then { result with outcome := .changed } else result) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ observe item)) then
      IO.println s!"resume recheck sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"resume recheck missed variant: {name}"
      return 1
  IO.println s!"resume_recheck_cases={cases.length}"
  return 0

end HoiminOracle.ResumeRecheck
