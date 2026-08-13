import HoiminOracle.ReportSequenceProofs

namespace HoiminOracle.ReportSequence

structure Expected where
  accepted : Bool
  errorCode : Option String
  errorFields : List (String × String)
  deriving Repr, DecidableEq, BEq

structure Case where
  id : String
  mode : String
  premise : String
  prefixEvents : List Event
  target : Event
  expected : Expected
  probe : Option Event
  probeExpected : Option Expected
  deriving Repr, DecidableEq, BEq

def runIdString : RunId → String
  | .first => "first"
  | .second => "second"

def mutantIdString : MutantId → String
  | .alpha => "alpha"
  | .beta => "beta"

def statusString : Status → String
  | .survived => "survived"
  | .killed => "killed"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .error => "error"
  | .notRun => "not_run"

def terminationString : Termination → String
  | .exitZero => "exit_zero"
  | .exitNonzero => "exit_nonzero"
  | .timeout => "timeout"
  | .outOfMemory => "out_of_memory"
  | .processLimit => "process_limit"
  | .cancelled => "cancelled"

def rejectionCode : Rejection → String
  | .runNotStarted => "report.sequence.run_not_started"
  | .runAlreadyStarted _ => "report.sequence.run_already_started"
  | .runAlreadyFinished _ => "report.sequence.run_already_finished"
  | .runFinishedWithActiveMutants _ => "report.sequence.run_finished_with_active_mutants"
  | .runIdMismatch _ _ => "report.sequence.run_id_mismatch"
  | .notMonotonic _ _ => "report.sequence.not_monotonic"
  | .mutantAlreadyStarted _ _ => "report.sequence.mutant_already_started"
  | .duplicateMutantIdentity _ _ => "report.sequence.duplicate_mutant_identity"
  | .mutantIdentitySequenceMismatch _ _ _ =>
      "report.sequence.mutant_identity_sequence_mismatch"
  | .mutantNotStarted _ _ => "report.sequence.mutant_not_started"
  | .mutantStatusTerminationMismatch _ _ _ _ _ =>
      "report.sequence.mutant_status_termination_mismatch"

def rejectionFields : Rejection → List (String × String)
  | .runNotStarted => []
  | .runAlreadyStarted runId | .runAlreadyFinished runId =>
      [("run_id", runIdString runId)]
  | .runFinishedWithActiveMutants count => [("count", toString count)]
  | .runIdMismatch expected received =>
      [("expected", runIdString expected), ("received", runIdString received)]
  | .notMonotonic previous received =>
      [("previous", toString previous), ("received", toString received)]
  | .mutantAlreadyStarted id sequence | .duplicateMutantIdentity id sequence
  | .mutantNotStarted id sequence =>
      [("mutant_id", mutantIdString id), ("mutant_sequence", toString sequence)]
  | .mutantIdentitySequenceMismatch id expected received =>
      [("mutant_id", mutantIdString id),
       ("expected_sequence", toString expected),
       ("received_sequence", toString received)]
  | .mutantStatusTerminationMismatch id sequence status expected termination =>
      [("mutant_id", mutantIdString id),
       ("mutant_sequence", toString sequence),
       ("status", statusString status),
       ("expected_status", statusString expected),
       ("termination", terminationString termination)]

def expectedOf (verdict : Verdict) : Expected :=
  match verdict.rejection with
  | none => { accepted := true, errorCode := none, errorFields := [] }
  | some reason =>
      { accepted := false
        errorCode := some (rejectionCode reason)
        errorFields := rejectionFields reason }

def start (sequence : Nat := 1) (runId : RunId := .first) : Event :=
  { runId, sequence, kind := .runStarted }

def diagnostic (sequence : Nat) (runId : RunId := .first) : Event :=
  { runId, sequence, kind := .diagnostic }

def mutantStart (sequence : Nat) (id : MutantId := .alpha)
    (mutantSequence : Nat := 0) : Event :=
  { runId := .first, sequence, kind := .mutantStarted id mutantSequence }

def mutantFinish (sequence : Nat) (id : MutantId := .alpha)
    (mutantSequence : Nat := 0) (status : Status := .survived)
    (termination : Option Termination := some .exitZero) : Event :=
  { runId := .first, sequence
    kind := .mutantFinished id mutantSequence status termination }

def finishRun (sequence : Nat) : Event :=
  { runId := .first, sequence, kind := .runFinished }

def makeCase (id premise : String) (prefixEvents : List Event) (target : Event)
    (probe : Option Event := none) : Case :=
  let state := run initial prefixEvents
  let verdict := step state target
  { id
    mode := "strict"
    premise
    prefixEvents
    target
    expected := expectedOf verdict
    probe
    probeExpected := probe.map fun event => expectedOf (step verdict.state event) }

