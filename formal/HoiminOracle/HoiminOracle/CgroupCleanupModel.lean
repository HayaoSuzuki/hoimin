import Std

namespace HoiminOracle.CgroupCleanup

inductive Strategy where
  | kernel | liveGroup | membership
  deriving DecidableEq, BEq

inductive Tree where
  | empty | populated | unreadable
  deriving DecidableEq, BEq

inductive Call where
  | kernelKill | groupSignal | memberScan | inspect (result : Tree) | wait
  deriving DecidableEq, BEq

inductive Failure where
  | groupSignal | memberScan | inspect | wait
  deriving DecidableEq, BEq

structure Start where
  kernelOk : Bool
  liveRoot : Bool
  groupOk : Bool
  membersOk : Bool

structure Round where
  tree : Tree
  membersOk : Bool := true
  waitOk : Bool := true

structure Progress where
  calls : List Call
  failure : Option Failure := none
  deriving DecidableEq

structure Observation where
  strategy : Option Strategy
  cleanup : Progress
  accountingFailed : Bool
  deriving DecidableEq

def choose (start : Start) : Strategy :=
  if start.kernelOk then .kernel else if start.liveRoot then .liveGroup else .membership

def beginCleanup (start : Start) : Progress :=
  match choose start with
  | .kernel => ⟨[.kernelKill], none⟩
  | .liveGroup =>
    if !start.groupOk then ⟨[.kernelKill, .groupSignal], some .groupSignal⟩
    else ⟨[.kernelKill, .groupSignal, .memberScan],
      if start.membersOk then none else some .memberScan⟩
  | .membership =>
    ⟨[.kernelKill, .memberScan], if start.membersOk then none else some .memberScan⟩

def Strategy.usesMemberScan : Strategy → Bool
  | .kernel => false
  | .liveGroup | .membership => true

def finishCleanup (strategy : Strategy) : List Round → Progress
  | [] => ⟨[.inspect .empty], none⟩
  | round :: rest =>
    match round.tree with
    | .empty => ⟨[.inspect .empty], none⟩
    | .unreadable => ⟨[.inspect .unreadable], some .inspect⟩
    | .populated =>
      let calls := [.inspect .populated] ++ if strategy.usesMemberScan then [.memberScan] else []
      if strategy.usesMemberScan && !round.membersOk then ⟨calls, some .memberScan⟩
      else if !round.waitOk then ⟨calls ++ [.wait], some .wait⟩
      else
        let next := finishCleanup strategy rest
        ⟨calls ++ [.wait] ++ next.calls, next.failure⟩

def run (start : Start) (rounds : List Round) (accountingFailed : Bool) : Observation :=
  let startResult := beginCleanup start
  match startResult.failure with
  | some _ => ⟨none, startResult, accountingFailed⟩
  | none =>
    let strategy := choose start
    let finishResult := finishCleanup strategy rounds
    ⟨some strategy, ⟨startResult.calls ++ finishResult.calls, finishResult.failure⟩, accountingFailed⟩

theorem startup_does_not_signal_a_group_without_a_live_root (start : Start)
    (noLiveRoot : start.liveRoot = false) : Call.groupSignal ∉ (beginCleanup start).calls := by
  cases start with
  | mk kernel owned group members =>
    simp at noLiveRoot
    subst owned
    cases kernel <;> simp [beginCleanup, choose]

theorem kernel_success_never_falls_back_in_startup (start : Start) (accepted : start.kernelOk = true) :
    beginCleanup start = ⟨[.kernelKill], none⟩ := by
  simp [beginCleanup, choose, accepted]

theorem finish_never_reissues_a_group_signal (strategy : Strategy) (rounds : List Round) :
    Call.groupSignal ∉ (finishCleanup strategy rounds).calls := by
  induction rounds with
  | nil => simp [finishCleanup]
  | cons round rest ih =>
    obtain ⟨tree, members, waited⟩ := round
    cases tree <;> cases strategy <;> cases members <;> cases waited <;>
      simp_all [finishCleanup, Strategy.usesMemberScan]

theorem kernel_finish_never_scans_members (rounds : List Round) :
    Call.memberScan ∉ (finishCleanup .kernel rounds).calls := by
  induction rounds with
  | nil => simp [finishCleanup]
  | cons round rest ih =>
    obtain ⟨tree, members, waited⟩ := round
    cases tree <;> cases waited <;> simp_all [finishCleanup, Strategy.usesMemberScan]

theorem finish_success_requires_an_empty_tree_check (strategy : Strategy) (rounds : List Round)
    (completed : (finishCleanup strategy rounds).failure = none) :
    Call.inspect .empty ∈ (finishCleanup strategy rounds).calls := by
  induction rounds with
  | nil => simp [finishCleanup]
  | cons round rest ih =>
    obtain ⟨tree, members, waited⟩ := round
    cases tree <;> cases strategy <;> cases members <;> cases waited <;>
      simp_all [finishCleanup, Strategy.usesMemberScan]

theorem accounting_failure_does_not_change_cleanup (start : Start) (rounds : List Round) :
    (run start rounds true).cleanup = (run start rounds false).cleanup := by
  cases startup : (beginCleanup start).failure <;> simp only [run, startup]

theorem no_group_signal_after_root_reap (start : Start) (rounds : List Round) (accountingFailed : Bool)
    (noLiveRoot : start.liveRoot = false) : Call.groupSignal ∉ (run start rounds accountingFailed).cleanup.calls := by
  have startupSafe := startup_does_not_signal_a_group_without_a_live_root start noLiveRoot
  cases startup : (beginCleanup start).failure <;>
    simp [run, startup, startupSafe, finish_never_reissues_a_group_signal]

theorem unreadable_tree_stops_before_scanning_or_waiting (strategy : Strategy) (later : List Round)
    (membersOk waitOk : Bool) :
    finishCleanup strategy (⟨.unreadable, membersOk, waitOk⟩ :: later) =
      ⟨[.inspect .unreadable], some .inspect⟩ := rfl

theorem group_signal_error_precedes_member_scan_and_inspection (start : Start) (rounds : List Round)
    (kernelFailed : start.kernelOk = false) (liveRoot : start.liveRoot = true)
    (groupFailed : start.groupOk = false) (accountingFailed : Bool) :
    run start rounds accountingFailed = ⟨none, ⟨[.kernelKill, .groupSignal], some .groupSignal⟩,
      accountingFailed⟩ := by
  simp [run, beginCleanup, choose, kernelFailed, liveRoot, groupFailed]

end HoiminOracle.CgroupCleanup
