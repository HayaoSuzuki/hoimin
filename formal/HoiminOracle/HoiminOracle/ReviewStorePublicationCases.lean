import HoiminOracle.ReviewStorePublicationModel
import Lean.Data.Json

namespace HoiminOracle.ReviewStorePublication

private structure Case where
  id : String
  shared : Bool
  actions : List Action

private def workerName : Worker → String
  | .first => "first"
  | .second => "second"

private def phaseName : Phase → String
  | .idle => "idle"
  | .created => "created"
  | .transactionFirst => "transaction_first"
  | .transactionSecond => "transaction_second"
  | .transactionThird => "transaction_third"
  | .publish => "publish"

private def advanceMany (worker : Worker) (count : Nat) : List Action :=
  List.replicate count (.advance worker)

private def cases : List Case :=
  let workers := [(Worker.first, Worker.second), (.second, .first)]
  let concurrent := [true, false].flatMap fun shared => workers.flatMap fun (winner, loser) =>
    (List.range 5).map fun index =>
      let delay := index + 1
      ⟨s!"race-{shared}-{workerName winner}-other-at-{delay}", shared,
        advanceMany winner 5 ++ advanceMany loser delay ++ advanceMany winner 1 ++ advanceMany loser (6 - delay)⟩
  let alternating := [true, false].flatMap fun shared => workers.map fun (winner, loser) =>
    ⟨s!"alternating-{shared}-{workerName winner}", shared,
      (List.replicate 5 [.advance .first, .advance .second]).flatten ++ [.advance winner, .advance loser]⟩
  let failures := (List.range 5).flatMap fun index => [false, true].flatMap fun unwind =>
    [false, true].map fun hasWinner =>
      let before := advanceMany .first (index + 1)
      let winner := if hasWinner then advanceMany .second 6 else []
      let retry := advanceMany .first (if hasWinner then 1 else 6)
      ⟨s!"abort-{index + 1}-unwind-{unwind}-winner-{hasWinner}", true,
        before ++ winner ++ [.fail .first unwind] ++ retry⟩
  let external := (List.range 6).map fun delay =>
    ⟨s!"foreign-at-{delay}", true, advanceMany .first delay ++ [.installForeign] ++
      advanceMany .first (if delay = 0 then 1 else 6 - delay) ++ [.advance .second]⟩
  concurrent ++ alternating ++ failures ++ external ++
    [⟨"open-after-publication-same-project", true, advanceMany .first 6 ++ [.advance .second]⟩,
     ⟨"open-after-publication-other-project", false, advanceMany .second 6 ++ [.advance .first]⟩]

private def actionJson : Action → Lean.Json
  | .advance worker => Lean.Json.mkObj [("kind", .str "advance"), ("worker", .str (workerName worker))]
  | .fail worker unwind => Lean.Json.mkObj [
      ("kind", .str "fail"), ("worker", .str (workerName worker)), ("unwind", Lean.toJson unwind)]
  | .installForeign => Lean.Json.mkObj [("kind", .str "install_foreign")]

private def replyJson (reply : Reply) : Lean.Json :=
  let kind := fun value => [("kind", Lean.Json.str value)]
  Lean.Json.mkObj (match reply with
    | .reached phase => kind "reached" ++ [("phase", .str (phaseName phase))]
    | .opened revision => kind "opened" ++ [("revision", Lean.toJson revision)]
    | .rejectedForeign => kind "rejected_foreign"
    | .rejectedProject => kind "rejected_project"
    | .failed => kind "failed"
    | .unwound => kind "unwound"
    | .installedForeign => kind "installed_foreign"
    | .invalidSchedule => kind "invalid_schedule")

private def publishedJson : Published → Lean.Json
  | .absent => Lean.Json.mkObj [("kind", .str "absent")]
  | .foreignFile => Lean.Json.mkObj [("kind", .str "foreign_file")]
  | .ready owner history => Lean.Json.mkObj [
    ("kind", .str "ready"),
    ("owner", .str (if owner == .first then "first" else "second")),
    ("history", Lean.toJson (history.map workerName))]

private def observationJson (observation : Observation) : Lean.Json :=
  Lean.Json.mkObj [
    ("reply", replyJson observation.reply), ("published", publishedJson observation.published),
    ("private_count", Lean.toJson observation.privateCount)]

def corpus : String :=
  String.join (cases.map fun item =>
    let observations := replayWith (step item.shared) {} item.actions
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
      ("shared_project", Lean.toJson item.shared), ("actions", .arr (item.actions.toArray.map actionJson)),
      ("observations", .arr (observations.toArray.map observationJson))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Bool → State → Action → State × Reply)) :=
    [ ("publish_at_creation", fun shared state action =>
        let (after, reply) := step shared state action
        match action, reply with
        | .advance worker, .reached .created =>
          ({ after with published := .ready (projectFor shared worker) [] }, reply)
        | _, _ => (after, reply))
    , ("overwrite_existing_publication", fun shared state action =>
        match action with
        | .advance worker =>
          if state.phase worker = .publish then
            let project := projectFor shared worker
            reopen { state.setPhase .idle worker with published := .ready project [] } worker project
          else step shared state action
        | _ => step shared state action)
    , ("loser_uses_private_database", fun shared state action =>
        let (after, reply) := step shared state action
        match action with
        | .advance worker =>
          if state.phase worker = .publish && state.published != .absent then
            ({ after with published := state.published }, .opened 1)
          else (after, reply)
        | _ => (after, reply))
    , ("abort_leaks_private_directory", fun shared state action =>
        match action with
        | .fail _ _ => (state, (step shared state action).2)
        | _ => step shared state action)
    , ("abort_removes_winner", fun shared state action =>
        let (after, reply) := step shared state action
        match action with
        | .fail _ _ => ({ after with published := .absent }, reply)
        | _ => (after, reply))
    , ("ignore_project_at_reopen", fun shared state action =>
        let (after, reply) := step shared state action
        match action, reply, after.published with
        | .advance worker, .rejectedProject, .ready owner _ => reopen after worker owner
        | _, _, _ => (after, reply))
    , ("open_foreign_as_store", fun shared state action =>
        let (after, reply) := step shared state action
        (after, if reply == .rejectedForeign then .opened 1 else reply))
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item =>
        replayWith (variant item.shared) {} item.actions != replayWith (step item.shared) {} item.actions) then
      IO.println s!"review store publication sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"review store publication missed variant: {name}"
      return 1
  IO.println s!"review_store_publication_cases={cases.length}"
  return 0

end HoiminOracle.ReviewStorePublication
