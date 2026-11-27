import HoiminOracle.PlanVerifyModel
import Lean.Data.Json

namespace HoiminOracle.PlanVerify

private def entry (id : Nat) (stored : Stored := .valid) : Entry :=
  ⟨id, if id == 1 then 1 else 0, stored⟩

private def allEntries : List Entry := [entry 0, entry 1, entry 2]
private def calcTarget : Target := ⟨0, [0, 2]⟩
private def other : Target := ⟨1, [1]⟩
private def absent : Target := ⟨2, []⟩
private def targets : List Target := [other, absent, calcTarget]

private structure Case where
  id : String
  entries : List Entry := allEntries
  requested : List Nat := [2, 1, 0, 2]
  slices : List Target := targets
  readable : List Nat := [0, 1]
  live : Bool := false

private def input (item : Case) : Input :=
  ⟨item.entries, item.requested, item.slices, item.readable⟩

private def storedName : Stored → String
  | .missing => "missing"
  | .valid => "valid"
  | .hash => "hash"
  | .original => "original"
  | .stable => "stable"
  | .symbol => "symbol"
  | .sequence => "sequence"

private def reasonName : Reason → String
  | .unknown => "unknown"
  | .unselected => "unselected"
  | .unreadable => "unreadable"
  | .hash => "hash"
  | .original => "original"
  | .stable => "stable"
  | .undiscovered => "undiscovered"
  | .descriptor => "descriptor"

private def cases : List Case :=
  let statuses := [Stored.valid, .missing, .hash, .original, .stable, .symbol]
  let matrix := statuses.flatMap fun first => statuses.map fun middle =>
    { id := s!"entries-{storedName first}-{storedName middle}"
      entries := [entry 2, entry 1 middle, entry 0 first] }
  matrix ++
  [ { id := "same-file-checked-once", requested := [2, 0, 2] }
  , { id := "ignore-unrequested-missing-entry", entries := [entry 0 .missing, entry 1, entry 2], requested := [2] }
  , { id := "ignore-unrequested-malformed-entry", entries := [entry 0 .hash, entry 1, entry 2], requested := [2] }
  , { id := "preserve-saved-sequence-independence", entries := [entry 0 .sequence, entry 1, entry 2 .sequence] }
  , { id := "missing-target-precedes-unreadable", slices := [calcTarget, absent], readable := [0] }
  , { id := "unreadable-selected-target", readable := [0] }
  , { id := "unreadable-first-target", readable := [1] }
  , { id := "unselected-first-candidate", slices := [other, absent] }
  , { id := "cached-later-failure-after-middle",
      entries := [entry 0, entry 1 .missing, entry 2 .original] }
  , { id := "cached-later-failure-when-middle-valid",
      entries := [entry 0, entry 1, entry 2 .original] }
  , { id := "missing-before-descriptor-in-discovery",
      entries := [entry 0, entry 1 .symbol, entry 2], slices := [other, ⟨0, [2]⟩, absent] }
  , { id := "descriptor-before-missing-in-discovery",
      entries := [entry 0 .symbol, entry 1, entry 2], slices := [⟨1, []⟩, calcTarget, absent] }
  , { id := "missing-before-same-id-descriptor",
      entries := [entry 0 .symbol, entry 1, entry 2], slices := [other, ⟨0, [2]⟩, absent] }
  , { id := "ignore-unrequested-descriptor",
      entries := [entry 0 .symbol, entry 1, entry 2], requested := [2] }
  , { id := "public-discovery-order", slices := [calcTarget, other], live := true }
  , { id := "public-single-candidate", slices := [calcTarget, other], requested := [1], live := true }
  , { id := "public-unknown-id", requested := [1, 3], slices := [calcTarget, other], live := true }
  , { id := "public-descriptor", entries := [entry 0 .symbol, entry 1, entry 2],
      slices := [calcTarget, other], live := true }
  , { id := "public-original", entries := [entry 0 .original, entry 1, entry 2],
      slices := [calcTarget, other], live := true }
  ]

private def observation (observed : Observation) : Lean.Json :=
  let result := match observed.result with
    | .ok ids => Lean.Json.mkObj [("confirmed", Lean.toJson ids)]
    | .error failure => Lean.Json.mkObj
        [("id", Lean.toJson failure.id), ("reason", .str (reasonName failure.reason))]
  Lean.Json.mkObj [("contexts", Lean.toJson observed.contexts), ("result", result)]

def corpus : String :=
  String.join (cases.map fun item =>
    let entries := item.entries.map fun row =>
      Lean.Json.mkObj [("id", Lean.toJson row.id), ("stored", .str (storedName row.stored))]
    let targets := item.slices.map fun slice =>
      Lean.Json.mkObj [("path", Lean.toJson slice.path), ("discovery", Lean.toJson slice.discovery)]
    (Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("id", .str item.id), ("live", Lean.toJson item.live),
      ("entries", .arr entries.toArray), ("targets", .arr targets.toArray),
      ("requested", Lean.toJson item.requested), ("readable", Lean.toJson item.readable),
      ("expected", observation (verify (input item)))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let reference := fun item => verify (input item)
  let variants : List (String × (Case → Observation)) :=
    [ ("inspect_in_request_order", fun item =>
        let inspected := inspect (input item) [] item.requested
        match inspected.failure with
        | some failure => ⟨inspected.checkedPaths.length, .error failure⟩
        | none => reference item)
    , ("skip_stored_validation", fun item =>
        verify (input { item with entries := item.entries.map fun row =>
          if row.stored == .hash || row.stored == .original || row.stored == .stable then
            { row with stored := .valid } else row }))
    , ("check_file_before_target", fun item =>
        let observed := reference item
        match observed.result with
        | .error ⟨id, .unselected⟩ =>
          if (lookup item.entries id).any (fun row => !item.readable.contains row.path) then
            { observed with result := .error ⟨id, .unreadable⟩ }
          else observed
        | _ => observed)
    , ("rebuild_context_for_each_request", fun item =>
        let observed := reference item
        match observed.result with
        | .ok _ => { observed with contexts := (requestedIds item.requested).length }
        | _ => observed)
    , ("respect_only_stable_id", fun item =>
        verify (input { item with entries := item.entries.map fun row =>
          if row.stored == .symbol then { row with stored := .valid } else row }))
    , ("accept_missing_discovery", fun item =>
        let observed := reference item
        match observed.result with
        | .error ⟨_, .undiscovered⟩ => { observed with result := .ok (requestedIds item.requested) }
        | _ => observed)
    , ("emit_requested_id_order", fun item =>
        let observed := reference item
        match observed.result with
        | .ok _ => { observed with result := .ok (requestedIds item.requested) }
        | _ => observed)
    , ("emit_unrequested_candidates", fun item =>
        let observed := reference item
        let inspected := inspect (input item) [] (requestedIds item.requested)
        let allFound := (projectedTargets (input item) inspected.checkedPaths).flatMap Target.discovery
        match observed.result with
        | .ok _ => { observed with result := .ok allFound }
        | _ => observed)
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => (observation (reference item)).compress !=
        (observation (variant item)).compress) then
      IO.println s!"plan verify sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"plan verify missed variant: {name}"
      return 1
  IO.println s!"plan_verify_cases={cases.length} public={(cases.filter Case.live).length}"
  return 0

end HoiminOracle.PlanVerify
