namespace HoiminOracle.BoundedFileRead

inductive Result
  | tooLargeAtOpen
  | tooLargeWhenRead
  | accepted (bytes : List UInt8)
  deriving DecidableEq

structure Observation where
  /-- Number of bytes consumed on the admitted descriptor. -/
  consumed : Option Nat
  result : Result
  deriving DecidableEq

structure Policy where
  admitMetadata : Nat → Nat → Bool
  readLimit : Nat → Nat → Nat
  admitPayload : Nat → Nat → Bool

def ordinary : Policy :=
  { admitMetadata := fun limit size => size ≤ limit
    readLimit := fun limit _ => limit + 1
    admitPayload := fun limit size => size ≤ limit }

def acquireWith (policy : Policy) (limit observed : Nat) (available : List UInt8) : Observation :=
  if !policy.admitMetadata limit observed then ⟨none, .tooLargeAtOpen⟩
  else
    let data := available.take (policy.readLimit limit observed)
    ⟨some data.length,
      if policy.admitPayload limit data.length then .accepted data else .tooLargeWhenRead⟩

def acquire := acquireWith ordinary

theorem oversized_metadata_skips_read (limit observed : Nat) (available : List UInt8)
    (tooLarge : limit < observed) :
    acquire limit observed available = ⟨none, .tooLargeAtOpen⟩ := by
  simp [acquire, acquireWith, ordinary, Nat.not_le_of_lt tooLarge]

theorem admitted_read_within_limit (limit observed : Nat) (available : List UInt8)
    (metadataFits : observed ≤ limit) (contentsFit : available.length ≤ limit) :
    acquire limit observed available = ⟨some available.length, .accepted available⟩ := by
  have allBytes : available.take (limit + 1) = available :=
    List.take_of_length_le (by omega)
  simp [acquire, acquireWith, ordinary, metadataFits, allBytes, contentsFit]

theorem growth_consumes_one_extra_byte_and_rejects (limit observed : Nat) (available : List UInt8)
    (metadataFits : observed ≤ limit) (exceeds : limit < available.length) :
    acquire limit observed available = ⟨some (limit + 1), .tooLargeWhenRead⟩ := by
  have size : (available.take (limit + 1)).length = limit + 1 := by
    rw [List.length_take, Nat.min_eq_left (by omega)]
  simp [acquire, acquireWith, ordinary, metadataFits, size]

theorem any_attempt_reads_at_most_limit_plus_one (limit observed : Nat) (available : List UInt8) :
    (acquire limit observed available).consumed.getD 0 ≤ limit + 1 := by
  by_cases metadataFits : observed ≤ limit
  · simp [acquire, acquireWith, ordinary, metadataFits, Nat.min_le_left]
  · simp [acquire, acquireWith, ordinary, metadataFits]

end HoiminOracle.BoundedFileRead
