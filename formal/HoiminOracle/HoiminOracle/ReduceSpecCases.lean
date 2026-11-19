import HoiminOracle.ReduceSpecProofs

namespace HoiminOracle.ReduceSpec

def triple (n : Nat) : Triple := ⟨n, n, n⟩
def baseContext : Context := {
  original := [1,2,3], keptSpans := [⟨3,20,30⟩], selected := [1,2],
  before := [⟨1,.killed⟩,⟨2,.survived⟩] }
def good : Proposal := {
  id := 1, retained := [2,3], removed := [⟨1,0,10⟩],
  after := baseContext.before, evidence := { variant := 1 },
  exploration := triple 90, originalPairs := triple 100,
  reducedPairs := triple 90, completedAt := 1 }
def faster : Proposal := { good with
  id := 2, retained := [1,3], removed := [⟨2,10,20⟩],
  evidence := { variant := 2 }, exploration := triple 80,
  reducedPairs := triple 80, completedAt := 2 }
def nested : Proposal := { good with removed := [⟨1,0,30⟩] }
def pending : Proposal := { faster with pairCount := 2, completedAt := 3 }

-- A deterministic toy runner: definitions 1 and 2 each kill mutant 1;
-- definition 3 is kept but does not kill it. Costs are deliberately not additive.
def redundantContext : Context := { baseContext with
  selected := [1], before := [⟨1,.killed⟩] }
def redundantOutcomes (retained : List Nat) : List Result :=
  [⟨1, if retained.contains 1 || retained.contains 2 then .killed else .survived⟩]
def redundantA : Proposal := { good with after := redundantOutcomes [2,3] }
def redundantB : Proposal := { good with retained := [1,3], removed := [⟨2,10,20⟩], after := redundantOutcomes [1,3] }
def redundantBoth : Proposal := { good with retained := [3], removed := [⟨1,0,10⟩,⟨2,10,20⟩], after := redundantOutcomes [3] }

