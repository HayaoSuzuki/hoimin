import HoiminOracle.OutputRetentionModel

namespace HoiminOracle.OutputRetention

theorem keepNewest_length_le (capacity : Nat) (bytes : List ByteValue) :
    (keepNewest capacity bytes).length ≤ capacity := by
  simp only [keepNewest, List.length_drop]
  omega

theorem keepNewest_eq_self (capacity : Nat) (bytes : List ByteValue)
    (fits : bytes.length ≤ capacity) : keepNewest capacity bytes = bytes := by
  simp [keepNewest, Nat.sub_eq_zero_of_le fits]

theorem finalBytes_length_le (marker : List ByteValue) (capacity : Nat)
    (stream : List ByteValue) : (finalBytes marker capacity stream).length ≤ capacity := by
  simp only [finalBytes]
  split
  · assumption
  · split
    · simp only [List.length_append]
      have tailBound := keepNewest_length_le (capacity - marker.length) stream
      omega
    · exact keepNewest_length_le capacity stream

theorem finalBytesFromObserved_length_le (marker : List ByteValue) (capacity observed : Nat)
    (stream : List ByteValue) (consistent : observed ≤ capacity → stream.length ≤ capacity) :
    (finalBytesFromObserved marker capacity observed stream).length ≤ capacity := by
  simp only [finalBytesFromObserved]
  split
  · exact consistent (by assumption)
  · split
    · simp only [List.length_append]
      have tailBound := keepNewest_length_le (capacity - marker.length) stream
      omega
    · exact keepNewest_length_le capacity stream

theorem saturatingAdd_assoc (maximum observed left right : Nat) :
    saturatingAdd maximum (saturatingAdd maximum observed left) right =
      saturatingAdd maximum observed (left + right) := by
  simp only [saturatingAdd]
  omega

theorem saturatingAdd_le (maximum observed amount : Nat) :
    saturatingAdd maximum observed amount ≤ maximum := by
  exact Nat.min_le_left _ _

theorem recordError_preserves_first (state : State) (first later : Nat)
    (present : state.firstError = some first) :
    (recordError state later).firstError = some first := by
  simp [recordError, present]

theorem receive_preserves_error (maximum capacity : Nat) (state : State)
    (chunk : Chunk) :
    (receive maximum capacity state chunk).firstError = state.firstError := by
  rfl

theorem receive_after_error_drains_without_writing
    (maximum capacity : Nat) (state : State) (chunk : Chunk) (code : Nat)
    (failed : state.firstError = some code) :
    let next := receive maximum capacity state chunk
    next.firstError = some code ∧ next.written = state.written ∧
      next.position = state.position ∧ next.drainedChunks = state.drainedChunks + 1 := by
  simp [receive, failed]

theorem receiveFailure_records_first_and_does_not_write
    (maximum : Nat) (state : State) (chunk : Chunk) (code : Nat)
    (success : state.firstError = none) :
    let next := receiveFailure maximum state chunk code
    next.firstError = some code ∧ next.written = state.written ∧
      next.position = state.position ∧ next.drainedChunks = state.drainedChunks + 1 := by
  simp [receiveFailure, recordError, success]

theorem run_received (maximum capacity : Nat) (state : State) (chunks : List Chunk) :
    (run maximum capacity state chunks).received = state.received ++ chunks.flatten := by
  induction chunks generalizing state with
  | nil => simp [run]
  | cons chunk rest induction =>
      rw [run]
      rw [induction]
      simp [receive, List.flatten]

theorem run_firstError (maximum capacity : Nat) (state : State) (chunks : List Chunk) :
    (run maximum capacity state chunks).firstError = state.firstError := by
  induction chunks generalizing state with
  | nil => rfl
  | cons chunk rest induction =>
      rw [run, induction]
      rfl

theorem run_drainedChunks (maximum capacity : Nat) (state : State) (chunks : List Chunk) :
    (run maximum capacity state chunks).drainedChunks =
      state.drainedChunks + chunks.length := by
  induction chunks generalizing state with
  | nil => simp [run]
  | cons chunk rest induction =>
      rw [run, induction]
      simp [receive]
      omega

theorem run_written_of_success (maximum capacity : Nat) (state : State)
    (chunks : List Chunk) (success : state.firstError = none) :
    (run maximum capacity state chunks).written = state.written ++ chunks.flatten := by
  induction chunks generalizing state with
  | nil => simp [run]
  | cons chunk rest induction =>
      rw [run]
      rw [induction (state := receive maximum capacity state chunk)]
      · simp [receive, success, List.flatten]
      · exact receive_preserves_error maximum capacity state chunk |>.trans success

theorem run_written_after_error (maximum capacity : Nat) (state : State)
    (chunks : List Chunk) (code : Nat) (failed : state.firstError = some code) :
    (run maximum capacity state chunks).written = state.written := by
  induction chunks generalizing state code with
  | nil => rfl
  | cons chunk rest induction =>
      rw [run]
      rw [induction (state := receive maximum capacity state chunk) (code := code)]
      · simp [receive, failed]
      · exact receive_preserves_error maximum capacity state chunk |>.trans failed

theorem run_observed (maximum capacity : Nat) (state : State) (chunks : List Chunk)
    (bounded : state.observed ≤ maximum) :
    (run maximum capacity state chunks).observed =
      saturatingAdd maximum state.observed chunks.flatten.length := by
  induction chunks generalizing state with
  | nil => simp [run, saturatingAdd, Nat.min_eq_right bounded]
  | cons chunk rest induction =>
      rw [run, induction (bounded := saturatingAdd_le maximum state.observed chunk.length)]
      simp only [receive, List.flatten_cons, List.length_append]
      rw [saturatingAdd_assoc]

