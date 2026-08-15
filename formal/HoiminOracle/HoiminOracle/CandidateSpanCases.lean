import HoiminOracle.CandidateSpanProofs

namespace HoiminOracle.CandidateSpan

def asciiSource : List ByteValue := [120, 32, 61, 32, 49, 32, 43, 32, 50, 10]
def multilineSource : List ByteValue :=
  [120, 32, 61, 32, 48, 10, 121, 32, 61, 32, 49, 32, 43, 32, 50, 10]
def multibyteSource : List ByteValue := [195, 169, 32, 61, 32, 49, 32, 43, 32, 50, 10]

def asciiEnvironment : Environment where
  source := asciiSource
  boundaries := List.replicate (asciiSource.length + 1) true
  scalarStarts := List.replicate asciiSource.length true
  sourceHash := "current"
  maximumOffset := 18446744073709551615

def multilineEnvironment : Environment where
  source := multilineSource
  boundaries := List.replicate (multilineSource.length + 1) true
  scalarStarts := List.replicate multilineSource.length true
  sourceHash := "current"
  maximumOffset := 18446744073709551615

def multibyteEnvironment : Environment where
  source := multibyteSource
  boundaries := [true, false, true, true, true, true, true, true, true, true, true, true]
  scalarStarts := [true, false, true, true, true, true, true, true, true, true, true]
  sourceHash := "current"
  maximumOffset := 18446744073709551615

def placeholderIdentity : Identity where
  schema := 0
  sourceHash := ""
  path := ""
  start := 0
  length := 0
  operator := ""
  replacement := []

def closeIdentity (candidate : Candidate) : Candidate :=
  { candidate with identity := identityOf candidate }

def makeCandidate (path : String) (start length : Nat) (original replacement : List ByteValue)
    (line column : Nat) (sourceHash : String := "current") : Candidate :=
  closeIdentity {
    path := path
    start := start
    length := length
    original := original
    replacement := replacement
    operator := "binary_add_sub"
    line := line
    column := column
    symbol := none
    sourceHash := sourceHash
    identity := placeholderIdentity }

def asciiCandidate : Candidate :=
  makeCandidate "src/ascii.py" 6 1 [43] [45] 1 6

def multilineCandidate : Candidate :=
  makeCandidate "src/multiline.py" 12 1 [43] [45] 2 6

def multibyteCandidate : Candidate :=
  makeCandidate "src/multibyte.py" 7 1 [43] [45] 1 6

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  environment : Environment
  candidate : Candidate
  expectedValid : Bool
  expectedBytes : List ByteValue
  rejection : Option String := none
  deriving Repr, DecidableEq, BEq

def validCase (id mode scenario : String) (environment : Environment)
    (candidate : Candidate) : OracleCase where
  id := id
  mode := mode
  scenario := scenario
  environment := environment
  candidate := candidate
  expectedValid := true
  expectedBytes := replaceBytes environment.source candidate

def rejectedCase (id mode rejection : String) (environment : Environment)
    (candidate : Candidate) : OracleCase where
  id := id
  mode := mode
  scenario := "reject"
  environment := environment
  candidate := candidate
  expectedValid := false
  expectedBytes := environment.source
  rejection := some rejection

def staleHashCandidate : Candidate :=
  closeIdentity { asciiCandidate with sourceHash := "stale" }

def staleOriginalCandidate : Candidate :=
  closeIdentity { asciiCandidate with original := [42] }

def overflowCandidate : Candidate :=
  closeIdentity { asciiCandidate with start := 18446744073709551614, length := 2 }

def nonBoundaryCandidate : Candidate :=
  makeCandidate "src/multibyte.py" 1 1 [169] [45] 1 1

def badLocationCandidate : Candidate :=
  closeIdentity { multilineCandidate with column := 5 }

def cases : List OracleCase :=
  [ validCase "ascii_public" "strict" "apply" asciiEnvironment asciiCandidate
  , validCase "multiline_public" "strict" "apply" multilineEnvironment multilineCandidate
  , validCase "multibyte_public" "strict" "apply" multibyteEnvironment multibyteCandidate
  , validCase "protocol_plan_roundtrip" "internal-fixture" "transport"
      multilineEnvironment multilineCandidate
  , validCase "spool_machine_session_projection" "internal-fixture" "transport"
      multibyteEnvironment multibyteCandidate
  , rejectedCase "stale_hash" "internal-fixture" "file_hash"
      asciiEnvironment staleHashCandidate
  , rejectedCase "stale_original" "internal-fixture" "original"
      asciiEnvironment staleOriginalCandidate
  , rejectedCase "offset_overflow" "internal-fixture" "span"
      asciiEnvironment overflowCandidate
  , rejectedCase "non_boundary" "model-only" "boundary"
      multibyteEnvironment nonBoundaryCandidate
  , rejectedCase "bad_location" "internal-fixture" "location"
      multilineEnvironment badLocationCandidate
  , validCase "reset_before_second" "internal-fixture" "reset"
      asciiEnvironment asciiCandidate
  , { id := "fixture_failure", mode := "infrastructure-error", scenario := "harness",
      environment := asciiEnvironment, candidate := asciiCandidate,
      expectedValid := false, expectedBytes := [], rejection := some "fixture" } ]

