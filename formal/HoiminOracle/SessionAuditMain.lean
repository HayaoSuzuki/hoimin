import HoiminOracle.SessionCases

open HoiminOracle.SessionAudit

namespace HoiminOracle.SessionAudit.Executable

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq

def eventAlphabet : List Event := [
  .open .h0,
  .open .h1,
  .begin .h0 .r0 .f0,
  .begin .h0 .r1 .f0,
  .begin .h1 .r0 .f0,
  .begin .h1 .r1 .f1,
  .load .h0 .f0,
  .load .h0 .f1,
  .load .h1 .f0,
  .load .h1 .f1,
  .persist .h0 .r0 .m0 .killed .p0 .valid,
  .persist .h0 .r0 .m0 .survived .p0 .valid,
  .persist .h0 .r0 .m0 .timeout .p0 .valid,
  .persist .h0 .r0 .m0 .outOfMemory .p0 .valid,
  .persist .h0 .r0 .m0 .processLimit .p0 .valid,
  .persist .h0 .r0 .m0 .error .p0 .valid,
  .persist .h0 .r0 .m0 .notRun .p0 .valid,
  .persist .h0 .r0 .m0 .killed .p1 .invalidDiagnostic,
  .finish .h0 .r0 false,
  .finish .h0 .r0 true,
  .finish .h1 .r0 false,
  .finish .h1 .r0 true,
  .drop .h0,
  .drop .h1,
  .crash .h0,
  .loadReadCandidate .h1 .f0,
  .loadAcquire .h1,
  .loadRecheck .h1,
  .replacementDelete .h0 .r0 .m0 .killed .p1,
  .replacementCommit .h0,
  .replacementRollback .h0
]

def containsState (items : List Reachable) (state : State) : Bool :=
  items.any fun item => item.state == state

def successors (next : State → Event → Verdict) (item : Reachable) : List Reachable :=
  eventAlphabet.map fun event => {
    trace := item.trace ++ [event]
    state := (next item.state event).state
  }

def uniqueNewStates (seen candidates : List Reachable) : List Reachable :=
  candidates.foldl (fun retained candidate =>
    if containsState seen candidate.state || containsState retained candidate.state then
      retained
    else
      retained ++ [candidate]) []

def explorationLayersWith (next : State → Event → Verdict) :
    Nat → List Reachable → List Reachable → List (List Reachable)
  | 0, _, frontier => [frontier]
  | depth + 1, seen, frontier =>
      let following := uniqueNewStates seen (frontier.flatMap (successors next))
      frontier :: explorationLayersWith next depth (seen ++ following) following

def explorationLayers (depth : Nat) : List (List Reachable) :=
  let initial : Reachable := { trace := [], state := State.initial }
  explorationLayersWith step depth [initial] [initial]

def reachableUpTo (depth : Nat) : List Reachable :=
  (explorationLayers depth).flatten

def firstCounterexample? (next : State → Event → Verdict)
    (depth : Nat) : Option (List Event) :=
  let initial : Reachable := { trace := [], state := State.initial }
  let layers := explorationLayersWith next depth [initial] [initial]
  (layers.flatten.find? fun item => !(safe item.state)).map Reachable.trace

def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

def removeAt : Nat → List α → List α
  | _, [] => []
  | 0, _ :: rest => rest
  | index + 1, item :: rest => item :: removeAt index rest

def shrinkWithFuel (fuel : Nat) (violates : List Event → Bool)
    (trace : List Event) : List Event :=
  match fuel with
  | 0 => trace
  | fuel + 1 =>
      let candidates := (List.range trace.length).map fun index => removeAt index trace
      match candidates.find? violates with
      | none => trace
      | some shorter => shrinkWithFuel fuel violates shorter

def shrinkTrace (next : State → Event → Verdict) (trace : List Event) : List Event :=
  shrinkWithFuel trace.length
    (fun candidate => !(safe (runWith next State.initial candidate))) trace

def atomicityWitnessDetected : Bool :=
  let correct := runWith step State.initial atomicityWitness
  let broken := runWith brokenAtomicityStep State.initial atomicityWitness
  decide (correct.results ≠ broken.results)

def uniquenessWitnessDetected : Bool :=
  safe (runWith step State.initial uniquenessWitness) &&
    !(safe (runWith brokenUniquenessStep State.initial uniquenessWitness))

