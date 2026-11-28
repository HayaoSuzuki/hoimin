import HoiminOracle.CandidateSpanModel

namespace HoiminOracle.WorkspaceMutation

abbrev Bytes := List Nat

inductive Encoding where
  | utf8 | ascii | latin1 | invalid
  deriving DecidableEq

structure Snapshot where
  bytes : Bytes
  encoding : Encoding := .utf8
  bom : Bool := false

structure File where
  bytes : Bytes
  readOnly : Bool
  deriving DecidableEq

structure Request where
  tracked : Bool := true
  currentHash : Bool := true
  currentId : Bool := true
  start : Nat
  length : Nat
  original : String
  replacement : String
  operator : String := "test"
  line : Nat
  column : Nat

inductive Fault where
  | reserve | beforeRepair | afterRepair
  deriving DecidableEq

inductive Outcome where
  | applied | notSelected | missing | hash | original | span | shared
  | reserveFault | beforeRepairFault | afterRepairFault
  deriving DecidableEq

structure Result where
  outcome : Outcome
  file : Option File
  bytesRead : Option Nat := none
  faultTriggered : Bool := false
  deriving DecidableEq

def encode (encoding : Encoding) (text : String) : Option Bytes :=
  let points := text.toList.map Char.toNat
  match encoding with
  | .utf8 | .invalid => some (text.toUTF8.data.toList.map UInt8.toNat)
  | .ascii => if points.all (· < 128) then some points else none
  | .latin1 => if points.all (· < 256) then some points else none

def startsScalar (encoding : Encoding) (byte : Nat) : Bool :=
  encoding = .latin1 || byte < 128 || 192 ≤ byte

def boundary (snapshot : Snapshot) (offset : Nat) : Bool :=
  if offset = snapshot.bytes.length then true
  else (snapshot.bytes[offset]?).any (startsScalar snapshot.encoding)

def location (snapshot : Snapshot) (offset : Nat) : CandidateSpan.Location :=
  ((snapshot.bytes.take offset).zipIdx.map fun (byte, index) =>
    (byte, startsScalar snapshot.encoding byte && !(snapshot.bom && index < 3)))
    |>.foldl CandidateSpan.advanceLocation {}

inductive Validation where
  | accepted (replacement : Bytes)
  | rejected (outcome : Outcome)
  deriving DecidableEq

def validate (snapshot : Snapshot) (request : Request) : Validation :=
  let stop := request.start + request.length
  if request.operator = "" || request.original = request.replacement || stop > snapshot.bytes.length then
    .rejected .span
  else if encode snapshot.encoding request.original ≠ some ((snapshot.bytes.drop request.start).take request.length) then
    .rejected .original
  else if snapshot.encoding = .invalid then .rejected .span
  else if !boundary snapshot request.start || !boundary snapshot stop then .rejected .original
  else
    match encode snapshot.encoding request.replacement with
    | none => .rejected .span
    | some encoded =>
      if location snapshot request.start = ⟨request.line, request.column⟩ && request.currentId then
        .accepted encoded
      else .rejected .span

def splice (bytes : Bytes) (start length : Nat) (replacement : Bytes) : Bytes :=
  bytes.take start ++ replacement ++ bytes.drop (start + length)

def resized (bytes : Bytes) (size : Nat) : Bytes :=
  bytes.take size ++ List.replicate (size - bytes.length) 0

private def afterRead (verify : Snapshot → Request → Validation) (snapshot : Snapshot) (request : Request)
    (observed : File) (read : Nat) (shared : Bool) (fault : Option Fault) : Result :=
  let unchanged outcome : Result := { outcome, file := some observed, bytesRead := some read }
  if observed.bytes.length ≠ snapshot.bytes.length || !request.currentHash || observed.bytes ≠ snapshot.bytes then
    unchanged .hash
  else match verify snapshot request with
  | .rejected outcome => unchanged outcome
  | .accepted replacement =>
    if fault = some .beforeRepair then { unchanged .beforeRepairFault with faultTriggered := true }
    else if shared then unchanged .shared
    else if fault = some .afterRepair then
      { unchanged .afterRepairFault with file := some { observed with readOnly := false }, faultTriggered := true }
    else
      { unchanged .applied with file := some ⟨splice observed.bytes request.start request.length replacement, false⟩ }

