// Public ReportSequence lifecycle, with finite event numbers and identities.
module ReportModel {
  const ALLOW_EARLY_FINISH: bool
  type Identity = { id: int, seq: int }
  var phase: int
  var runId: int
  var last: int
  var active: Set[Identity]
  var seen: Set[Identity]
  var op: str
  var eventRun: int
  var mutant: int
  var mutantSequence: int
  var sequence: int
  var result: str

  def validate(eventKind: str, candidateRun: int, candidateMutant: int,
      candidateMutantSequence: int, candidateSequence: int): str = {
    val identity = { id: candidateMutant, seq: candidateMutantSequence }
    val businessError =
      if (phase == 2) "finished"
      else if (phase == 1 and eventKind == "run_start") "started"
      else if (phase == 0 and eventKind != "run_start") "not_started"
      else if (phase == 1 and candidateRun != runId) "run_id"
      else if (eventKind == "mutant_start") {
        if (active.contains(identity)) "active"
        else if (seen.contains(identity)) "identity_duplicate"
        else if (seen.exists(i => i.id == candidateMutant)) "identity_sequence"
        else "ok"
      } else if (eventKind == "mutant_finish") {
        if (seen.exists(i => i.id == candidateMutant and i.seq != candidateMutantSequence)) "identity_sequence"
        else if (active.contains(identity)) "ok" else "not_active"
      } else if (eventKind == "run_finish" and active.size() != 0 and not(ALLOW_EARLY_FINISH)) "unfinished"
      else "ok"
    if (businessError != "ok") businessError
    else if (candidateSequence <= last) "sequence" else "ok"
  }
  action init = all {
    phase' = 0, runId' = 0, last' = -1, active' = Set(), seen' = Set(),
    op' = "init", eventRun' = 0, mutant' = 0, mutantSequence' = 0, sequence' = 0, result' = "init",
  }
  action dispatchEvent(chosenOp: str, chosenRun: int, chosenMutant: int,
      chosenMutantSequence: int, chosenSequence: int): bool = {
    val outcome = validate(chosenOp, chosenRun, chosenMutant, chosenMutantSequence, chosenSequence)
    val accepted = outcome == "ok"
    val identity = { id: chosenMutant, seq: chosenMutantSequence }
    all {
      phase' = if (accepted and chosenOp == "run_start") 1 else if (accepted and chosenOp == "run_finish") 2 else phase,
      runId' = if (accepted and chosenOp == "run_start") chosenRun else runId,
      last' = if (accepted) chosenSequence else last,
      active' = if (accepted and chosenOp == "mutant_start") active.union(Set(identity))
        else if (accepted and chosenOp == "mutant_finish") active.filter(i => i != identity) else active,
      seen' = if (accepted and chosenOp == "mutant_start") seen.union(Set(identity)) else seen,
      op' = chosenOp, eventRun' = chosenRun, mutant' = chosenMutant,
      mutantSequence' = chosenMutantSequence, sequence' = chosenSequence, result' = outcome,
    }
  }
  action step = {
    nondet chosenOp = Set("run_start", "mutant_start", "mutant_finish", "diagnostic", "run_finish").oneOf()
    val relevantRun = (phase == 0 and chosenOp == "run_start") or (phase == 1 and chosenOp != "run_start")
    nondet chosenRun = (if (relevantRun) Set(0, 1) else Set(0)).oneOf()
    val relevantMutant = phase == 1 and chosenRun == runId and (chosenOp == "mutant_start" or chosenOp == "mutant_finish")
    nondet chosenMutant = (if (relevantMutant) Set(0, 1) else Set(0)).oneOf()
    nondet chosenMutantSequence = (if (relevantMutant) Set(0, 1) else Set(0)).oneOf()
    val sequenceChoices = if (last == -1) Set(0) else if (last < 5) Set(last, last + 1) else Set(last)
    nondet chosenSequence = sequenceChoices.oneOf()
    dispatchEvent(chosenOp, chosenRun, chosenMutant, chosenMutantSequence, chosenSequence)
  }
  // Explicit overlapping inputs, with expectations derived by dispatchEvent.
  action interleavedStep =
    if (last == -1) dispatchEvent("run_start", 0, 0, 0, 0)
    else if (last == 0) dispatchEvent("mutant_start", 0, 0, 0, 1)
    else if (last == 1) dispatchEvent("mutant_start", 0, 1, 1, 2)
    else if (last == 2) dispatchEvent("mutant_finish", 0, 1, 1, 3)
    else if (last == 3) dispatchEvent("mutant_finish", 0, 0, 0, 4)
    else if (last == 4) dispatchEvent("run_finish", 0, 0, 0, 5)
    else dispatchEvent("diagnostic", 0, 0, 0, 5)
  val Safety = all {
    phase != 2 or active.size() == 0,
    active.subseteq(seen),
    seen.map(i => i.id).size() == seen.size(),
  }
}
module Report { import ReportModel(ALLOW_EARLY_FINISH = false).* }
module ReportEarlyFinish { import ReportModel(ALLOW_EARLY_FINISH = true).* }
