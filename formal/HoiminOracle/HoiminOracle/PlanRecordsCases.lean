import HoiminOracle.PlanRecordsModel
import Lean.Data.Json

namespace HoiminOracle.PlanRecords

private def hash (digit : Char) : String := String.ofList (List.replicate 64 digit)

private structure Case where
  id : String
  saved : Inventory := {}
  current : Inventory := {}
  live : Bool := false

private def slots : List (List Record) :=
  let choices := [none, some (hash 'a'), some (hash 'b')]
  choices.flatMap fun first =>
    choices.map fun second =>
      [("pkg.py", first), ("pkg/a.py", second)].filterMap fun (path, value) =>
        value.map (Record.mk path)

private def matrix : List Case :=
  slots.zipIdx.flatMap fun (saved, a) =>
    slots.zipIdx.flatMap fun (current, b) =>
      [ ⟨s!"source-{a}-{b}", { sources := saved }, { sources := current.reverse }, false⟩
      , ⟨s!"input-{a}-{b}", { inputs := saved.reverse }, { inputs := current }, false⟩
      ]

private def boundary (count : Nat) : Case :=
  let changed := (List.range count).map fun n =>
    ⟨s!"p/{if n < 10 then "0" else ""}{n}.py", hash 'a'⟩
  let unchanged : Record := ⟨"p/-unchanged.py", hash 'a'⟩
  let saved := (changed.zipIdx.filterMap fun (row, n) =>
    if n % 3 == 2 then none else some row).reverse
  let current := changed.zipIdx.filterMap fun (row, n) =>
    if n % 3 == 0 then none else some { row with hash := hash 'b' }
  ⟨s!"mixed-cap-{count}", { sources := unchanged :: saved }, { sources := current ++ [unchanged] }, false⟩

private def rejected : List Case :=
  let base : Record := ⟨"pkg/a.py", hash 'a'⟩
  let badPaths := ["", "/absolute", "p/../a.py", "p/./a.py", "p//a.py", "p/",
    "p\\a.py", "p:a.py", "p\x00.py", "p\n/a/../b.py"]
  let badHashes := ["", String.ofList (List.replicate 63 'a'), hash 'A', hash 'g',
    String.ofList (List.replicate 32 'é')]
  (badPaths.zipIdx.map fun (path, n) =>
    ⟨s!"path-{n}", { sources := [base, ⟨path, "invalid"⟩] }, {}, false⟩) ++
  (badHashes.zipIdx.map fun (value, n) =>
    ⟨s!"hash-before-duplicate-{n}", { inputs := [base, { base with hash := value }] }, {}, false⟩) ++
  [ ⟨"duplicate-same-digest", { sources := [base, base] }, {}, false⟩
  , ⟨"duplicate-different-digest", { inputs := [base, { base with hash := hash 'b' }] }, {}, false⟩
  , ⟨"first-invalid-record", { sources := [⟨"first.py", "x"⟩, ⟨"../second.py", hash 'a'⟩] }, {}, false⟩
  , ⟨"invalid-source-before-invalid-input",
      { sources := [⟨"first.py", "x"⟩], inputs := [⟨"../second.py", hash 'a'⟩] }, {}, false⟩
  , ⟨"invalid-record-after-visible-staleness",
      { sources := [base], inputs := [⟨"../bad.txt", "x"⟩] }, {}, false⟩
  , ⟨"same-path-in-different-inventories",
      { sources := [base], inputs := [base] }, { sources := [base], inputs := [base] }, false⟩
  , ⟨"newline-in-invalid-hash-diagnostic", { inputs := [⟨"one\ntwo.txt", "x"⟩] }, {}, false⟩
  ]

private def liveCurrent : Inventory :=
  { sources := [⟨"src/calc.py", hash 'a'⟩], inputs := [⟨"support.cfg", hash 'b'⟩] }

private def liveCases : List Case :=
  let both : Inventory :=
    { sources := liveCurrent.sources.map (fun row => { row with hash := hash 'c' })
      inputs := liveCurrent.inputs.map (fun row => { row with hash := hash 'd' }) }
  let extraSources := (List.range 12).map fun n => ⟨s!"retired/{n}.py", hash 'd'⟩
  [ ⟨"workspace-matches", liveCurrent, liveCurrent, true⟩
  , ⟨"workspace-source-and-input-modified", both, liveCurrent, true⟩
  , ⟨"workspace-source-added", { liveCurrent with sources := [] }, liveCurrent, true⟩
  , ⟨"workspace-many-sources-removed", { liveCurrent with sources := extraSources ++ liveCurrent.sources },
      liveCurrent, true⟩
  , ⟨"workspace-input-modified", { liveCurrent with inputs := both.inputs }, liveCurrent, true⟩
  , ⟨"workspace-input-added", { liveCurrent with inputs := [] }, liveCurrent, true⟩
  , ⟨"workspace-input-removed", { liveCurrent with inputs := ⟨"gone.txt", hash 'd'⟩ :: liveCurrent.inputs },
      liveCurrent, true⟩
  , ⟨"workspace-invalid-source-path",
      { both with sources := [⟨"src/./calc.py", hash 'a'⟩] }, liveCurrent, true⟩
  , ⟨"workspace-invalid-input-before-stale-source",
      { both with inputs := liveCurrent.inputs ++ liveCurrent.inputs }, liveCurrent, true⟩
  ]

