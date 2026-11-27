import HoiminOracle.GitInventoryModel
import HoiminOracle.GitPhysicalRowsModel

namespace HoiminOracle.GitResolution

inductive Request
  | worktree | head | symbolicHead | headReference | revision (name : String) | peelHead (oid : String)
  | mergeBase (base head : String)
  | inventory (commit : String)
  | patch (commit : String) (context : Nat) (paths : List String := [])
  | numstat (commit : String) (paths : List String := [])
  | indexed | untracked
  deriving BEq

inductive Failure
  | repository | command | oidUtf8 | oidForm | inventory | hunk
  | missingHead | noMergeBase | multipleMergeBases
  deriving BEq

inductive Inventory | empty | modified | renamed | malformed
  deriving BEq

structure Reply where
  status : Option Nat := some 0
  bytes : List UInt8 := []
  deriving BEq

def Reply.exited (status : Nat) (bytes : List UInt8 := []) : Reply := ⟨some status, bytes⟩

def utf8 (s : String) : List UInt8 := s.toUTF8.data.toList

def rawHeadOid := String.ofList (List.replicate 40 'b')
def headCommitOid := String.ofList (List.replicate 40 'c')
def baseOid := String.ofList (List.replicate 40 'a')
def mergeBaseOid := String.ofList (List.replicate 40 'd')

structure Exchange where
  request : Request
  reply : Reply
  failure : Option Failure := none
  deriving BEq

structure Input where
  revision : Option String := none
  context : Nat := 0
  scope : Option (List String) := none
  worktree : Reply := .exited 0 (utf8 "true\n")
  headStatus : Option Nat := some 0
  headBytes : List UInt8 := utf8 (rawHeadOid ++ "\n")
  headCommit : Reply := .exited 0 (utf8 (headCommitOid ++ "\n"))
  symbolicStatus : Option Nat := some 0
  referenceStatus : Option Nat := some 1
  oid : Reply := .exited 0 (utf8 (baseOid ++ "\n"))
  merge : Reply := .exited 0 (utf8 (mergeBaseOid ++ "\n"))
  inventory : Inventory := .modified
  changedPatch : Bool := false
  corruptPatch : Bool := false
  failAt : Option Request := none

def selectedPath := "pkg/a.py"
def untrackedPath := "pkg/u.py"
def selectedContents : List UInt8 := utf8 "a\rb\n"
def untrackedContents : List UInt8 := utf8 "u\n"

private def trim (input : List UInt8) : List UInt8 :=
  let whitespace (byte : UInt8) := ([9, 10, 12, 13, 32] : List UInt8).contains byte
  ((input.dropWhile whitespace).reverse.dropWhile whitespace).reverse

private def asciiHex (byte : UInt8) : Bool :=
  (48 ≤ byte && byte ≤ 57) || (65 ≤ byte && byte ≤ 70) || (97 ≤ byte && byte ≤ 102)

def readOid (reply : Reply) : Except Failure String :=
  if reply.status != some 0 then .error .command
  else
    let bytes := trim reply.bytes
    match String.fromUTF8? ⟨bytes.toArray⟩ with
    | none => .error .oidUtf8
    | some oid =>
      if (bytes.length == 40 || bytes.length == 64) && bytes.all asciiHex then .ok oid
      else .error .oidForm

private def response (input : Input) (request : Request) (reply : Reply) (failure : Option Failure := none) :
    Exchange :=
  if input.failAt == some request then ⟨request, .exited 23, some .command⟩
  else ⟨request, reply, failure⟩

private def inventoryFacts (inventory : Inventory) : List HoiminOracle.GitInventory.Record :=
  match inventory with
  | .modified => [(.modified, selectedPath, none)]
  | .renamed => [(.renamed, "pkg/b.py", some selectedPath)]
  | _ => []

private def inventoryWire (inventory : Inventory) : List UInt8 :=
  match inventory with
  | .empty => []
  | .modified => utf8 ("M\x00" ++ selectedPath ++ "\x00")
  | .renamed => utf8 ("R099\x00pkg/b.py\x00" ++ selectedPath ++ "\x00")
  | .malformed => utf8 ("M\x00" ++ selectedPath)

