import HoiminOracle.ReduceSpecCases
import Lean.Data.Json

open Lean HoiminOracle.ReduceSpec

deriving instance ToJson for Outcome
deriving instance ToJson for Result
deriving instance ToJson for Span
deriving instance ToJson for Identity
deriving instance ToJson for Triple
deriving instance ToJson for Context
deriving instance ToJson for Proposal
deriving instance ToJson for Event

def row (kind id : String) (input expected : Json) : Json := Json.mkObj [
  ("schema", toJson (1:Nat)), ("mode", toJson "model-only"),
  ("kind", toJson kind), ("id", toJson id), ("contract", toJson "agreed"),
  ("input", input), ("expected", expected)]

def certificationRow (c : String × Context × Proposal) : Json :=
  let (id, ctx, p) := c
  row "certification" id
    (Json.mkObj [("context",toJson ctx),("proposal",toJson p)])
    (Json.mkObj [("certified",toJson (certifies ctx p)),
      ("config_valid",toJson (configValid ctx)),("scope_valid",toJson (scopeValid ctx p)),
      ("same_evidence",toJson (sameEvidence ctx p)),
      ("measurements_valid",toJson (measurementsValid p)),
      ("exact_results",toJson (exactResults ctx.selected ctx.before p.after)),
      ("exploration_median",toJson (median p.exploration)),
      ("final_pairs_pass",toJson (finalPass ctx.numerator ctx.denominator p.originalPairs p.reducedPairs))])

def timingInputs : List (Nat × Nat × Nat × Nat) :=
  [0,1,20,21].flatMap fun n => [0,1,20,100].flatMap fun o =>
    [0,1,19,20,94,95,96,100,101].map fun r => (n,20,o,r)

def pairRow (id : String) (n d o r : Nat) : Json :=
  row "pair" id
    (Json.mkObj [("numerator",toJson n),("denominator",toJson d),
      ("original",toJson o),("reduced",toJson r)])
    (toJson (pairPass n d o r))

def timingRows : List Json := timingInputs.map fun (n,d,o,r) =>
  pairRow s!"pair-{n}-{d}-{o}-{r}" n d o r

def fullWidthTimingRows : List Json := fullWidthTimingInputs.map fun (id,n,d,o,r) =>
  pairRow s!"pair-full-width-{id}" n d o r

def matrixRows : List Json :=
  let outcomes := [Outcome.killed,.survived,.inconclusive]
  let results := outcomes.flatMap fun a => outcomes.map fun b => [Result.mk 1 a,Result.mk 2 b]
  (results.zipIdx).flatMap fun (before,i) => (results.zipIdx).map fun (after,j) =>
    row "results" s!"results-{i}-{j}"
      (Json.mkObj [("selected",toJson ([1,2]:List Nat)),("before",toJson before),("after",toJson after)])
      (toJson (exactResults [1,2] before after))

def medianRows : List Json :=
  [1,2,3].flatMap fun a => [1,2,3].flatMap fun b => [1,2,3].map fun c =>
    row "median" s!"median-{a}-{b}-{c}" (toJson (Triple.mk a b c)) (toJson (median ⟨a,b,c⟩))

def overlapRows : List Json :=
  (List.range 4).flatMap fun a => (List.range 4).flatMap fun b =>
  (List.range 4).flatMap fun c => (List.range 4).map fun d =>
    row "overlap" s!"overlap-{a}-{b}-{c}-{d}"
      (Json.mkObj [("removed",toJson (Span.mk 1 a b)),("kept",toJson (Span.mk 2 c d))])
      (toJson (overlaps ⟨1,a,b⟩ ⟨2,c,d⟩))

def traceRow (id : String) (events : List Event) : Json :=
  let states := events.scanl (step baseContext) {deadline:=4}
  row "trace" id (Json.mkObj [
    ("context",toJson baseContext),("deadline",toJson (4:Nat)),("events",toJson events)])
    (toJson (states.map fun s => Json.mkObj [
      ("now",toJson s.now),("stopped",toJson s.stopped),
      ("best",toJson (s.best.map (·.id))),("safe",toJson (stateSafe s))]))

def corpus (depth : Nat) : String :=
  let indexedTraces := (tracesUpTo depth).zipIdx
  let traceRows := indexedTraces.map fun (events, index) => traceRow s!"trace-{index}" events
  let retentionRows := bestRetentionCases.map fun (id, events) => traceRow s!"trace-best-{id}" events
  String.join <|
    ((certificationCases.map certificationRow) ++ timingRows ++ matrixRows ++ medianRows ++
      overlapRows ++ traceRows ++ fullWidthTimingRows ++ retentionRows).map
        (fun (j : Json) => j.compress ++ "\n")

def checkAudit (depth : Nat) : IO Unit := do
  unless sensitivity.all (fun (_,a,b) => a != b) do
    throw (IO.userError "broken reduce variant was not detected")
  unless (firstBrokenTrace depth).map List.length == some 1 do
    throw (IO.userError "shortest-first broken-state witness missing")
  unless (tracesUpTo depth).all (fun t => stateSafe (run baseContext {deadline:=4} t)) do
    throw (IO.userError "bounded reduce state invariant failed")
  -- Positive control prevents an always-reject model from passing sensitivity.
  unless certifies baseContext good && certifies baseContext faster do
    throw (IO.userError "reduce positive control failed")
  unless (run baseContext {deadline:=4}
    [{tick:=1,proposal:=some good},{tick:=2,proposal:=some faster}]).best == some faster do
    throw (IO.userError "best certified candidate was not retained")

def main (rawArgs : List String) : IO Unit := do
  let args := if rawArgs.head? == some "--" then rawArgs.drop 1 else rawArgs
  let depth := match args with
    | ["--stats",value] => value.toNat?.getD 0
    | _ => 3
  unless depth > 0 && depth ≤ 3 do throw (IO.userError "supported depth: 1..3")
  checkAudit depth
  match args with
  | ["--output",path] => IO.FS.writeFile path (corpus depth)
  | ["--check",path] =>
    unless (← IO.FS.readFile path) == corpus depth do
      throw (IO.userError "stale reduce specification corpus")
  | ["--sensitivity"] =>
    for (name,a,b) in sensitivity do IO.println s!"{name}: correct={a} broken={b} detected={a != b}"
  | ["--stats"] | ["--stats",_] =>
    let ts := tracesUpTo depth
    let pairs := timingRows.length + fullWidthTimingRows.length
    let rows := certificationCases.length + pairs + matrixRows.length + medianRows.length +
      overlapRows.length + ts.length + bestRetentionCases.length
    IO.println s!"depth={depth} alphabet={alphabet.length} traces={ts.length} transitions={(ts.map List.length).foldl (·+·) 0} certification={certificationCases.length} pairs={pairs} results={matrixRows.length} medians={medianRows.length} overlaps={overlapRows.length} bestRetention={bestRetentionCases.length} rows={rows} sensitivity={sensitivity.length}"
  | _ => throw (IO.userError "--output PATH | --check PATH | --stats [1..3] | --sensitivity")
