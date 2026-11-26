// One saved incomplete run, two independent handles, and atomic public calls.
module SessionOwnershipModel {
  const SHARE_OWNERSHIP: bool

  type State = { owners: Set[int], budget: int, complete: bool, result: str }
  type Command = Init | Load({ actor: int, limit: int })
    | Finish({ actor: int, complete: bool }) | Reopen(int) | BeginAgain(int)
  var s: State
  var dispatch: Command

  pure def heldByOther(state: State, actor: int): bool =
    state.owners.exists(owner => owner != actor)

  pure def loadResult(state: State, actor: int, limit: int, share: bool): str =
    if (state.complete) "matching_run_complete"
    else if (limit < state.budget) "budget_decreased"
    else if (heldByOther(state, actor) and not(share)) "session.resume.active"
    else "resumed"

  action init = all {
    s' = { owners: Set(), budget: 1, complete: false, result: "init" },
    dispatch' = Init,
  }

  action loadRun(actor: int, limit: int): bool = {
    val result = loadResult(s, actor, limit, SHARE_OWNERSHIP)
    all {
      s' = if (result == "resumed")
        { ...s, owners: s.owners.union(Set(actor)), budget: limit, result: result }
      else { ...s, result: result },
      dispatch' = Load({ actor: actor, limit: limit }),
    }
  }

  action finishRun(actor: int, complete: bool): bool = all {
    s' = if (s.owners.contains(actor))
      { ...s, owners: s.owners.exclude(Set(actor)), complete: complete, result: "finished" }
    else { ...s, result: "session.finish.owner" },
    dispatch' = Finish({ actor: actor, complete: complete }),
  }

  action reopenHandle(actor: int): bool = all {
    s' = { ...s, owners: s.owners.exclude(Set(actor)), result: "reopened" },
    dispatch' = Reopen(actor),
  }

  action beginAgain(actor: int): bool = all {
    s' = { ...s, result: if (heldByOther(s, actor)) "session.resume.active" else "session.begin" },
    dispatch' = BeginAgain(actor),
  }

  action step = {
    nondet actor = Set(0, 1).oneOf()
    nondet limit = Set(1, 2).oneOf()
    nondet complete = Set(false, true).oneOf()
    any { loadRun(actor, limit), finishRun(actor, complete), reopenHandle(actor), beginAgain(actor) }
  }

  action handoffStep = any {
    all { dispatch == Init, beginAgain(0) },
    all { dispatch == BeginAgain(0), loadRun(0, 1) },
    all { dispatch == Load({ actor: 0, limit: 1 }), s.budget == 1, loadRun(1, 2) },
    all { dispatch == Load({ actor: 1, limit: 2 }), s.budget == 1, finishRun(1, true) },
    all { dispatch == Finish({ actor: 1, complete: true }), reopenHandle(0) },
    all { dispatch == Reopen(0), loadRun(1, 2) },
    all { dispatch == Load({ actor: 1, limit: 2 }), s.owners == Set(1), loadRun(0, 1) },
    all { dispatch == Load({ actor: 0, limit: 1 }), s.budget == 2, finishRun(1, false) },
    all { dispatch == Finish({ actor: 1, complete: false }), loadRun(0, 2) },
    all { dispatch == Load({ actor: 0, limit: 2 }), finishRun(0, true) },
    all { s.complete, loadRun(1, 2) },
  }

  val someoneResumed = s.result == "resumed"
  val blockedPromotion = dispatch == Load({ actor: 1, limit: 2 }) and s.budget == 1 and
    s.result == "session.resume.active"
  val ineligibleWhileOwned = s.result == "budget_decreased" and s.owners == Set(1)
  val deniedFinish = s.result == "session.finish.owner"
  val reopenedUnowned = dispatch == Reopen(0) and s.owners == Set()
  val duplicateWithoutOwner = dispatch == BeginAgain(0) and s.result == "session.begin" and s.owners == Set()
  val finishedAndExcluded = s.complete and s.result == "matching_run_complete"

  val Safety = all {
    s.owners.size() <= 1,
    s.owners.subseteq(Set(0, 1)),
    s.complete implies s.owners == Set(),
    Set(1, 2).contains(s.budget),
    match dispatch {
      | Init => s.result == "init"
      | Reopen(actor) => s.result == "reopened" and not(s.owners.contains(actor))
      | BeginAgain(actor) => s.result == (if (heldByOther(s, actor)) "session.resume.active" else "session.begin")
      | Finish(command) => if (s.result == "finished") s.complete == command.complete and s.owners == Set()
        else s.result == "session.finish.owner" and not(s.owners.contains(command.actor))
      | Load(command) =>
        if (s.complete) s.result == "matching_run_complete"
        else if (s.budget > command.limit) s.result == "budget_decreased"
        else if (heldByOther(s, command.actor)) s.result == "session.resume.active"
        else s.result == "resumed" and s.owners.contains(command.actor) and s.budget == command.limit
    },
  }
}
module SessionOwnership { import SessionOwnershipModel(SHARE_OWNERSHIP = false).* }
module SessionOwnershipShared { import SessionOwnershipModel(SHARE_OWNERSHIP = true).* }
