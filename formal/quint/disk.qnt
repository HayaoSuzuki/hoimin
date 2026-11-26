module DiskModel {
  const OWNED: Set[int]
  const ALLOW_BAD_DELIVERY: bool
  type Diagnostic = { code: str, message: str }
  var owned: Set[int]
  var primary: Diagnostic
  var secondary: List[Diagnostic]
  var active: int
  var dispatched: int
  var process: int
  var output: int
  var monitor: int
  var report: int
  var executionCleanup: int
  var deliveryCleanup: int
  var finished: bool
  var op: str
  var arg: int
  var root: int
  var accepted: bool
  // Components: pending0, succeeded1, failed2. Cleanup: none0, requested1,
  // clean2, failed3, deferred4, retained5, unowned6.
  pure val noDiagnostic = { code: "", message: "" }
  pure val measurementFailure = { code: "disk.measurement.failed", message: "model" }
  pure val processFailure = { code: "process.failed", message: "process drain failed" }
  pure val cleanupFailure = { code: "workspace.cleanup.failed", message: "model" }
  pure val cleanupDeferred = { code: "workspace.cleanup.deferred", message: "model" }
  pure def appendDiagnostic(items: List[Diagnostic], value: Diagnostic): List[Diagnostic] =
    if (items.foldl(false, (found, entry) => found or entry == value)) items else items.append(value)

  val stopped = primary != noDiagnostic
  val settled = process != 0 and output != 0 and monitor != 0
  val safe = process == 1 and output == 1 and monitor == 1
  val canFinish = active == 0 and settled and report == 1 and
    Set(2, 3, 5, 6).contains(executionCleanup) and
    (Set(2, 6).contains(deliveryCleanup) or (ALLOW_BAD_DELIVERY and deliveryCleanup == 4))
  action init = all {
    owned' = OWNED, primary' = noDiagnostic, secondary' = List(), active' = 0, dispatched' = 0,
    process' = 0, output' = 0, monitor' = 0, report' = 0,
    executionCleanup' = if (OWNED.contains(0)) 0 else 6,
    deliveryCleanup' = if (OWNED.contains(1)) 0 else 6,
    finished' = false, op' = "init", arg' = 0, root' = 0, accepted' = true,
  }
  // Input history is needed for ITF replay, but is not read by admission,
  // lifecycle updates, or Safety. TLC fixes it to the initial values to avoid
  // exploring copies of the same lifecycle state for each preceding input.
  action applyWithTrace(command: str, value: int, target: int, recordInput: bool): bool = {
    val cleanup = if (target == 0) executionCleanup else deliveryCleanup
    val admitted = not(finished) and
      (if (command == "dispatch") not(stopped) and process == 0
       else if (command == "process") process == 0
       else if (command == "output") output == 0
       else if (command == "monitor") monitor == 0
       else if (command == "report") report == 0
       else if (command == "request") active == 0 and settled and cleanup == 0 and (target == 0 or report != 0)
       else if (command == "cleanup") cleanup == 1 and (not(Set(2,3).contains(value)) or safe)
       else if (command == "finish") canFinish
       else true)
    val failure = if (admitted and command == "stop") measurementFailure
      else if (admitted and command == "process" and value == 2) processFailure
      else noDiagnostic
    val cleanupDiagnostic = if (admitted and command == "cleanup" and value == 3) cleanupFailure
      else if (admitted and command == "cleanup" and value == 4) cleanupDeferred
      else noDiagnostic
    all {
      owned' = owned,
      primary' = if (stopped) primary else failure,
      secondary' = if (cleanupDiagnostic != noDiagnostic) appendDiagnostic(secondary, cleanupDiagnostic)
        else if (stopped and failure != noDiagnostic and failure != primary) appendDiagnostic(secondary, failure)
        else secondary,
      active' = if (admitted and command == "dispatch") active + 1 else if (admitted and command == "process") 0 else active,
      dispatched' = if (admitted and command == "dispatch") dispatched + 1 else dispatched,
      process' = if (admitted and command == "process") value else process,
      output' = if (admitted and command == "output") value else output,
      monitor' = if (admitted and command == "monitor") value else monitor,
      report' = if (admitted and command == "report") value else report,
      executionCleanup' = if (admitted and target == 0 and command == "request") 1
        else if (admitted and target == 0 and command == "cleanup") value else executionCleanup,
      deliveryCleanup' = if (admitted and target == 1 and command == "request") 1
        else if (admitted and target == 1 and command == "cleanup") value else deliveryCleanup,
      finished' = (finished or (admitted and command == "finish")),
      op' = if (recordInput) command else "init",
      arg' = if (recordInput) value else 0,
      root' = if (recordInput) target else 0,
      accepted' = if (recordInput) admitted else true,
    }
  }
  action apply(command: str, value: int, target: int): bool =
    applyWithTrace(command, value, target, true)
  action stepWithTrace(recordInput: bool): bool = {
    nondet command = Set("dispatch", "stop", "process", "output", "monitor", "report", "request", "cleanup", "finish").oneOf()
    nondet value = (if (command == "cleanup") Set(2,3,4,5) else if (Set("process","output","monitor","report").contains(command)) Set(1,2) else Set(0)).oneOf()
    nondet target = (if (Set("request","cleanup").contains(command)) Set(0,1) else Set(0)).oneOf()
    all {
      command != "dispatch" or dispatched < 1 or stopped or process != 0 or finished,
      applyWithTrace(command, value, target, recordInput),
    }
  }
  action step = stepWithTrace(true)
  action verifyStep = stepWithTrace(false)
  action cleanStep =
    if (process == 0) apply("process",1,0)
    else if (output == 0) apply("output",1,0)
    else if (monitor == 0) apply("monitor",1,0)
    else if (executionCleanup == 0) apply("request",0,0)
    else if (executionCleanup == 1) apply("cleanup",2,0)
    else if (report == 0) apply("report",1,0)
    else if (deliveryCleanup == 0) apply("request",0,1)
    else if (deliveryCleanup == 1) apply("cleanup",2,1)
    else if (finished) apply("stop",0,0)
    else apply("finish",0,0)
  action deferredStep =
    if (process == 0) apply("process",1,0)
    else if (output == 0) apply("output",1,0)
    else if (monitor == 0) apply("monitor",1,0)
    else if (executionCleanup == 0) apply("request",0,0)
    else if (executionCleanup == 1) apply("cleanup",5,0)
    else if (report == 0) apply("report",1,0)
    else if (deliveryCleanup == 0) apply("request",0,1)
    else if (deliveryCleanup == 1) apply("cleanup",4,1)
    else apply("finish",0,0)
  action processEvidenceStep =
    if (op == "init") apply("dispatch",0,0)
    else if (not(stopped)) apply("stop",0,0)
    else if (process == 0 or (op == "process" and accepted)) apply("process",2,0)
    else if (output == 0) apply("output",1,0)
    else if (monitor == 0) apply("monitor",1,0)
    else if (executionCleanup == 0) apply("request",0,0)
    else if (executionCleanup == 1) apply("cleanup",5,0)
    else if (report == 0) apply("report",1,0)
    else if (deliveryCleanup == 0) apply("request",0,1)
    else if (deliveryCleanup == 1) apply("cleanup",4,1)
    else apply("finish",0,0)
  action cleanupDiagnosticStep =
    if (process == 0) apply("process",1,0)
    else if (output == 0) apply("output",1,0)
    else if (monitor == 0) apply("monitor",1,0)
    else if (executionCleanup == 0) apply("request",0,0)
    else if (executionCleanup == 1) apply("cleanup",3,0)
    else if (report == 0) apply("report",1,0)
    else if (deliveryCleanup == 0) apply("request",0,1)
    else if (deliveryCleanup == 1) apply("cleanup",3,1)
    else if (not(stopped)) apply("stop",0,0)
    else apply("finish",0,0)

  val diagnosticSet = secondary.foldl(Set(), (items, entry) => items.union(Set(entry)))
  val measurementStaysPrimary = primary == measurementFailure and dispatched == 1 and active == 0 and
    secondary == List(processFailure, cleanupDeferred)
  val sharedDiagnosticWithoutStop = not(stopped) and executionCleanup == 3 and deliveryCleanup == 3 and
    secondary == List(cleanupFailure)
  val finishedRejectsNewEvidence = finished and op == "stop" and not(accepted) and not(stopped)
  val DiagnosticSafety = all {
    Set(noDiagnostic, measurementFailure, processFailure).contains(primary),
    diagnosticSet.subseteq(Set(measurementFailure, processFailure, cleanupFailure, cleanupDeferred)),
    not(diagnosticSet.contains(primary)),
    diagnosticSet.size() == secondary.length(),
    (process == 2) == (primary == processFailure or diagnosticSet.contains(processFailure)),
    (executionCleanup == 3 or deliveryCleanup == 3) == diagnosticSet.contains(cleanupFailure),
    (executionCleanup == 4 or deliveryCleanup == 4) == diagnosticSet.contains(cleanupDeferred),
  }
  val Safety = all {
    DiagnosticSafety,
    active >= 0, active <= dispatched,
    not(finished) or (active == 0 and settled and report == 1 and Set(2,3,5,6).contains(executionCleanup) and Set(2,6).contains(deliveryCleanup)),
    not(Set(2,3).contains(executionCleanup)) or safe,
    not(Set(2,3).contains(deliveryCleanup)) or safe,
  }
}
module DiskLifecycle { import DiskModel(OWNED=Set(0,1), ALLOW_BAD_DELIVERY=false).* }
module DiskExecutionOnly { import DiskModel(OWNED=Set(0), ALLOW_BAD_DELIVERY=false).* }
module DiskNoRoots { import DiskModel(OWNED=Set(), ALLOW_BAD_DELIVERY=false).* }
module DiskBadDelivery { import DiskModel(OWNED=Set(0,1), ALLOW_BAD_DELIVERY=true).* }