private theorem afterRead_records (verify : Snapshot → Request → Validation) (snapshot : Snapshot)
    (request : Request) (observed : File) (read : Nat) (shared : Bool) (fault : Option Fault) :
    (afterRead verify snapshot request observed read shared fault).bytesRead = some read := by
  unfold afterRead
  repeat' (first | rfl | split)

def runWith (verify : Snapshot → Request → Validation) (snapshot : Snapshot) (request : Request)
    (entry : Option File) (shared : Bool) (fault : Option Fault) (resize : Option Nat) : Result :=
  let stopped outcome : Result := { outcome, file := entry }
  if !request.tracked then stopped .notSelected
  else match entry with
  | none => stopped .missing
  | some file =>
    let budget := snapshot.bytes.length
    if file.bytes.length ≠ budget then stopped .hash
    else if fault = some .reserve then { stopped .reserveFault with faultTriggered := true }
    else
      let observed := (resize.map (fun size => { file with bytes := resized file.bytes size })).getD file
      afterRead verify snapshot request observed (min observed.bytes.length (budget + 1)) shared fault

def run := runWith validate

theorem splice_preserves_prefix_and_suffix (bytes : Bytes) (start length : Nat) (replacement : Bytes)
    (fits : start ≤ bytes.length) :
    (splice bytes start length replacement).take start = bytes.take start ∧
    (splice bytes start length replacement).drop (start + replacement.length) = bytes.drop (start + length) := by
  have taken : (bytes.take start).length = start := by simp [List.length_take, Nat.min_eq_left fits]
  constructor
  · simp [splice, taken]
  · rw [splice, List.append_assoc]
    rw [show start + replacement.length = (bytes.take start).length + replacement.length by rw [taken]]
    rw [List.drop_length_add_append]
    simp

theorem read_respects_captured_length (snapshot : Snapshot) (request : Request) (entry : Option File)
    (shared : Bool) (fault : Option Fault) (resize : Option Nat) (read : Nat)
    (observed : (run snapshot request entry shared fault resize).bytesRead = some read) :
    read ≤ snapshot.bytes.length + 1 := by
  by_cases tracked : request.tracked = true
  · cases entry with
    | none => simp [run, runWith, tracked] at observed
    | some file =>
      by_cases sized : file.bytes.length = snapshot.bytes.length <;>
        by_cases reserve : fault = some .reserve <;>
        simp [run, runWith, tracked, sized, reserve, afterRead_records] at observed
      rw [← observed]
      exact Nat.min_le_right ..
  · simp [run, runWith, tracked] at observed

theorem validation_rejection_preserves_matching_file (snapshot : Snapshot) (request : Request) (file : File)
    (tracked : request.tracked = true) (hash : request.currentHash = true) (matching : file.bytes = snapshot.bytes)
    (outcome : Outcome) (invalid : validate snapshot request = .rejected outcome)
    (shared : Bool) (fault : Option Fault) (afterReserve : fault ≠ some .reserve) :
    run snapshot request (some file) shared fault none = ⟨outcome, some file, some file.bytes.length, false⟩ := by
  simp [run, runWith, afterRead, tracked, hash, matching, afterReserve, invalid]

theorem shared_target_stops_before_permission_repair (snapshot : Snapshot) (request : Request) (file : File)
    (tracked : request.tracked = true) (hash : request.currentHash = true) (matching : file.bytes = snapshot.bytes)
    (replacement : Bytes) (valid : validate snapshot request = .accepted replacement) :
    run snapshot request (some file) true none none = ⟨.shared, some file, some file.bytes.length, false⟩ := by
  simp [run, runWith, afterRead, tracked, hash, matching, valid]

theorem final_link_fault_preserves_bytes_after_repair (snapshot : Snapshot) (request : Request) (file : File)
    (tracked : request.tracked = true) (hash : request.currentHash = true) (matching : file.bytes = snapshot.bytes)
    (replacement : Bytes) (valid : validate snapshot request = .accepted replacement) :
    run snapshot request (some file) false (some .afterRepair) none =
      ⟨.afterRepairFault, some { file with readOnly := false }, some file.bytes.length, true⟩ := by
  simp [run, runWith, afterRead, tracked, hash, matching, valid]

end HoiminOracle.WorkspaceMutation