private def cases : List Case :=
  matrix ++ ([0, 9, 10, 11, 12, 17, 21].map boundary) ++ rejected ++ liveCases ++
  [ ⟨"diagnostic-escapes",
      { sources := [⟨"pkg.py", hash 'a'⟩, ⟨"package/日本語.py", hash 'a'⟩,
        ⟨"pkg/controls\n\r\t\"\x1b.py", hash 'a'⟩] },
      { sources := [⟨"pkg/calc.py", hash 'a'⟩, ⟨"pkg.py", hash 'b'⟩] }, false⟩
  ]

private def kindName : Kind → String
  | .source => "source"
  | .fingerprintInput => "fingerprint_input"

private def issueName : Invalid → String
  | .path => "path"
  | .hash => "hash"
  | .duplicate => "duplicate"

private def changeName : ChangeKind → String
  | .added => "added"
  | .removed => "removed"
  | .modified => "modified"

private def recordJson (record : Record) : Lean.Json :=
  Lean.Json.mkObj [("path", .str record.path), ("hash", .str record.hash)]

private def inventoryJson (inventory : Inventory) : Lean.Json :=
  Lean.Json.mkObj [("sources", .arr (inventory.sources.map recordJson).toArray),
    ("inputs", .arr (inventory.inputs.map recordJson).toArray)]

private def outcomeJson : Outcome → Lean.Json
  | .ok => Lean.Json.mkObj [("result", .str "ok")]
  | .invalid failure => Lean.Json.mkObj [("result", .str "invalid"),
    ("kind", .str (kindName failure.kind)), ("issue", .str (issueName failure.issue)), ("path", .str failure.path)]
  | .stale kind summary =>
    let displayed := summary.displayed.map fun change =>
      Lean.Json.mkObj [("path", .str change.path), ("kind", .str (changeName change.kind))]
    Lean.Json.mkObj [("result", .str "stale"), ("kind", .str (kindName kind)),
      ("displayed", .arr displayed.toArray), ("omitted", Lean.toJson summary.omitted)]

def corpus : String :=
  String.join (cases.map fun item => (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str item.id), ("live", Lean.toJson item.live),
    ("saved", inventoryJson item.saved), ("current", inventoryJson item.current),
    ("expected", outcomeJson (verify item.saved item.current))]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let reference := fun item : Case => verify item.saved item.current
  let variants : List (String × (Case → Outcome)) :=
    [ ("skip_saved_validation", fun item => compare item.saved item.current)
    , ("validate_inputs_first", fun item =>
        match validateFrom .fingerprintInput [] item.saved.inputs with
        | some failure => .invalid failure
        | none => reference item)
    , ("check_hash_before_path", fun item =>
        match reference item with
        | .invalid ⟨kind, .path, path⟩ =>
          let rows := match kind with
            | .source => item.saved.sources
            | .fingerprintInput => item.saved.inputs
          if rows.any (fun row => row.path == path && !validHash row.hash) then
            .invalid ⟨kind, .hash, path⟩
          else reference item
        | other => other)
    , ("ignore_duplicate_paths", fun item =>
        match reference item with
        | .invalid ⟨_, .duplicate, _⟩ => compare item.saved item.current
        | other => other)
    , ("ignore_change_to_existing_digest", fun item =>
        let align := fun (before after : List Record) =>
          before.map (fun row => { row with hash := (digest after row.path).getD row.hash })
        verify { sources := align item.saved.sources item.current.sources
                 inputs := align item.saved.inputs item.current.inputs } item.current)
    , ("compare_inputs_first", fun item =>
        match validateSaved item.saved, changes item.saved.inputs item.current.inputs with
        | none, first :: rest => .stale .fingerprintInput (summarize (first :: rest))
        | _, _ => reference item)
    , ("use_raw_path_order", fun item =>
        match reference item with
        | .stale kind _ =>
          let rows := match kind with
            | .source => changes item.saved.sources item.current.sources
            | .fingerprintInput => changes item.saved.inputs item.current.inputs
          .stale kind (summarize (rows.mergeSort (fun a b => a.path ≤ b.path)))
        | other => other)
    , ("omit_at_exact_limit", fun item =>
        match reference item with
        | .stale kind summary =>
          if summary.displayed.length == displayLimit && summary.omitted == 0 then
            .stale kind ⟨summary.displayed.take (displayLimit - 1), 1⟩
          else .stale kind summary
        | other => other)
    , ("omit_count_lost", fun item =>
        match reference item with
        | .stale kind summary => .stale kind { summary with omitted := 0 }
        | other => other)
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ reference item)) then
      IO.println s!"plan records sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"plan records missed variant: {name}"
      return 1
  IO.println s!"plan_record_cases={cases.length} live={(cases.filter Case.live).length}"
  return 0

end HoiminOracle.PlanRecords
