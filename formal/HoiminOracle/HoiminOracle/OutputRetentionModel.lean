import Std

namespace HoiminOracle.OutputRetention

abbrev ByteValue := Nat
abbrev Chunk := List ByteValue

def keepNewest (capacity : Nat) (bytes : List ByteValue) : List ByteValue :=
  bytes.drop (bytes.length - capacity)

def advance (capacity position amount : Nat) : Nat :=
  if capacity = 0 then 0 else (position + amount) % capacity

def saturatingAdd (maximum observed amount : Nat) : Nat :=
  min maximum (observed + amount)

structure State where
  received : List ByteValue := []
  written : List ByteValue := []
  position : Nat := 0
  observed : Nat := 0
  firstError : Option Nat := none
  drainedChunks : Nat := 0
  deriving Repr, DecidableEq, BEq

def initial : State := {}

def receive (maximum capacity : Nat) (state : State) (chunk : Chunk) : State :=
  let failed := state.firstError.isSome
  { received := state.received ++ chunk
    written := if failed then state.written else state.written ++ chunk
    position := if failed then state.position else advance capacity state.position chunk.length
    observed := saturatingAdd maximum state.observed chunk.length
    firstError := state.firstError
    drainedChunks := state.drainedChunks + 1 }

def recordError (state : State) (code : Nat) : State :=
  if state.firstError.isSome then state else { state with firstError := some code }

def receiveFailure (maximum : Nat) (state : State) (chunk : Chunk) (code : Nat) : State :=
  recordError
    { state with
      received := state.received ++ chunk
      observed := saturatingAdd maximum state.observed chunk.length
      drainedChunks := state.drainedChunks + 1 }
    code

def run (maximum capacity : Nat) : State → List Chunk → State
  | state, [] => state
  | state, chunk :: rest => run maximum capacity (receive maximum capacity state chunk) rest

def logicalRing (capacity : Nat) (state : State) : List ByteValue :=
  keepNewest capacity state.written

def finalBytes (marker : List ByteValue) (capacity : Nat)
    (stream : List ByteValue) : List ByteValue :=
  if stream.length ≤ capacity then stream
  else if marker.length < capacity then
    marker ++ keepNewest (capacity - marker.length) stream
  else keepNewest capacity stream

structure Observation where
  observed : Nat
  retained : Nat
  position : Nat
  bytes : List ByteValue
  deriving Repr, DecidableEq, BEq

def referenceObservation (marker : List ByteValue) (maximum capacity : Nat)
    (stream : List ByteValue) : Observation where
  observed := saturatingAdd maximum 0 stream.length
  retained := min (saturatingAdd maximum 0 stream.length) capacity
  position := advance capacity 0 stream.length
  bytes := finalBytes marker capacity stream

def successfulObservation (marker : List ByteValue) (_maximum capacity : Nat)
    (state : State) : Observation where
  observed := state.observed
  retained := min state.observed capacity
  position := state.position
  bytes := finalBytes marker capacity state.written

end HoiminOracle.OutputRetention