-- Cases are data, never imported by the library root; no exhaustive work in proofs.
def certificationCases : List (String × Context × Proposal) := [
  ("individually-redundant-a", redundantContext, redundantA),
  ("individually-redundant-b", redundantContext, redundantB),
  ("joint-removal-loses-kill", redundantContext, redundantBoth),
  ("valid", baseContext, good),
  ("boundary-five-percent", baseContext, { good with reducedPairs := triple 95 }),
  ("permuted-results", baseContext, { good with after := good.after.reverse }),
  ("group-removal", baseContext, { good with retained := [3], removed := [⟨1,0,10⟩,⟨2,10,20⟩] }),
  ("lost-kill", baseContext, { good with after := [⟨1,.survived⟩,⟨2,.survived⟩] }),
  ("new-kill", baseContext, { good with after := [⟨1,.killed⟩,⟨2,.killed⟩] }),
  ("same-count-different-ids", baseContext, { good with after := [⟨3,.killed⟩,⟨2,.survived⟩] }),
  ("missing-result", baseContext, { good with after := [⟨1,.killed⟩] }),
  ("duplicate-result", baseContext, { good with after := [⟨1,.killed⟩,⟨1,.killed⟩] }),
  ("inconclusive", baseContext, { good with after := [⟨1,.killed⟩,⟨2,.inconclusive⟩] }),
  ("candidate-baseline-failed", baseContext, { good with baselinePass := false }),
  ("original-baseline-failed", { baseContext with baselinePass := false }, good),
  ("fingerprint-overlap", { baseContext with sourceDisjoint := false }, good),
  ("ambiguous-selector", { baseContext with selectorsUnique := false }, good),
  ("no-original-kill", { baseContext with before := [⟨1,.survived⟩,⟨2,.survived⟩] },
    { good with after := [⟨1,.survived⟩,⟨2,.survived⟩] }),
  ("empty-selection", { baseContext with selected := [], before := [] }, { good with after := [] }),
  ("duplicate-selection", { baseContext with selected := [1,1] }, good),
  ("direct-keep-removal", baseContext, { good with retained := [1,2], removed := [⟨3,20,30⟩] }),
  ("ancestor-erases-keep", baseContext, nested),
  ("adjacent-span", baseContext, { good with removed := [⟨1,0,20⟩] }),
  ("empty-span", baseContext, { good with removed := [⟨1,10,10⟩] }),
  ("no-removal", baseContext, { good with retained := [1,2,3], removed := [] }),
  ("unknown-definition", baseContext, { good with removed := [⟨4,0,10⟩] }),
  ("duplicate-retained", baseContext, { good with retained := [2,2,3] }),
  ("wrong-variant", baseContext, { good with evidence := { good.evidence with variant := 2 } }),
  ("wrong-source", baseContext, { good with evidence := { good.evidence with source := 2 } }),
  ("wrong-plan", baseContext, { good with evidence := { good.evidence with plan := 2 } }),
  ("wrong-command", baseContext, { good with evidence := { good.evidence with command := 2 } }),
  ("wrong-environment", baseContext, { good with evidence := { good.evidence with environment := 2 } }),
  ("timeout-recomputed", baseContext, { good with evidence := { good.evidence with timeout := 900 } }),
  ("unfinished-final-confirmation", baseContext, pending),
  ("unobserved-pair-slots", baseContext, { good with pairCount := 2, originalPairs := ⟨100,100,0⟩, reducedPairs := ⟨90,90,0⟩ }),
  ("third-pair-regresses", baseContext, { good with reducedPairs := ⟨90,90,101⟩ }),
  ("zero-exploration-tick", baseContext, { good with exploration := ⟨0,90,90⟩ }),
  ("zero-original-tick", baseContext, { good with originalPairs := ⟨0,100,100⟩ }),
  ("zero-reduced-tick", baseContext, { good with reducedPairs := ⟨0,90,90⟩ }),
  ("zero-rate", { baseContext with numerator := 0 }, good),
  ("hundred-percent", { baseContext with numerator := 20 }, good),
  ("zero-denominator", { baseContext with denominator := 0 }, good),
  ("above-hundred-percent", { baseContext with numerator := 21 }, good),
  ("above-u64-duration", baseContext, { good with originalPairs := triple (2^64), reducedPairs := triple (2^63) }) ]

def alphabet : List Event := [
  { tick := 1, proposal := some good },
  { tick := 2, proposal := some faster },
  { tick := 3, proposal := some pending },
  { tick := 2, proposal := some { faster with evidence := { variant := 99 } } },
  { tick := 2, proposal := some { faster with exploration := ⟨0,80,80⟩ } },
  { tick := 3, stop := true },
  { tick := 4, proposal := some { faster with completedAt := 4 } } ]

def traces : Nat → List (List Event)
  | 0 => [[]]
  | n+1 => alphabet.flatMap fun e => (traces n).map (e :: ·)

def tracesUpTo (depth : Nat) : List (List Event) :=
  (List.range (depth+1)).flatMap traces

def stateSafe (s : State) : Bool := match s.best with
  | none => true
  | some p => certifies baseContext p && p.completedAt < s.deadline

-- Deliberately broken implementations belong only to this audit boundary.
def brokenEarlyPublish (s : State) (e : Event) : State :=
  match e.proposal with
  | some p => { s with now := e.tick, best := some p }
  | none => s

def runBroken : State → List Event → State
  | s, [] => s
  | s, e :: rest => runBroken (brokenEarlyPublish s e) rest

def firstBrokenTrace (depth : Nat) : Option (List Event) :=
  (tracesUpTo depth).find? fun t => !stateSafe (runBroken {deadline:=4} t)

def killSubsetOnly (before after : List Result) : Bool :=
  before.all fun r => r.outcome != .killed || lookup after r.id == some .killed

def rawPairPass (num den original reduced : Nat) : Bool :=
  den * reduced ≤ (den-num) * original

def subtractRoundedDiscount (num den original reduced : Nat) : Bool :=
  reduced ≤ original - original * num / den

