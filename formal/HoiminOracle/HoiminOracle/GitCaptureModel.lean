namespace HoiminOracle.GitCapture

inductive Stream | stdout | stderr
  deriving DecidableEq, BEq

inductive Step
  | bytes (content : List UInt8)
  | ioError

inductive Failure | overflow | ioError
  deriving BEq

structure Capture where
  result : Except Failure (List UInt8)
  reads : Nat

def stderrMarker : List UInt8 :=
  "\n[Git stderr truncated after 65536 bytes]".toUTF8.data.toList

def diagnosticBytes (limit : Nat) (bytes : List UInt8) : List UInt8 :=
  bytes.take limit ++ if limit < bytes.length then stderrMarker else []

/-- The reference keeps unbounded received bytes, applying the retention rule once at EOF. -/
def captureFrom (stream : Stream) (limit : Nat) (received : List UInt8) :
    List Step → Capture
  | [] => ⟨.ok (if stream = .stderr then diagnosticBytes limit received else received), 1⟩
  | .ioError :: _ => ⟨.error .ioError, 1⟩
  | .bytes content :: more =>
    let complete := received ++ content
    if stream = .stdout ∧ limit < complete.length then ⟨.error .overflow, 1⟩
    else
      let result := captureFrom stream limit complete more
      { result with reads := result.reads + 1 }

def capture (stream : Stream) (limit : Nat) (steps : List Step) : Capture :=
  captureFrom stream limit [] steps

theorem diagnostic_partition_independent (limit : Nat) (received : List UInt8)
    (chunks : List (List UInt8)) :
    captureFrom .stderr limit received (chunks.map Step.bytes) =
      ⟨.ok (diagnosticBytes limit (received ++ chunks.flatten)), chunks.length + 1⟩ := by
  induction chunks generalizing received with
  | nil => simp [captureFrom]
  | cons bytes rest ih =>
    simp [captureFrom, ih, List.append_assoc, Nat.add_assoc]

theorem stdout_partition_independent (limit : Nat) (received : List UInt8)
    (chunks : List (List UInt8)) (within : received.length ≤ limit) :
    (captureFrom .stdout limit received (chunks.map Step.bytes)).result =
      if (received ++ chunks.flatten).length ≤ limit then .ok (received ++ chunks.flatten)
      else .error .overflow := by
  induction chunks generalizing received with
  | nil => simp [captureFrom, within]
  | cons bytes rest ih =>
    by_cases fits : (received ++ bytes).length ≤ limit
    · have notOver : ¬ limit < received.length + bytes.length := by
        simp only [List.length_append] at fits
        omega
      simpa [captureFrom, notOver, List.append_assoc] using ih (received ++ bytes) fits
    · have over : limit < received.length + bytes.length := by
        simp only [List.length_append] at fits
        omega
      have totalOver : ¬ (received ++ (bytes :: rest).flatten).length ≤ limit := by
        simp
        omega
      simp at totalOver
      simp [captureFrom, over, totalOver]

theorem diagnostic_bound (limit : Nat) (bytes : List UInt8) :
    (diagnosticBytes limit bytes).length ≤ limit + stderrMarker.length := by
  unfold diagnosticBytes
  split <;> simp <;> omega

theorem stderr_error_is_observed (limit : Nat) (received : List UInt8)
    (chunks : List (List UInt8)) (after : List Step) :
    captureFrom .stderr limit received ((chunks.map Step.bytes) ++ .ioError :: after) =
      ⟨.error .ioError, chunks.length + 1⟩ := by
  induction chunks generalizing received with
  | nil => simp [captureFrom]
  | cons bytes rest ih => simp [captureFrom, ih, Nat.add_assoc]

theorem stdout_io_error_before_overflow (limit : Nat) (received : List UInt8)
    (chunks : List (List UInt8)) (after : List Step)
    (fits : (received ++ chunks.flatten).length ≤ limit) :
    captureFrom .stdout limit received ((chunks.map Step.bytes) ++ .ioError :: after) =
      ⟨.error .ioError, chunks.length + 1⟩ := by
  induction chunks generalizing received with
  | nil => simp [captureFrom]
  | cons bytes rest ih =>
    have remainingFits : ((received ++ bytes) ++ rest.flatten).length ≤ limit := by
      simpa [List.append_assoc] using fits
    have chunkFits : ¬ limit < received.length + bytes.length := by
      simp only [List.length_append] at fits
      simp at fits
      omega
    simp [captureFrom, chunkFits, ih (received ++ bytes) remainingFits, Nat.add_assoc]

theorem stdout_stops_at_crossing (limit : Nat) (received bytes : List UInt8) (after : List Step)
    (crossing : limit < (received ++ bytes).length) :
    captureFrom .stdout limit received (.bytes bytes :: after) = ⟨.error .overflow, 1⟩ := by
  simp only [List.length_append] at crossing
  simp [captureFrom, crossing]

def requestedCapacity (current needed ceiling : Nat) : Nat :=
  min ceiling (max needed (2 * max current 8192))

theorem bounded_growth (current needed ceiling : Nat) (fits : needed ≤ ceiling) :
    needed ≤ requestedCapacity current needed ceiling ∧ requestedCapacity current needed ceiling ≤ ceiling := by
  unfold requestedCapacity
  omega

end HoiminOracle.GitCapture