private def patchWire (input : Input) : List UInt8 :=
  let hunk := if input.corruptPatch then "@@ -1 +invalid @@\n" else "@@ -1 +1 @@\n"
  if input.changedPatch || input.corruptPatch then utf8
    ("diff --git a/pkg/a.py b/pkg/a.py\n--- a/pkg/a.py\n+++ b/pkg/a.py\n" ++ hunk)
  else []

private def tracked (input : Input) (base : String) : List Exchange :=
  let pair (paths : List String) := [
    response input (.patch base input.context paths) (.exited 0 (patchWire input))
      (if input.corruptPatch then some .hunk else none),
    response input (.numstat base paths) {}]
  match input.scope with
  | none => pair []
  | some scope =>
    let inventory := response input (.inventory base) (.exited 0 (inventoryWire input.inventory))
      (if input.inventory == .malformed then some .inventory else none)
    let batches := match HoiminOracle.GitInventory.plan scope (inventoryFacts input.inventory) with
      | .legacy => pair []
      | .scoped parts => parts.flatMap pair
    inventory :: batches

def headRoute (status : Option Nat) : Except Failure Bool :=
  if status = some 0 then .ok true
  else if status = some 128 then .ok false
  else .error .command

theorem headRoute_failure_iff (status : Option Nat) :
    headRoute status = .error .command ↔ status ≠ some 0 ∧ status ≠ some 128 := by
  by_cases zero : status = some 0 <;> by_cases fatal : status = some 128 <;>
    simp [headRoute, zero, fatal]

private def worktreeAccepted (reply : Reply) : Bool :=
  reply.status == some 0 && trim reply.bytes == utf8 "true"

private def confirmUnborn (input : Input) (base : Option String) : List Exchange :=
  let symbolic : Exchange := ⟨.symbolicHead, ⟨input.symbolicStatus, utf8 "refs/heads/fixture\n"⟩,
    if input.symbolicStatus != some 0 then some .command
    else if base.isSome then some .missingHead else none⟩
  let reference : Exchange := ⟨.headReference, ⟨input.referenceStatus, []⟩,
    if input.referenceStatus == some 1 then none else some .command⟩
  reference :: symbolic :: (if base.isNone then
    [response input .indexed (.exited 0 (utf8 (selectedPath ++ "\x00")))] else [])

def readMergeBase (reply : Reply) : Except Failure String :=
  if reply.status == some 1 then .error .noMergeBase
  else if reply.status != some 0 then .error .command
  else if (trim reply.bytes).isEmpty then .error .noMergeBase
  else if (trim reply.bytes).contains 10 then .error .multipleMergeBases
  else readOid reply

private def compareHead (input : Input) (base : Option String) (head : String) : List Exchange :=
  match base with
  | none => tracked input head
  | some oid =>
    let request := Request.mergeBase oid head
    match readMergeBase input.merge with
    | .error failure => [⟨request, input.merge, some failure⟩]
    | .ok merge => ⟨request, input.merge, none⟩ :: tracked input merge

private def headPlan (input : Input) (base : Option String) : List Exchange :=
  let reply : Reply := ⟨input.headStatus, if input.headStatus == some 0 then input.headBytes else []⟩
  match headRoute input.headStatus with
  | .ok true =>
    match readOid reply with
    | .error failure => [⟨.head, reply, some failure⟩]
    | .ok oid =>
      ⟨.head, reply, none⟩ :: (match readOid input.headCommit with
        | .error failure => [⟨.peelHead oid, input.headCommit, some failure⟩]
        | .ok commit => ⟨.peelHead oid, input.headCommit, none⟩ :: compareHead input base commit)
  | .ok false => ⟨.head, reply, none⟩ :: confirmUnborn input base
  | .error failure => [⟨.head, reply, some failure⟩]