def sensitivity : List (String × Bool × Bool) := [
  ("atomicity/publish-before-complete", stateSafe (step baseContext {deadline:=4}
    {tick:=3,proposal:=some pending}), stateSafe (brokenEarlyPublish {deadline:=4}
    {tick:=3,proposal:=some pending})),
  ("identity/reused-variant-certificate", certifies baseContext
    { good with evidence := { variant := 99 } }, certifies baseContext good),
  ("provenance/recomputed-timeout", certifies baseContext
    { good with evidence := { good.evidence with timeout := 900 } },
    certifies { baseContext with identity := { baseContext.identity with timeout := 900 } }
      { good with evidence := { good.evidence with timeout := 900 } }),
  ("uniqueness/duplicate-results", exactResults [1,2] baseContext.before
    [⟨1,.killed⟩,⟨1,.killed⟩], decide (baseContext.before.length = 2)),
  ("preservation/ignore-new-kill", exactResults [1,2] baseContext.before
    [⟨1,.killed⟩,⟨2,.killed⟩], killSubsetOnly baseContext.before [⟨1,.killed⟩,⟨2,.killed⟩]),
  ("composition/union-individual-removals", certifies redundantContext redundantBoth,
    certifies redundantContext redundantA && certifies redundantContext redundantB),
  ("retention/ancestor-erases-keep", scopeValid baseContext nested,
    scopeValid {baseContext with keptSpans:=[]} nested),
  ("timing/median-hides-pair", finalPass 1 20 (triple 100) ⟨90,90,101⟩,
    pairPass 1 20 100 (median ⟨90,90,101⟩)),
  ("boundary/zero-rate-equal-time", pairPass 0 20 1 1, rawPairPass 0 20 1 1),
  ("configuration/zero-rate-preflight", configValid { baseContext with numerator := 0 }, configValid baseContext),
  ("measurement/zero-time-ignored", (step baseContext {deadline:=4}
    {tick:=1,proposal:=some {good with exploration:=⟨0,90,90⟩}}).stopped,
    (brokenEarlyPublish {deadline:=4} {tick:=1,proposal:=some {good with exploration:=⟨0,90,90⟩}}).stopped),
  ("boundary/hundred-percent-zero-time", pairPass 20 20 1 0, rawPairPass 20 20 1 0),
  ("arithmetic/rounded-discount", pairPass 1 20 1 1, subtractRoundedDiscount 1 20 1 1),
  ("precedence/deadline-equality", (step baseContext {deadline:=1}
    {tick:=1,proposal:=some good}).best.isSome,
    (brokenEarlyPublish {deadline:=1} {tick:=1,proposal:=some good}).best.isSome) ]

-- Fixed witnesses are kernel-checked with finite computation; exhaustive
-- enumeration and serialization remain in ReduceSpecAuditMain.
set_option maxHeartbeats 20000 in
example : certifies baseContext good = true := by decide
set_option maxHeartbeats 20000 in
example : scopeValid baseContext nested = false := by decide
set_option maxHeartbeats 20000 in
example : (step baseContext {deadline:=1} {tick:=1,proposal:=some good}).best = none := by decide
set_option maxHeartbeats 20000 in
example : sensitivity.all (fun (_, goodValue, brokenValue) => goodValue != brokenValue) = true := by decide
set_option maxHeartbeats 20000 in
example : firstBrokenTrace 1 = some [{tick:=3,proposal:=some pending}] := by decide
set_option maxHeartbeats 20000 in
example : configValid { baseContext with numerator := 0 } = false := by decide
set_option maxHeartbeats 20000 in
example : measurementsValid {good with pairCount:=2, originalPairs:=⟨100,100,0⟩, reducedPairs:=⟨90,90,0⟩} = true := by decide
set_option maxHeartbeats 20000 in
example : (step baseContext {deadline:=4} {tick:=1,proposal:=some {good with exploration:=⟨0,90,90⟩}}).stopped = true := by decide

end HoiminOracle.ReduceSpec