def caseResult (item : OracleCase) : Bool × List ByteValue :=
  if item.scenario == "harness" then (false, [])
  else (valid item.environment item.candidate,
    applyOrOriginal item.environment item.candidate)

def caseSafe (item : OracleCase) : Bool :=
  item.schema == 1 &&
    match item.mode, item.scenario with
    | "strict", "apply" => item.rejection.isNone && item.expectedValid
    | "internal-fixture", "transport" => item.rejection.isNone && item.expectedValid
    | "internal-fixture", "reset" => item.rejection.isNone && item.expectedValid
    | "internal-fixture", "reject" => item.rejection.isSome && !item.expectedValid
    | "model-only", "reject" => item.rejection == some "boundary" && !item.expectedValid
    | "infrastructure-error", "harness" =>
        item.rejection == some "fixture" && !item.expectedValid && item.expectedBytes.isEmpty
    | _, _ => false

def byteCharacterOffsetSensitivity : Bool :=
  sourceSlice multibyteEnvironment multibyteCandidate == [43] &&
    sourceSlice multibyteEnvironment { multibyteCandidate with start := 6 } != [43]

def byteColumnSensitivity : Bool :=
  locationAt multibyteEnvironment multibyteCandidate.start == { line := 1, column := 6 } &&
    locationAt multibyteEnvironment multibyteCandidate.start != { line := 1, column := 7 }

def offByOneSensitivity : Bool :=
  sourceSlice asciiEnvironment { asciiCandidate with length := 0 } != [43] &&
    sourceSlice asciiEnvironment { asciiCandidate with length := 2 } != [43]

def replacementLengthSensitivity : Bool :=
  let candidate := closeIdentity { asciiCandidate with replacement := [45, 45] }
  valid asciiEnvironment candidate && candidate.replacement.length != candidate.original.length

def overflowBoundarySensitivity : Bool :=
  !valid asciiEnvironment overflowCandidate &&
    !valid multibyteEnvironment nonBoundaryCandidate

def snapshotLocationSensitivity : Bool :=
  locationAt multilineEnvironment multilineCandidate.start == { line := 2, column := 6 } &&
    locationAt { multilineEnvironment with source := asciiSource }
      multilineCandidate.start != { line := 2, column := 6 }

def brokenTransportDropsSpan (candidate : Candidate) : Candidate :=
  { candidate with start := 0, length := 0 }

def transportFieldSensitivity : Bool :=
  completeTransport multilineCandidate == multilineCandidate &&
    brokenTransportDropsSpan multilineCandidate != multilineCandidate

def brokenIdentityWithoutSpan (candidate : Candidate) : Identity :=
  { identityOf candidate with start := 0, length := 0 }

def identityOmissionSensitivity : Bool :=
  let changed := { asciiCandidate with start := 3 }
  identityOf asciiCandidate != identityOf changed &&
    brokenIdentityWithoutSpan asciiCandidate == brokenIdentityWithoutSpan changed

def applyBeforeValidationSensitivity : Bool :=
  !valid asciiEnvironment staleHashCandidate &&
    replaceBytes asciiEnvironment.source staleHashCandidate != asciiEnvironment.source

def truncateBeforeValidationSensitivity : Bool :=
  !valid multilineEnvironment badLocationCandidate &&
    multilineEnvironment.source.take badLocationCandidate.start != multilineEnvironment.source

def suffixSensitivity : Bool :=
  let correct := replaceBytes asciiEnvironment.source asciiCandidate
  let lost := asciiEnvironment.source.take asciiCandidate.start ++ asciiCandidate.replacement
  let duplicated := correct ++ asciiEnvironment.source.drop (asciiCandidate.start + asciiCandidate.length)
  lost != correct && duplicated != correct

def missingResetSensitivity : Bool :=
  let first := replaceBytes asciiEnvironment.source asciiCandidate
  let second := makeCandidate "src/ascii.py" 8 1 [50] [51] 1 8
  replaceBytes first second !=
    (applyAfterReset asciiEnvironment asciiCandidate second).getD asciiEnvironment.source

def sensitivityPasses : Bool :=
  byteCharacterOffsetSensitivity && byteColumnSensitivity && offByOneSensitivity &&
    replacementLengthSensitivity && overflowBoundarySensitivity && snapshotLocationSensitivity &&
    transportFieldSensitivity && identityOmissionSensitivity && applyBeforeValidationSensitivity &&
    truncateBeforeValidationSensitivity && suffixSensitivity && missingResetSensitivity

end HoiminOracle.CandidateSpan
