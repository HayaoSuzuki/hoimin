import HoiminOracle.OutputRetentionProofs

namespace HoiminOracle.OutputRetention

def truncationMarker : List ByteValue :=
  [10, 91, 46, 46, 46, 32, 104, 111, 105, 109, 105, 110, 32, 111, 117, 116,
    112, 117, 116, 32, 116, 114, 117, 110, 99, 97, 116, 101, 100, 32, 46, 46,
    46, 93, 10]

def u64Maximum : Nat := 18446744073709551615

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  capacity : Nat := 0
  chunks : List Chunk := []
  observedSeed : Nat := 0
  arithmeticIncrement : Nat := 0
  errorCode : Option Nat := none
  deriving Repr, DecidableEq, BEq

def successCase (id mode : String) (capacity : Nat) (chunks : List Chunk) : OracleCase :=
  { id := id, mode := mode, scenario := "success", capacity := capacity, chunks := chunks }

def cases : List OracleCase :=
  [ successCase "empty_zero" "strict" 0 []
  , successCase "empty_capacity" "strict" 64 []
  , successCase "fits_partitioned" "strict" 8 [[1, 2], [3, 4, 5]]
  , successCase "exact_capacity" "strict" 5 [[1, 2], [3, 4, 5]]
  , successCase "one_over_tiny" "strict" 4 [[1, 2, 3], [4, 5]]
  , successCase "marker_exact_boundary" "strict" 35 [List.range 36]
  , successCase "marker_plus_one" "strict" 36 [List.range 40]
  , successCase "recorded_stdout_stderr_order" "internal-fixture" 5 [[111, 49], [101, 49], [111, 50]]
  , successCase "large_chunk" "internal-fixture" 4 [[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]]
  , successCase "multiple_wraps" "internal-fixture" 5
      [[1, 2, 3], [4, 5, 6], [7, 8, 9, 10, 11, 12]]
  , successCase "partition_single" "internal-fixture" 4 [[1, 2, 3, 4, 5]]
  , successCase "partition_many" "internal-fixture" 4 [[1], [2, 3, 4], [], [5]]
  , { id := "create_failure_drains", mode := "internal-fixture", scenario := "create_error",
      capacity := 8, chunks := [[1], [2, 3], [4], [5, 6]], errorCode := some 7 }
  , { id := "first_write_failure_drains", mode := "internal-fixture", scenario := "write_error",
      capacity := 8, chunks := [[1, 2], [3], [4, 5], [6]], errorCode := some 11 }
  , { id := "observed_u64_saturation", mode := "model-only", scenario := "arithmetic",
      observedSeed := u64Maximum - 2, arithmeticIncrement := 8 }
  , { id := "fixture_timeout", mode := "infrastructure-error", scenario := "harness" } ]

def startingState (item : OracleCase) : State :=
  { initial with observed := item.observedSeed }

def caseState (item : OracleCase) : State :=
  let start := startingState item
  match item.scenario, item.errorCode, item.chunks with
  | "create_error", some code, _ => run u64Maximum item.capacity (recordError start code) item.chunks
  | "write_error", some code, chunk :: rest =>
      run u64Maximum item.capacity (receiveFailure u64Maximum start chunk code) rest
  | "arithmetic", _, _ =>
      { start with observed := saturatingAdd u64Maximum start.observed item.arithmeticIncrement }
  | _, _, _ => run u64Maximum item.capacity start item.chunks

structure CaseResult where
  observed : Nat
  retained : Nat
  position : Nat
  drainedChunks : Nat
  bytes : List ByteValue
  errorCode : Option Nat
  deriving Repr, DecidableEq, BEq

def caseResult (item : OracleCase) : CaseResult :=
  let state := caseState item
  { observed := state.observed
    retained := min state.observed item.capacity
    position := state.position
    drainedChunks := state.drainedChunks
    bytes := if state.firstError.isSome then []
      else finalBytesFromObserved truncationMarker item.capacity state.observed state.written
    errorCode := state.firstError }

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 &&
    match item.mode, item.scenario with
    | "strict", "success" => item.errorCode.isNone && item.observedSeed == 0
    | "internal-fixture", "success" => item.errorCode.isNone && item.observedSeed == 0
    | "internal-fixture", "create_error" => item.errorCode.isSome && !item.chunks.isEmpty
    | "internal-fixture", "write_error" => item.errorCode.isSome && !item.chunks.isEmpty
    | "model-only", "arithmetic" => item.chunks.isEmpty && item.errorCode.isNone
    | "infrastructure-error", "harness" => item.chunks.isEmpty && item.errorCode.isNone
    | _, _ => false

