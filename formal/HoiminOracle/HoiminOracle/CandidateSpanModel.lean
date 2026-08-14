import Std

namespace HoiminOracle.CandidateSpan

abbrev ByteValue := Nat

structure Location where
  line : Nat := 1
  column : Nat := 0
  deriving Repr, DecidableEq, BEq

structure Identity where
  schema : Nat
  sourceHash : String
  path : String
  start : Nat
  length : Nat
  operator : String
  replacement : List ByteValue
  deriving Repr, DecidableEq, BEq

structure Candidate where
  schema : Nat := 1
  path : String
  start : Nat
  length : Nat
  original : List ByteValue
  replacement : List ByteValue
  operator : String
  line : Nat
  column : Nat
  symbol : Option String := none
  sourceHash : String
  identity : Identity
  sequence : Nat := 1
  deriving Repr, DecidableEq, BEq

structure Environment where
  source : List ByteValue
  -- One entry per byte plus the terminal boundary.
  boundaries : List Bool
  -- One entry per byte; true only when that byte begins a Unicode scalar.
  scalarStarts : List Bool
  sourceHash : String
  maximumOffset : Nat
  deriving Repr, DecidableEq, BEq

def identityOf (candidate : Candidate) : Identity where
  schema := candidate.schema
  sourceHash := candidate.sourceHash
  path := candidate.path
  start := candidate.start
  length := candidate.length
  operator := candidate.operator
  replacement := candidate.replacement

def boundaryAt (environment : Environment) (offset : Nat) : Bool :=
  environment.boundaries.getD offset false

def advanceLocation (location : Location) (item : ByteValue × Bool) : Location :=
  if item.1 == 10 then { line := location.line + 1, column := 0 }
  else if item.2 then { location with column := location.column + 1 }
  else location

def locationAt (environment : Environment) (offset : Nat) : Location :=
  (environment.source.zip environment.scalarStarts).take offset |>.foldl advanceLocation {}

def sourceSlice (environment : Environment) (candidate : Candidate) : List ByteValue :=
  (environment.source.drop candidate.start).take candidate.length

def Valid (environment : Environment) (candidate : Candidate) : Prop :=
  candidate.start + candidate.length ≤ environment.maximumOffset ∧
  candidate.start + candidate.length ≤ environment.source.length ∧
  boundaryAt environment candidate.start = true ∧
  boundaryAt environment (candidate.start + candidate.length) = true ∧
  candidate.original.length = candidate.length ∧
  sourceSlice environment candidate = candidate.original ∧
  locationAt environment candidate.start = { line := candidate.line, column := candidate.column } ∧
  candidate.operator ≠ "" ∧
  candidate.replacement ≠ candidate.original ∧
  candidate.sourceHash = environment.sourceHash ∧
  candidate.identity = identityOf candidate

instance (environment : Environment) (candidate : Candidate) : Decidable (Valid environment candidate) :=
  by
    unfold Valid
    infer_instance

def valid (environment : Environment) (candidate : Candidate) : Bool :=
  decide (Valid environment candidate)

def replaceBytes (source : List ByteValue) (candidate : Candidate) : List ByteValue :=
  source.take candidate.start ++ candidate.replacement ++
    source.drop (candidate.start + candidate.length)

def applyChecked (environment : Environment) (candidate : Candidate) : Option (List ByteValue) :=
  if valid environment candidate then some (replaceBytes environment.source candidate) else none

def applyOrOriginal (environment : Environment) (candidate : Candidate) : List ByteValue :=
  (applyChecked environment candidate).getD environment.source

structure FullTransport where
  candidate : Candidate
  deriving Repr, DecidableEq, BEq

def protocolRoundTrip (candidate : Candidate) : FullTransport := ⟨candidate⟩
def planRoundTrip (transport : FullTransport) : FullTransport := transport
def spoolRoundTrip (transport : FullTransport) : FullTransport := transport
def machineRoundTrip (transport : FullTransport) : FullTransport := transport

def completeTransport (candidate : Candidate) : Candidate :=
  (machineRoundTrip (spoolRoundTrip (planRoundTrip (protocolRoundTrip candidate)))).candidate

structure SessionProjection where
  identity : Identity
  deriving Repr, DecidableEq, BEq

def sessionProjection (candidate : Candidate) : SessionProjection :=
  ⟨identityOf candidate⟩

def applyAfterReset (environment : Environment) (_first second : Candidate) :
    Option (List ByteValue) :=
  applyChecked environment second

end HoiminOracle.CandidateSpan
