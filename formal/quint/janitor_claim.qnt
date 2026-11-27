// Claim one observed directory under the coordinator, before tree removal.
module JanitorClaim {
  type Platform = Unix | Windows
  type DirectoryIdentity = ExpectedDirectory | OtherDirectory
  type LeaseIdentity = ExpectedLease | OtherLease
  type Binding = Missing | Present({ directory: DirectoryIdentity, lease: LeaseIdentity })
  type Evidence = MatchingMarker | MarkerMismatch
  type Interruption = Uninterrupted | Deadline | RenameError | RestoreError | ReopenError
  type LeaseGuard = HeldExpectedLease | Released
  type Outcome = Claimed | Absent | Rejected | Stopped | RenameFailed |
    RestoreFailed | ReopenFailed
  type Input = {
    platform: Platform,
    current: Binding,
    deleting: Binding,
    marker: Evidence,
    interruption: Interruption,
  }
  type Claim = {
    current: Binding,
    deleting: Binding,
    notified: bool,
    guard: LeaseGuard,
    outcome: Outcome,
  }
  type Evaluation = Pending | Evaluated({ input: Input, claim: Claim })
  type State = { checked: int, last: Evaluation }

  pure val maxChecks = 4
  pure val expected = Present({ directory: ExpectedDirectory, lease: ExpectedLease })
  pure val otherDirectory = Present({ directory: OtherDirectory, lease: OtherLease })
  pure val otherLease = Present({ directory: ExpectedDirectory, lease: OtherLease })
  var s: State

  action init = s' = { checked: 0, last: Pending }

  pure def unchanged(input: Input, outcome: Outcome, guard: LeaseGuard): Claim = {
    current: input.current, deleting: input.deleting, notified: false,
    guard: guard, outcome: outcome,
  }

  pure def published(input: Input): Claim = {
    current: Missing, deleting: expected, notified: true,
    guard: if (input.interruption == ReopenError) Released else HeldExpectedLease,
    outcome: if (input.interruption == ReopenError) ReopenFailed else Claimed,
  }

  pure def attempt(input: Input): Claim = {
    val candidate = if (input.current == Missing) input.deleting else input.current
    if (input.interruption == Deadline) unchanged(input, Stopped, HeldExpectedLease)
    else if (candidate == Missing) unchanged(input, Absent, HeldExpectedLease)
    else if (candidate != expected or input.marker == MarkerMismatch)
      unchanged(input, Rejected, HeldExpectedLease)
    else if (input.interruption == RenameError)
      unchanged(input, RenameFailed, HeldExpectedLease)
    else if (input.interruption == RestoreError) unchanged(input, RestoreFailed, Released)
    else published(input)
  }

  action record(input: Input, claim: Claim): bool = all {
    s.checked < maxChecks,
    s' = { checked: s.checked + 1, last: Evaluated({ input: input, claim: claim }) },
  }

  action evaluate(platform: Platform, current: Binding, deleting: Binding,
    marker: Evidence, interruption: Interruption): bool = {
    val input = { platform: platform, current: current, deleting: deleting,
      marker: marker, interruption: interruption }
    record(input, attempt(input))
  }

  action sharedScenarios(platform: Platform): bool = {
    nondet binding = Set(Missing, expected, otherLease, otherDirectory).oneOf()
    nondet findDeleting = Set(false, true).oneOf()
    nondet marker = Set(MatchingMarker, MarkerMismatch).oneOf()
    val interruptions = if (binding == expected and not(findDeleting) and marker == MatchingMarker)
      (if (platform == Windows)
        Set(Uninterrupted, Deadline, RenameError, RestoreError, ReopenError)
        else Set(Uninterrupted, Deadline, RenameError))
      else (if (platform == Windows and binding == expected and marker == MatchingMarker)
        Set(Uninterrupted, Deadline, ReopenError) else Set(Uninterrupted, Deadline))
    nondet interruption = interruptions.oneOf()
    evaluate(platform, if (findDeleting) Missing else binding,
      if (findDeleting) binding else Missing, marker, interruption)
  }

  action step = {
    nondet platform = Set(Unix, Windows).oneOf()
    sharedScenarios(platform)
  }

  action identityScenarios(platform: Platform): bool =
    if (s.checked == 0) evaluate(platform, otherDirectory, Missing, MatchingMarker, Uninterrupted)
    else if (s.checked == 1) evaluate(platform, otherLease, Missing, MatchingMarker, Uninterrupted)
    else if (s.checked == 2) evaluate(platform, Missing, otherDirectory, MatchingMarker, Uninterrupted)
    else evaluate(platform, expected, Missing, MarkerMismatch, Uninterrupted)

  action handoffScenarios(platform: Platform): bool =
    if (s.checked == 0) evaluate(platform, expected, Missing, MatchingMarker, Uninterrupted)
    else if (s.checked == 1) evaluate(platform, Missing, expected, MatchingMarker, Uninterrupted)
    else if (s.checked == 2) evaluate(platform, Missing, Missing, MatchingMarker, Uninterrupted)
    else evaluate(platform, expected, Missing, MatchingMarker, Deadline)

  action unixIdentityStep = identityScenarios(Unix)
  action windowsIdentityStep = identityScenarios(Windows)
  action unixHandoffStep = handoffScenarios(Unix)
  action windowsHandoffStep = handoffScenarios(Windows)

  action unixFaultStep = evaluate(Unix, expected, Missing, MatchingMarker, RenameError)

  action windowsFaultStep =
    if (s.checked == 0) evaluate(Windows, expected, Missing, MatchingMarker, RenameError)
    else if (s.checked == 1) evaluate(Windows, expected, Missing, MatchingMarker, RestoreError)
    else if (s.checked == 2) evaluate(Windows, expected, Missing, MatchingMarker, ReopenError)
    else evaluate(Windows, Missing, expected, MatchingMarker, ReopenError)

  action acceptWrongLeaseStep = {
    val input = { platform: Unix, current: otherLease, deleting: Missing,
      marker: MatchingMarker, interruption: Uninterrupted }
    record(input, { current: Missing, deleting: otherLease, notified: true,
      guard: HeldExpectedLease, outcome: Claimed })
  }

  pure def safe(input: Input, claim: Claim): bool = {
    val candidate = if (input.current == Missing) input.deleting else input.current
    all {
      claim.notified == Set(Claimed, ReopenFailed).contains(claim.outcome),
      claim.notified implies
        candidate == expected and input.marker == MatchingMarker and
        claim.deleting == expected and claim.current == Missing,
      not(claim.notified) implies
        claim.current == input.current and claim.deleting == input.deleting,
      (claim.outcome == Claimed) implies claim.guard != Released,
      (claim.guard == Released) == Set(RestoreFailed, ReopenFailed).contains(claim.outcome),
      (claim.guard == Released) implies input.platform == Windows,
      (claim.outcome == Absent) implies candidate == Missing,
      (input.interruption == Deadline) implies claim.outcome == Stopped,
    }
  }

  val Safety = match s.last {
    | Pending => true
    | Evaluated(value) => safe(value.input, value.claim)
  }
}