def brokenOldest (capacity : Nat) (stream : List ByteValue) : List ByteValue :=
  stream.take capacity

def brokenRetainedAdvance (capacity position : Nat) (chunk : Chunk) : Nat :=
  advance capacity position (min capacity chunk.length)

def brokenOversizedChunk (capacity : Nat) (chunk : Chunk) : List ByteValue :=
  chunk.take capacity

def brokenWrapSkip (capacity : Nat) (stream : List ByteValue) : List ByteValue :=
  (keepNewest capacity stream).drop 1

def brokenWrapDuplicate (capacity : Nat) (stream : List ByteValue) : List ByteValue :=
  let tail := keepNewest capacity stream
  tail ++ tail.getLast?.toList

def brokenMarkerThreshold (marker : List ByteValue) (capacity : Nat)
    (stream : List ByteValue) : List ByteValue :=
  if stream.length ≤ capacity then stream
  else if marker.length ≤ capacity then marker ++ keepNewest (capacity - marker.length) stream
  else keepNewest capacity stream

def brokenMarkerWithoutTailReduction (marker : List ByteValue) (capacity : Nat)
    (stream : List ByteValue) : List ByteValue :=
  if stream.length ≤ capacity then stream else marker ++ keepNewest capacity stream

def brokenRetainedFromWrites (capacity : Nat) (state : State) : Nat :=
  min state.written.length capacity

def brokenWrappingAdd (maximum observed amount : Nat) : Nat :=
  (observed + amount) % (maximum + 1)

def brokenOverwriteError (_first later : Nat) : Nat := later

def brokenPartitionTail (capacity : Nat) (chunks : List Chunk) : List ByteValue :=
  keepNewest capacity (chunks.getLast?.getD [])

def oldestSensitivity : Bool :=
  brokenOldest 3 [1, 2, 3, 4] != keepNewest 3 [1, 2, 3, 4]

def retainedAdvanceSensitivity : Bool :=
  brokenRetainedAdvance 4 0 [1, 2, 3, 4, 5] != advance 4 0 5

def oversizedChunkSensitivity : Bool :=
  brokenOversizedChunk 3 [1, 2, 3, 4] != keepNewest 3 [1, 2, 3, 4]

def wrapSkipSensitivity : Bool :=
  brokenWrapSkip 4 [1, 2, 3, 4, 5, 6] != keepNewest 4 [1, 2, 3, 4, 5, 6]

def wrapDuplicateSensitivity : Bool :=
  brokenWrapDuplicate 4 [1, 2, 3, 4, 5, 6] != keepNewest 4 [1, 2, 3, 4, 5, 6]

def markerThresholdSensitivity : Bool :=
  brokenMarkerThreshold truncationMarker 35 (List.range 36) !=
    finalBytes truncationMarker 35 (List.range 36)

def markerTailSensitivity : Bool :=
  brokenMarkerWithoutTailReduction truncationMarker 36 (List.range 40) !=
    finalBytes truncationMarker 36 (List.range 40)

def retainedWrittenSensitivity : Bool :=
  let failed := receiveFailure u64Maximum initial [1, 2, 3] 7
  brokenRetainedFromWrites 8 failed != min failed.observed 8

def observedSaturationSensitivity : Bool :=
  brokenWrappingAdd 10 9 3 != saturatingAdd 10 9 3

def stopDrainSensitivity : Bool :=
  let failed := recordError initial 7
  failed.drainedChunks != (run u64Maximum 8 failed [[1], [2], [3]]).drainedChunks

def firstErrorSensitivity : Bool := brokenOverwriteError 7 11 != 7

def partitionSensitivity : Bool :=
  brokenPartitionTail 4 [[1, 2, 3, 4, 5]] != brokenPartitionTail 4 [[1], [2, 3], [4, 5]]

def crossPipeOrderSensitivity : Bool :=
  finalBytes truncationMarker 4 ([1, 2] ++ [3, 4]) !=
    finalBytes truncationMarker 4 ([3, 4] ++ [1, 2])

def sensitivityPasses : Bool :=
  oldestSensitivity && retainedAdvanceSensitivity && oversizedChunkSensitivity &&
    wrapSkipSensitivity && wrapDuplicateSensitivity && markerThresholdSensitivity &&
    markerTailSensitivity && retainedWrittenSensitivity && observedSaturationSensitivity &&
    stopDrainSensitivity && firstErrorSensitivity && partitionSensitivity &&
    crossPipeOrderSensitivity

end HoiminOracle.OutputRetention