theorem saturated_observed_matches_stream_length (maximum : Nat) (stream : List ByteValue)
    (representable : stream.length ≤ maximum) :
    saturatingAdd maximum 0 stream.length = stream.length := by
  simp [saturatingAdd, Nat.min_eq_right representable]

theorem advance_assoc (capacity position left right : Nat) :
    advance capacity (advance capacity position left) right =
      advance capacity position (left + right) := by
  by_cases zero : capacity = 0
  · simp [advance, zero]
  · simp only [advance, zero, ↓reduceIte]
    rw [Nat.mod_add_mod]
    congr 1 <;> omega

theorem run_position_of_success (maximum capacity : Nat) (state : State)
    (chunks : List Chunk) (success : state.firstError = none)
    (normalized : state.position = advance capacity state.position 0) :
    (run maximum capacity state chunks).position =
      advance capacity state.position chunks.flatten.length := by
  induction chunks generalizing state with
  | nil => simpa [run] using normalized
  | cons chunk rest induction =>
      rw [run]
      rw [induction (state := receive maximum capacity state chunk)]
      · simp only [receive, success, Option.isSome_none, Bool.false_eq_true, ↓reduceIte,
          List.flatten_cons, List.length_append]
        rw [advance_assoc]
      · exact receive_preserves_error maximum capacity state chunk |>.trans success
      · simp only [receive, success, Option.isSome_none, Bool.false_eq_true, ↓reduceIte]
        rw [advance_assoc]
        simp

theorem successful_run_refines_reference (marker : List ByteValue)
    (maximum capacity : Nat) (chunks : List Chunk)
    (representable : chunks.flatten.length ≤ maximum) :
    successfulObservation marker maximum capacity (run maximum capacity initial chunks) =
      referenceObservation marker maximum capacity chunks.flatten := by
  have observedRun : (run maximum capacity initial chunks).observed =
      chunks.flatten.length := by
    rw [run_observed maximum capacity initial chunks (by simp [initial])]
    exact saturated_observed_matches_stream_length maximum chunks.flatten representable
  have writtenRun : (run maximum capacity initial chunks).written = chunks.flatten := by
    simpa [initial] using run_written_of_success maximum capacity initial chunks rfl
  have positionRun : (run maximum capacity initial chunks).position =
      advance capacity 0 chunks.flatten.length := by
    simpa [initial, advance] using
      run_position_of_success maximum capacity initial chunks rfl (by simp [initial, advance])
  have saturated := saturated_observed_matches_stream_length maximum chunks.flatten representable
  simp only [successfulObservation, referenceObservation]
  rw [observedRun, writtenRun, positionRun, saturated]
  rfl

theorem successful_ring_refines_newest (maximum capacity : Nat) (chunks : List Chunk) :
    logicalRing capacity (run maximum capacity initial chunks) =
      keepNewest capacity chunks.flatten := by
  simp [logicalRing, initial, run_written_of_success]

theorem successful_position_tracks_all_bytes (maximum capacity : Nat)
    (chunks : List Chunk) :
    (run maximum capacity initial chunks).position =
      advance capacity 0 chunks.flatten.length := by
  simpa [initial, advance] using
    run_position_of_success maximum capacity initial chunks rfl (by simp [initial, advance])

theorem successful_observed_saturates (maximum capacity : Nat) (chunks : List Chunk) :
    (run maximum capacity initial chunks).observed =
      min maximum chunks.flatten.length := by
  simpa [initial, saturatingAdd] using
    run_observed maximum capacity initial chunks (by simp [initial])

theorem chunk_partition_invariant (marker : List ByteValue) (maximum capacity : Nat)
    {left right : List Chunk} (sameStream : left.flatten = right.flatten)
    (representable : left.flatten.length ≤ maximum) :
    successfulObservation marker maximum capacity (run maximum capacity initial left) =
      successfulObservation marker maximum capacity (run maximum capacity initial right) := by
  rw [successful_run_refines_reference marker maximum capacity left representable,
    successful_run_refines_reference marker maximum capacity right (sameStream ▸ representable),
    sameStream]

theorem successful_retained_eq_min (marker : List ByteValue) (maximum capacity : Nat)
    (chunks : List Chunk) (representable : chunks.flatten.length ≤ maximum) :
    (successfulObservation marker maximum capacity
      (run maximum capacity initial chunks)).retained =
      min (saturatingAdd maximum 0 chunks.flatten.length) capacity := by
  rw [successful_run_refines_reference marker maximum capacity chunks representable]
  rfl

theorem successful_final_bytes_bounded (marker : List ByteValue)
    (maximum capacity : Nat) (chunks : List Chunk)
    (representable : chunks.flatten.length ≤ maximum) :
    (successfulObservation marker maximum capacity
      (run maximum capacity initial chunks)).bytes.length ≤ capacity := by
  rw [successful_run_refines_reference marker maximum capacity chunks representable]
  exact finalBytes_length_le marker capacity chunks.flatten

theorem marker_requires_strict_spare_capacity (marker stream : List ByteValue)
    (truncated : marker.length < stream.length) :
    finalBytes marker marker.length stream = keepNewest marker.length stream := by
  simp [finalBytes, Nat.not_le_of_lt truncated]

theorem error_run_remains_error_and_drains (maximum capacity : Nat) (state : State)
    (chunks : List Chunk) (code : Nat) (failed : state.firstError = some code) :
    (run maximum capacity state chunks).firstError = some code ∧
      (run maximum capacity state chunks).drainedChunks =
        state.drainedChunks + chunks.length := by
  constructor
  · exact (run_firstError maximum capacity state chunks).trans failed
  · exact run_drainedChunks maximum capacity state chunks

end HoiminOracle.OutputRetention