def terminationCases : List Case :=
  [(Status.survived, Termination.exitZero, "accepts_exit_zero_status"),
   (Status.killed, Termination.exitNonzero, "accepts_exit_nonzero_status"),
   (Status.timeout, Termination.timeout, "accepts_timeout_status"),
   (Status.outOfMemory, Termination.outOfMemory, "accepts_out_of_memory_status"),
   (Status.processLimit, Termination.processLimit, "accepts_process_limit_status"),
   (Status.notRun, Termination.cancelled, "accepts_cancelled_status")].map fun entry =>
      makeCase entry.2.2 "matching_present_termination"
        [start, mutantStart 2]
        (mutantFinish 3 .alpha 0 entry.1 (some entry.2.1))

def cases : List Case :=
  [makeCase "requires_run_start" "initial" [] (diagnostic 1)
      (some (start 1)),
   makeCase "accepts_diagnostic_after_start" "started" [start] (diagnostic 2),
   makeCase "rejects_cross_run" "started_first_run" [start]
      (diagnostic 2 .second) (some (diagnostic 2)),
   makeCase "rejects_nonmonotonic_equal" "last_sequence_one" [start]
      (diagnostic 1) (some (diagnostic 2)),
   makeCase "rejects_nonmonotonic_reverse" "last_sequence_two"
      [start, diagnostic 2] (diagnostic 1) (some (diagnostic 3)),
   makeCase "rejects_duplicate_run_start" "started" [start]
      (start 2) (some (diagnostic 2)),
   makeCase "rejects_active_duplicate" "alpha_active" [start, mutantStart 2]
      (mutantStart 3) (some (mutantFinish 3)),
   makeCase "rejects_same_sequence_identity_reuse" "alpha_finished"
      [start, mutantStart 2, mutantFinish 3]
      (mutantStart 4) (some (finishRun 4)),
   makeCase "rejects_changed_sequence_identity_reuse" "alpha_finished"
      [start, mutantStart 2, mutantFinish 3]
      (mutantStart 4 .alpha 1) (some (finishRun 4)),
   makeCase "rejects_finish_before_start" "started" [start]
      (mutantFinish 2) (some (diagnostic 2)),
   makeCase "rejects_finish_identity_sequence_mismatch" "alpha_zero_active"
      [start, mutantStart 2] (mutantFinish 3 .alpha 1)
      (some (mutantFinish 3)),
   makeCase "rejects_status_termination_mismatch" "alpha_active"
      [start, mutantStart 2] (mutantFinish 3 .alpha 0 .killed (some .exitZero))
      (some (mutantFinish 3)),
   makeCase "accepts_absent_termination" "alpha_active"
      [start, mutantStart 2] (mutantFinish 3 .alpha 0 .error none),
   makeCase "rejects_finish_with_active_mutants" "alpha_active"
      [start, mutantStart 2] (finishRun 3) (some (mutantFinish 3)),
   makeCase "accepts_distinct_concurrent_mutants" "alpha_active"
      [start, mutantStart 2] (mutantStart 3 .beta 1),
   makeCase "accepts_finish_then_run_finish" "alpha_finished"
      [start, mutantStart 2, mutantFinish 3] (finishRun 4),
   makeCase "rejects_after_run_finish" "run_finished"
      [start, finishRun 2] (diagnostic 3) none] ++ terminationCases

def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

def brokenMutateBeforeValidate (state : State) (event : Event) : Verdict :=
  let verdict := step state event
  match verdict.rejection with
  | none => verdict
  | some reason => { state := { state with last := some event.sequence }, rejection := some reason }

def brokenAllowIdentityReuse (state : State) (event : Event) : Verdict :=
  match event.kind, seenSequence? state.seen (match event.kind with
    | .mutantStarted id _ => id | _ => .alpha) with
  | .mutantStarted _ _, some _ => { state := accept state event, rejection := none }
  | _, _ => step state event

def brokenAcceptEqualSequence (state : State) (event : Event) : Verdict :=
  let normalized := match state.last with
    | some previous => if event.sequence = previous then { state with last := none } else state
    | none => state
  step normalized event

def brokenSequenceFirst (state : State) (event : Event) : Verdict :=
  match state.last with
  | some previous =>
      if event.sequence ≤ previous then reject state (.notMonotonic previous event.sequence)
      else step state event
  | none => step state event

def atomicityWitness : Bool :=
  let events := [start]
  let target := diagnostic 2 .second
  let probe := diagnostic 2
  let correctState := (step (run initial events) target).state
  let brokenState := (brokenMutateBeforeValidate (run initial events) target).state
  expectedOf (step correctState probe) != expectedOf (step brokenState probe)

def uniquenessWitness : Bool :=
  let events := [start, mutantStart 2, mutantFinish 3]
  expectedOf (step (run initial events) (mutantStart 4)) !=
    expectedOf (brokenAllowIdentityReuse (run initial events) (mutantStart 4))

def equalityWitness : Bool :=
  expectedOf (step (run initial [start]) (diagnostic 1)) !=
    expectedOf (brokenAcceptEqualSequence (run initial [start]) (diagnostic 1))

def precedenceWitness : Bool :=
  let event := diagnostic 1 .second
  expectedOf (step (run initial [start]) event) !=
    expectedOf (brokenSequenceFirst (run initial [start]) event)

def sensitivityPasses : Bool :=
  atomicityWitness && uniquenessWitness && equalityWitness && precedenceWitness

end HoiminOracle.ReportSequence