def requests (input : Input) : List Exchange :=
  let worktreeFailure := if worktreeAccepted input.worktree then none else some Failure.repository
  let start : Exchange := ⟨.worktree, input.worktree, worktreeFailure⟩
  let selected := if let some revision := input.revision then
    match readOid input.oid with
    | .error failure => [⟨.revision revision, input.oid, some failure⟩]
    | .ok oid => ⟨.revision revision, input.oid, none⟩ :: headPlan input (some oid)
  else headPlan input none
  start :: selected ++ [response input .untracked (.exited 0 (utf8 (untrackedPath ++ "\x00")))]

theorem explicit_revision_first (input : Input) (name : String) (specified : input.revision = some name) :
    ((requests input).map (·.request)).take 2 = [.worktree, .revision name] := by
  cases checked : readOid input.oid <;> simp [requests, specified, checked]

/-- Only the prefix through the first failure is observable. -/
def execute : List Exchange → List Exchange
  | [] => []
  | call :: remaining => call :: if call.failure.isSome then [] else execute remaining

theorem fatal_head_stops (input : Input) (ordinary : input.revision = none)
    (accepted : worktreeAccepted input.worktree = true)
    (fatal : headRoute input.headStatus = .error .command) :
    (execute (requests input)).map (·.request) = [.worktree, .head] := by
  simp [requests, ordinary, accepted, headPlan, fatal, execute]

theorem indexed_iff_unborn_after_unresolved (input : Input) (ordinary : input.revision = none)
    (accepted : worktreeAccepted input.worktree = true) (unresolved : input.headStatus = some 128) :
    Request.indexed ∈ (execute (requests input)).map (·.request) ↔
      input.symbolicStatus = some 0 ∧ input.referenceStatus = some 1 := by
  by_cases symbolic : input.symbolicStatus = some 0 <;>
    by_cases reference : input.referenceStatus = some 1 <;>
    by_cases injected : (input.failAt == some Request.indexed) = true <;>
    simp [requests, ordinary, accepted, unresolved, headPlan, headRoute, confirmUnborn,
      symbolic, reference, execute, response, injected]

theorem failed_merge_base_stops_before_diff (input : Input) (name base exact head : String)
    (explicit : input.revision = some name) (accepted : worktreeAccepted input.worktree = true)
    (baseOk : readOid input.oid = .ok base) (lookupOk : input.headStatus = some 0)
    (exactOk : readOid (.exited 0 input.headBytes) = .ok exact)
    (peelOk : readOid input.headCommit = .ok head)
    (failure : Failure) (mergeFailed : readMergeBase input.merge = .error failure) :
    (execute (requests input)).map (·.request) =
      [.worktree, .revision name, .head, .peelHead exact, .mergeBase base head] := by
  simp only [Reply.exited] at exactOk
  simp [requests, explicit, accepted, baseOk, headPlan, headRoute, lookupOk,
    exactOk, compareHead, peelOk, mergeFailed, execute]

theorem stops_at_failure (before after : List Exchange) (failed : Exchange)
    (ok : ∀ call ∈ before, call.failure = none) (hasError : failed.failure.isSome = true) :
    execute (before ++ failed :: after) = before ++ [failed] := by
  induction before with
  | nil => simp [execute, hasError]
  | cons call rest ih =>
    have success := ok call (by simp)
    simp [execute, success, ih (fun c member => ok c (List.mem_cons_of_mem _ member))]

private def span (bytes : List UInt8) : HoiminOracle.GitPhysicalRows.Span :=
  ⟨1, HoiminOracle.GitPhysicalRows.physicalCount bytes⟩

def changed (input : Input) (calls : List Exchange) : List (String × HoiminOracle.GitPhysicalRows.Span) :=
  let admitted (path : String) := input.scope.all (·.contains path)
  let tracked := calls.any fun call => match call.request with
    | .indexed => true
    | .patch _ _ _ => input.changedPatch
    | _ => false
  (if tracked && admitted selectedPath then [(selectedPath, span selectedContents)] else []) ++
    (if admitted untrackedPath then [(untrackedPath, span untrackedContents)] else [])

end HoiminOracle.GitResolution
