import Std

namespace HoiminOracle.ReduceSpec

inductive Outcome where
  | killed | survived | inconclusive
  deriving Repr, DecidableEq, BEq

structure Result where
  id : Nat
  outcome : Outcome
  deriving Repr, DecidableEq, BEq

structure Span where
  id : Nat
  lo : Nat
  hi : Nat
  deriving Repr, DecidableEq, BEq

structure Identity where
  source : Nat := 1
  plan : Nat := 1
  command : Nat := 1
  environment : Nat := 1
  timeout : Nat := 1000
  variant : Nat := 0
  deriving Repr, DecidableEq, BEq

structure Triple where
  a : Nat
  b : Nat
  c : Nat
  deriving Repr, DecidableEq, BEq

def median (t : Triple) : Nat :=
  t.a + t.b + t.c - min t.a (min t.b t.c) - max t.a (max t.b t.c)

-- One owner for the percentage domain; CLI syntax is deliberately outside
-- this contract. Integer cross-products avoid rounding the discount first.
def validRate (num den : Nat) : Bool := decide (0 < num ∧ num < den)

def pairPass (num den original reduced : Nat) : Bool :=
  validRate num den && decide (0 < original ∧ 0 < reduced ∧
    den * reduced ≤ (den - num) * original)

def finalPass (num den : Nat) (original reduced : Triple) : Bool :=
  pairPass num den original.a reduced.a &&
  pairPass num den original.b reduced.b &&
  pairPass num den original.c reduced.c

def overlaps (a b : Span) : Bool :=
  decide (a.lo < a.hi ∧ b.lo < b.hi ∧ a.lo < b.hi ∧ b.lo < a.hi)

def lookup (results : List Result) (id : Nat) : Option Outcome :=
  (results.find? fun r => r.id == id).map (·.outcome)

def exactResults (selected : List Nat) (before after : List Result) : Bool :=
  decide selected.Nodup && !selected.isEmpty &&
  (before.map (·.id)).isPerm selected && (after.map (·.id)).isPerm selected &&
  before.all (fun r => r.outcome != .inconclusive) &&
  after.all (fun r => r.outcome != .inconclusive) &&
  selected.all (fun id => decide (lookup before id = lookup after id))

structure Context where
  original : List Nat
  keptSpans : List Span
  selected : List Nat
  before : List Result
  identity : Identity := {}
  baselinePass : Bool := true
  sourceDisjoint : Bool := true
  selectorsUnique : Bool := true
  numerator : Nat := 1
  denominator : Nat := 20
  deriving Repr, DecidableEq, BEq

structure Proposal where
  id : Nat
  retained : List Nat
  removed : List Span
  after : List Result
  evidence : Identity
  baselinePass : Bool := true
  exploration : Triple
  originalPairs : Triple
  reducedPairs : Triple
  pairCount : Nat := 3
  completedAt : Nat
  deriving Repr, DecidableEq, BEq

-- Only observed final pairs are checked: an unobserved slot is not a zero
-- duration observation. Exploration timings represent three completed runs.
def measurementsValid (p : Proposal) : Bool :=
  [p.exploration.a, p.exploration.b, p.exploration.c].all (fun n => 0 < n) &&
  ([p.originalPairs.a, p.originalPairs.b, p.originalPairs.c].take p.pairCount).all (fun n => 0 < n) &&
  ([p.reducedPairs.a, p.reducedPairs.b, p.reducedPairs.c].take p.pairCount).all (fun n => 0 < n)

-- Preflight configuration errors never become ordinary "no reduction".
def configValid (ctx : Context) : Bool :=
  validRate ctx.numerator ctx.denominator &&
  ctx.baselinePass && ctx.sourceDisjoint && ctx.selectorsUnique &&
  decide ctx.original.Nodup &&
  exactResults ctx.selected ctx.before ctx.before &&
  ctx.before.any (fun r => r.outcome == .killed)

-- Identity retention and physical edit coverage are checked together, so
-- deleting an ancestor cannot silently bypass a kept nested definition.
def scopeValid (ctx : Context) (p : Proposal) : Bool :=
  !p.removed.isEmpty && decide p.retained.Nodup &&
  decide (p.removed.map (·.id)).Nodup &&
  (p.retained ++ p.removed.map (·.id)).isPerm ctx.original &&
  p.removed.all (fun s => s.lo < s.hi) &&
  ctx.keptSpans.all (fun k => p.retained.contains k.id &&
    p.removed.all (fun r => !overlaps r k))

def sameEvidence (ctx : Context) (p : Proposal) : Bool :=
  decide (p.evidence = { ctx.identity with variant := p.id })

-- Complete evidence for one exact variant. Callers do not reproduce these
-- gates; certifies/step own admission and publication respectively.
def Certified (ctx : Context) (p : Proposal) : Prop :=
  configValid ctx = true ∧ scopeValid ctx p = true ∧
  sameEvidence ctx p = true ∧ p.baselinePass = true ∧
  exactResults ctx.selected ctx.before p.after = true ∧
  measurementsValid p = true ∧
  p.pairCount = 3 ∧ finalPass ctx.numerator ctx.denominator p.originalPairs p.reducedPairs = true

instance (ctx : Context) (p : Proposal) : Decidable (Certified ctx p) := by
  unfold Certified
  infer_instance

def certifies (ctx : Context) (p : Proposal) : Bool := decide (Certified ctx p)

def chooseBest (best : Option Proposal) (p : Proposal) : Option Proposal :=
  match best with
  | none => some p
  | some old => if median p.exploration < median old.exploration then some p else best

structure State where
  now : Nat := 0
  deadline : Nat
  stopped : Bool := false
  best : Option Proposal := none
  deriving Repr, DecidableEq, BEq

structure Event where
  tick : Nat
  proposal : Option Proposal := none
  stop : Bool := false
  deriving Repr, DecidableEq, BEq

-- Candidate certification is atomic. Deadline/stop wins over a simultaneous
-- proposal; already certified artifacts survive the stop, but no work resumes.
-- Unusable observed times stop the run, without an automatic retry policy.
def step (ctx : Context) (s : State) (e : Event) : State :=
  if s.stopped then s
  else if configValid ctx = false ∨ e.tick < s.now ∨ s.deadline ≤ e.tick ∨ e.stop = true then
    { s with now := max s.now e.tick, stopped := true }
  else match e.proposal with
    | none => { s with now := e.tick }
    | some p =>
      if measurementsValid p = false then
        { s with now := e.tick, stopped := true }
      else if Certified ctx p ∧ p.completedAt = e.tick then
        { s with now := e.tick, best := chooseBest s.best p }
      else { s with now := e.tick }

def run (ctx : Context) : State → List Event → State
  | s, [] => s
  | s, e :: rest => run ctx (step ctx s e) rest

def Safe (ctx : Context) (s : State) : Prop :=
  ∀ p, s.best = some p → Certified ctx p ∧ p.completedAt < s.deadline

end HoiminOracle.ReduceSpec
