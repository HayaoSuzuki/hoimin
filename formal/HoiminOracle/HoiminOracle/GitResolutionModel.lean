import HoiminOracle.GitInventoryModel
import HoiminOracle.GitPhysicalRowsModel

namespace HoiminOracle.GitResolution

inductive Comparison | head | mergeBase (oid : String)
  deriving BEq

inductive Request
  | worktree | head | revision (name : String)
  | inventory (base : Comparison)
  | patch (base : Comparison) (context : Nat) (paths : List String := [])
  | numstat (base : Comparison) (paths : List String := [])
  | indexed | untracked
  deriving BEq

inductive Failure | repository | command | oidUtf8 | oidForm | inventory | hunk
  deriving BEq

inductive Inventory | empty | modified | renamed | malformed
  deriving BEq

structure Reply where
  status : Option Nat := some 0
  bytes : List UInt8 := []
  deriving BEq

def Reply.exited (status : Nat) (bytes : List UInt8 := []) : Reply := ⟨some status, bytes⟩

def utf8 (s : String) : List UInt8 := s.toUTF8.data.toList

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
  oid : Reply := .exited 0 (List.replicate 40 97 ++ [10])
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

private def tracked (input : Input) (base : Comparison) : List Exchange :=
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

def headRoute : Option Nat → Except Failure Bool
  | some 0 => .ok true
  | some 1 => .ok false
  | _ => .error .command

theorem headRoute_failure_iff (status : Option Nat) :
    headRoute status = .error .command ↔ status ≠ some 0 ∧ status ≠ some 1 := by
  cases status with
  | none => simp [headRoute]
  | some code =>
    cases code with
    | zero => simp [headRoute]
    | succ rest => cases rest <;> simp [headRoute]

private def worktreeAccepted (reply : Reply) : Bool :=
  reply.status == some 0 && trim reply.bytes == utf8 "true"

def requests (input : Input) : List Exchange :=
  let worktreeFailure := if worktreeAccepted input.worktree then none else some Failure.repository
  let start : Exchange := ⟨.worktree, input.worktree, worktreeFailure⟩
  let selected := if let some revision := input.revision then
    match readOid input.oid with
    | .error failure => [⟨.revision revision, input.oid, some failure⟩]
    | .ok oid => ⟨.revision revision, input.oid, none⟩ :: tracked input (.mergeBase oid)
  else
    let reply : Reply := ⟨input.headStatus, []⟩
    match headRoute input.headStatus with
    | .ok true => ⟨.head, reply, none⟩ :: tracked input .head
    | .ok false => [
        ⟨.head, reply, none⟩, response input .indexed (.exited 0 (utf8 (selectedPath ++ "\x00")))]
    | .error failure => [⟨.head, reply, some failure⟩]
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
  simp [requests, ordinary, accepted, fatal, execute]

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
