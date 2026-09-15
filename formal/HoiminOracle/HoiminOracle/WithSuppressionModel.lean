import Std

namespace WithAudit

inductive Event where
  | importTyping | mayRaise | explicitRaise
  deriving DecidableEq, BEq, Repr

structure Flow where
  normal : List Bool
  raised : List Bool
  deriving DecidableEq, BEq, Repr

def run (typing : Bool) : List Event → Flow
  | [] => ⟨[typing], []⟩
  | .importTyping :: rest => run true rest
  | .mayRaise :: rest =>
      let next := run typing rest
      ⟨next.normal, typing :: next.raised⟩
  | .explicitRaise :: _ => ⟨[], [typing]⟩

def afterWith (suppresses : Bool) (flow : Flow) : Flow :=
  if suppresses then ⟨flow.normal ++ flow.raised, []⟩ else flow

def candidate (suppresses : Bool) (events : List Event) : Bool :=
  ((afterWith suppresses (run false events)).normal).all id

-- Deliberately broken: drop suppressed exceptional entries from the join.
-- Keep a conservative fallback when there is no normal body exit.
def brokenCandidate (events : List Event) : Bool :=
  !(run false events).normal.isEmpty && (run false events).normal.all id

def runtime (typing raises : Bool) : List Event → Bool
  | [] => typing
  | .importTyping :: rest => runtime true raises rest
  | .mayRaise :: rest => if raises then typing else runtime typing raises rest
  | .explicitRaise :: _ => typing

theorem runtime_reachable (events : List Event) (typing raises : Bool) :
    runtime typing raises events ∈
      (run typing events).normal ++ (run typing events).raised := by
  induction events generalizing typing with
  | nil => simp [runtime, run]
  | cons event rest ih =>
    cases event with
    | importTyping => exact ih true
    | explicitRaise => simp [runtime, run]
    | mayRaise =>
      have h := ih typing
      cases raises with
      | false =>
        simp only [runtime, Bool.false_eq_true, ↓reduceIte, run,
          List.mem_append, List.mem_cons] at h ⊢
        exact h.imp_right Or.inr
      | true => simp [runtime, run]

theorem candidate_sound (events : List Event) (raises : Bool)
    (allowed : candidate true events = true) : runtime false raises events = true := by
  have member := runtime_reachable events false raises
  simp only [candidate, afterWith, ↓reduceIte, List.all_eq_true] at allowed
  exact allowed _ member

theorem suppressed_prefix_rejects (rest : List Event) :
    candidate true (.mayRaise :: rest) = false := by
  simp [candidate, afterWith, run, List.all_append]

theorem suppression_idempotent (flow : Flow) :
    afterWith true (afterWith true flow) = afterWith true flow := by
  simp [afterWith]

theorem propagation_preserves (flow : Flow) : afterWith false flow = flow := by
  rfl

theorem minimal_witness : candidate true [.mayRaise, .importTyping] = false ∧
    brokenCandidate [.mayRaise, .importTyping] = true := by decide

-- Cost abstraction: one recording traversal and one normal-exit transfer
-- traverse every nested finalbody, including when recording is already off.
def visits : Nat → Nat
  | 0 => 1
  | n + 1 => 2 * visits n

theorem finally_visits (n : Nat) : visits n = 2 ^ n := by
  induction n with
  | zero => rfl
  | succ n ih => simp [visits, ih, Nat.pow_succ, Nat.mul_comm]

def transferOnce : Nat → Nat
  | 0 => 1
  | n + 1 => transferOnce n

theorem transfer_once_leaf_visits (n : Nat) : transferOnce n = 1 := by
  induction n with
  | zero => rfl
  | succ n ih => exact ih

end WithAudit
