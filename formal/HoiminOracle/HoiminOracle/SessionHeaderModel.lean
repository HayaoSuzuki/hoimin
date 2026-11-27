import Std

namespace HoiminOracle.SessionHeader

-- The ASCII bytes of "SQLite format 3" followed by NUL.
def magic : List UInt8 := [83, 81, 76, 105, 116, 101, 32, 102, 111, 114, 109, 97, 116, 32, 51, 0]

def headerSize : Nat := 20

def isWal (bytes : List UInt8) : Bool :=
  decide (headerSize ≤ bytes.length) &&
    bytes.take 16 == magic && bytes[18]? == some 2 && bytes[19]? == some 2

theorem short_header_is_ordinary (bytes : List UInt8) (short : bytes.length < headerSize) :
    isWal bytes = false := by
  simp [isWal, Nat.not_le.mpr short]

theorem wal_header_requires_magic_and_both_versions (bytes : List UInt8) (wal : isWal bytes = true) :
    headerSize ≤ bytes.length ∧
      bytes.take 16 = magic ∧ bytes[18]? = some 2 ∧ bytes[19]? = some 2 := by
  simpa [isWal, Bool.and_eq_true, and_assoc] using wal

theorem page_size_does_not_affect_header_routing (pageHigh pageLow write read : UInt8)
    (suffix : List UInt8) :
    isWal (magic ++ [pageHigh, pageLow, write, read] ++ suffix) =
      ((write == 2) && (read == 2)) := by
  simp [isWal, headerSize, magic, List.cons_append, List.nil_append]

theorem trailing_bytes_do_not_affect_routing (prefixBytes suffix : List UInt8)
    (full : headerSize ≤ prefixBytes.length) :
    isWal (prefixBytes ++ suffix) = isWal prefixBytes := by
  have enough : 20 ≤ prefixBytes.length := full
  have combined : headerSize ≤ prefixBytes.length + suffix.length := by omega
  simp [isWal, full, combined, List.take_append_of_le_length (by omega : 16 ≤ prefixBytes.length),
    List.getElem?_append_left (by omega : 18 < prefixBytes.length),
    List.getElem?_append_left (by omega : 19 < prefixBytes.length)]

inductive Fault where
  | interrupted | unexpectedEof | permissionDenied | invalidData
  deriving DecidableEq, BEq, Repr

inductive Read where
  | bytes (data : List UInt8)
  | failed (fault : Fault)

inductive Outcome where
  | immutable | ordinary | failed (fault : Fault) | incompleteScript
  deriving DecidableEq, Repr

inductive FaultDecision where
  | retry
  | finish (outcome : Outcome)
  deriving DecidableEq, Repr

def onFault : Fault → FaultDecision
  | .interrupted => .retry
  | .unexpectedEof => .finish .ordinary
  | fault => .finish (.failed fault)

structure Observation where
  outcome : Outcome
  requests : List Nat
  deriving DecidableEq, Repr

def readWith (classify : List UInt8 → Bool) (handleFault : Fault → FaultDecision)
    (seen : List UInt8) (reads : List Read) : Observation :=
  if headerSize ≤ seen.length then ⟨if classify seen then .immutable else .ordinary, []⟩
  else
    let remaining := headerSize - seen.length
    match reads with
    | [] => ⟨.incompleteScript, []⟩
    | event :: rest =>
      let finishOrRetry := fun fault =>
        match handleFault fault with
        | .retry => readWith classify handleFault seen rest
        | .finish outcome => ⟨outcome, []⟩
      let result := match event with
        | .bytes [] => finishOrRetry .unexpectedEof
        | .bytes data => readWith classify handleFault (seen ++ data.take remaining) rest
        | .failed fault => finishOrRetry fault
      { result with requests := remaining :: result.requests }
termination_by reads.length

def run := readWith isWal onFault

theorem no_more_reads_after_header (seen : List UInt8) (reads : List Read)
    (full : headerSize ≤ seen.length) :
    run seen reads = ⟨if isWal seen then .immutable else .ordinary, []⟩ := by
  rw [run, readWith]
  simp [full]

theorem interruption_preserves_partial_header (seen : List UInt8) (reads : List Read)
    (short : seen.length < headerSize) :
    run seen (.failed .interrupted :: reads) =
      { run seen reads with requests := (headerSize - seen.length) :: (run seen reads).requests } := by
  rw [run, readWith]
  simp [Nat.not_le.mpr short, onFault]

theorem early_eof_uses_ordinary_reader (seen : List UInt8) (reads : List Read)
    (short : seen.length < headerSize) :
    run seen (.failed .unexpectedEof :: reads) = ⟨.ordinary, [headerSize - seen.length]⟩ := by
  rw [run, readWith]
  simp [Nat.not_le.mpr short, onFault]

theorem non_eof_read_failure_is_preserved (seen : List UInt8) (reads : List Read) (fault : Fault)
    (short : seen.length < headerSize) (interrupted : fault ≠ .interrupted)
    (eof : fault ≠ .unexpectedEof) :
    run seen (.failed fault :: reads) = ⟨.failed fault, [headerSize - seen.length]⟩ := by
  cases fault <;> simp_all [run, readWith, Nat.not_le.mpr short, onFault]

end HoiminOracle.SessionHeader
