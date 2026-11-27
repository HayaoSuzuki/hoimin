namespace HoiminOracle.SnapshotRead

inductive Reply
  | available (bytes : Nat)
  | interrupted
  | ioError

inductive Outcome
  | emptyRequest
  | data (bytes : Nat)
  | eof
  | grew
  | shrank
  | interrupted
  | ioError
  deriving DecidableEq

def Outcome.delivered : Outcome → Nat
  | .data bytes => bytes
  | _ => 0

def Outcome.stopsDrain : Outcome → Bool
  | .emptyRequest | .data _ | .interrupted => false
  | .eof | .grew | .shrank | .ioError => true

structure Call where
  capacity : Nat
  reply : Reply

structure Observation where
  requested : Option Nat
  consumed : Nat
  remaining : Nat
  outcome : Outcome
  deriving DecidableEq

def requested (remaining capacity : Nat) : Option Nat :=
  if capacity = 0 then none
  else if remaining = 0 then some 1
  else some (min remaining capacity)

def readWith (request : Nat → Nat → Option Nat) (remaining : Nat) (call : Call) : Observation :=
  let size := request remaining call.capacity
  match size, call.reply with
  | none, _ => ⟨none, 0, remaining, .emptyRequest⟩
  | some _, .interrupted => ⟨size, 0, remaining, .interrupted⟩
  | some _, .ioError => ⟨size, 0, remaining, .ioError⟩
  | some limit, .available bytes =>
    let count := min limit bytes
    if remaining = 0 then
      ⟨size, count, 0, if count = 0 then .eof else .grew⟩
    else
      ⟨size, count, remaining - count, if count = 0 then .shrank else .data count⟩

def read := readWith requested

/-- Physical bytes read by a drain that stops at EOF or an error other than Interrupted. -/
def drainConsumed (remaining : Nat) : List Call → Nat
  | [] => 0
  | call :: rest =>
    let obs := read remaining call
    obs.consumed + if obs.outcome.stopsDrain then 0 else drainConsumed obs.remaining rest

theorem empty_request_avoids_underlying_io (remaining : Nat) (reply : Reply) :
    read remaining ⟨0, reply⟩ = ⟨none, 0, remaining, .emptyRequest⟩ := by
  simp [read, readWith, requested]

theorem delivered_conserves_budget (remaining : Nat) (call : Call) :
    (read remaining call).remaining + (read remaining call).outcome.delivered = remaining := by
  rcases call with ⟨capacity, reply⟩
  by_cases hc : capacity = 0
  · simp [hc, empty_request_avoids_underlying_io, Outcome.delivered]
  cases reply with
  | interrupted | ioError =>
    by_cases hr : remaining = 0 <;> simp [read, readWith, requested, hc, hr, Outcome.delivered]
  | available bytes =>
    by_cases hr : remaining = 0
    · simp only [read, readWith, requested, hc, if_false, hr, if_true, Nat.zero_add]
      by_cases hz : min 1 bytes = 0 <;> simp [hz, Outcome.delivered]
    · simp only [read, readWith, requested, hc, hr, if_false]
      have fits : min (min remaining capacity) bytes ≤ remaining :=
        Nat.le_trans (Nat.min_le_left _ _) (Nat.min_le_left _ _)
      by_cases hz : min (min remaining capacity) bytes = 0
      · rw [hz]
        simp [Outcome.delivered]
      · rw [if_neg hz]
        exact Nat.sub_add_cancel fits

theorem continued_read_conserves_budget (remaining : Nat) (call : Call)
    (h : (read remaining call).outcome.stopsDrain = false) :
    (read remaining call).consumed + (read remaining call).remaining = remaining := by
  rcases call with ⟨capacity, reply⟩
  by_cases hc : capacity = 0
  · simp [hc, empty_request_avoids_underlying_io]
  cases reply with
  | interrupted | ioError =>
    by_cases hr : remaining = 0 <;> simp [read, readWith, requested, hc, hr]
  | available bytes =>
    by_cases hr : remaining = 0
    · simp only [read, readWith, requested, hc, if_false, hr, if_true] at h
      by_cases hz : min 1 bytes = 0 <;> simp [hz, Outcome.stopsDrain] at h
    · simp only [read, readWith, requested, hc, hr, if_false]
      have fits : min (min remaining capacity) bytes ≤ remaining :=
        Nat.le_trans (Nat.min_le_left _ _) (Nat.min_le_left _ _)
      omega

theorem individual_read_bound (remaining : Nat) (call : Call) :
    (read remaining call).consumed ≤ remaining + 1 := by
  rcases call with ⟨capacity, reply⟩
  by_cases hc : capacity = 0
  · simp [hc, empty_request_avoids_underlying_io]
  by_cases hr : remaining = 0
  · cases reply <;> simp [read, readWith, requested, hc, hr, Nat.min_le_left]
  cases reply <;> simp [read, readWith, requested, hc, hr]
  omega

theorem drain_consumes_at_most_one_extra_byte (remaining : Nat) (calls : List Call) :
    drainConsumed remaining calls ≤ remaining + 1 := by
  induction calls generalizing remaining with
  | nil => simp [drainConsumed]
  | cons call rest ih =>
    simp only [drainConsumed]
    split
    · simpa using individual_read_bound remaining call
    · rename_i cont
      have conserved := continued_read_conserves_budget remaining call (by simpa using cont)
      have tailBound := ih (read remaining call).remaining
      omega

end HoiminOracle.SnapshotRead