def boundaryWitnessDetected : Bool :=
  safe (runWith step State.initial boundaryWitness) &&
    !(safe (runWith brokenBoundaryStep State.initial boundaryWitness))

def resultOwnershipWitness : List Event := [
  .open .h0, .open .h1, .begin .h0 .r0 .f0,
  .persist .h1 .r0 .m0 .killed .p0 .valid
]

def resultOwnershipWitnessDetected : Bool :=
  decide ((runWith step State.initial resultOwnershipWitness).results ≠
    (runWith brokenResultOwnershipStep State.initial resultOwnershipWitness).results)

def lookupOwnershipWitnessDetected : Bool :=
  let state := run State.initial [
    .open .h0, .open .h1, .begin .h0 .r0 .f0,
    .persist .h0 .r0 .m0 .killed .p0 .valid
  ]
  let event := Event.lookup .h1 .r0 .m0
  (step state event).rejection.isSome &&
    (brokenResultOwnershipStep state event).storedResult.isSome

def sensitivityPasses : Bool :=
  atomicityWitnessDetected && uniquenessWitnessDetected && boundaryWitnessDetected &&
    resultOwnershipWitnessDetected && lookupOwnershipWitnessDetected

def auditDepth : Nat := 8

def alphabetSize : Nat := eventAlphabet.length

def reachableStateCount (depth : Nat := auditDepth) : Nat :=
  (reachableUpTo depth).length

def checkedTransitionCount (depth : Nat := auditDepth) : Nat :=
  ((explorationLayers depth).take depth).foldl
    (fun count layer => count + layer.length * alphabetSize) 0

def boundedAuditPasses (depth : Nat := auditDepth) : Bool :=
  (reachableUpTo depth).all fun item => safe item.state

def traceName (trace : List Event) : String :=
  String.intercalate " -> " (trace.map eventName)

private def ensureAudit (depth : Nat) : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "session audit did not detect every broken family"
    return .error 2
  unless boundedAuditPasses depth do
    let witness := firstCounterexample? step depth
    IO.eprintln s!"bounded session audit found an unsafe state: {witness.map traceName}"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) (depth : Nat) : IO UInt32 := do
  match ← ensureAudit depth with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then
        IO.FS.createDirAll parent
      IO.FS.writeFile path renderCorpus
      return 0

private def checkCorpus (path : System.FilePath) (depth : Nat) : IO UInt32 := do
  match ← ensureAudit depth with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then
          return 0
        IO.eprintln s!"stale Lean session oracle corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean session oracle corpus {path}: {error}"
        return 1

private def printStats (depth : Nat) : IO UInt32 := do
  match ← ensureAudit depth with
  | .error code => return code
  | .ok () =>
      IO.println s!"depth={depth} alphabet={alphabetSize} states={reachableStateCount depth} transitions={checkedTransitionCount depth} corpus_cases={cases.length}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"atomicity detected={atomicityWitnessDetected} trace={traceName atomicityWitness}"
  IO.println s!"uniqueness detected={uniquenessWitnessDetected} trace={traceName uniquenessWitness}"
  IO.println s!"boundary detected={boundaryWitnessDetected} trace={traceName boundaryWitness}"
  IO.println s!"result_ownership detected={resultOwnershipWitnessDetected} trace={traceName resultOwnershipWitness}"
  IO.println s!"lookup_ownership detected={lookupOwnershipWitnessDetected}"
  return if sensitivityPasses then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  let (depth, args) ← match args with
    | "--depth" :: raw :: rest =>
        match raw.toNat? with
        | some depth =>
            if depth > 0 && depth ≤ auditDepth then pure (depth, rest)
            else
              IO.eprintln s!"depth must be between 1 and {auditDepth}"
              return 2
        | none =>
            IO.eprintln "depth must be a natural number"
            return 2
    | _ => pure (auditDepth, args)
  match args with
  | ["--output", path] => writeCorpus path depth
  | ["--check", path] => checkCorpus path depth
  | ["--stats"] => printStats depth
  | ["--sensitivity"] => printSensitivity
  | _ =>
      IO.eprintln "usage: generate_session [--depth 1..8] (--output PATH | --check PATH | --stats | --sensitivity)"
      return 2

end HoiminOracle.SessionAudit.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.SessionAudit.Executable.main args
