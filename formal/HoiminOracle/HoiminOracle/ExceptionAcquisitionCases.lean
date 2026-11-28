import HoiminOracle.ExceptionAcquisitionModel
import Lean.Data.Json

namespace HoiminOracle.ExceptionAcquisition

inductive Material
  | ascii (size : Nat)
  | latin1 (count : Nat)
  | badEncoding
  | deep

private def latin1Header := "# coding: latin-1\n# ".toUTF8.toList

private def Material.bytes : Material → List UInt8
  | .ascii 0 => []
  | .ascii (size + 1) => 35 :: List.replicate size 32
  | .latin1 count => latin1Header ++ List.replicate count 255
  | .badEncoding => [255]
  | .deep => ("x = " ++ String.join (List.replicate 160 "1 + ") ++ "1\n").toUTF8.toList

private def Material.decoded : Material → Option Nat
  | .latin1 count => some (latin1Header.length + 2 * count)
  | .badEncoding => none
  | other => some other.bytes.length

inductive Fingerprint | matching | changed | missing

structure File where
  path : String
  contents : Material
  fingerprint : Fingerprint := .matching

structure Attempt where
  replacement : Option (String × Material) := none
  cancelled : Bool := false

structure Fixture where
  id : String
  limits : Limits
  files : List File
  prepared : Bool := true
  attempts : List Attempt := [{}]

private def single : List Fixture :=
  let materials : List (String × Material) := [("empty", .ascii 0), ("ascii20", .ascii 20), ("ascii24", .ascii 24),
    ("latin2", .latin1 2), ("latin4", .latin1 4), ("encoding", .badEncoding)]
  let hashes : List (String × Fingerprint) := [("matching", .matching), ("changed", .changed), ("missing", .missing)]
  let limits : List Limits := [⟨20, 40⟩, ⟨24, 22⟩, ⟨24, 24⟩, ⟨28, 40⟩]
  materials.flatMap fun (name, material) => hashes.flatMap fun (hash, fingerprint) => limits.map fun bound =>
    { id := s!"single-{name}-{hash}-{bound.perFile}-{bound.total}", limits := bound,
      files := [⟨"a.py", material, fingerprint⟩] }

private def pairs : List Fixture :=
  let materials : List (String × Material) := [("empty", .ascii 0), ("ascii12", .ascii 12),
    ("latin2", .latin1 2), ("latin4", .latin1 4)]
  let limits : List Limits := [⟨24, 24⟩, ⟨24, 36⟩, ⟨28, 44⟩]
  materials.flatMap fun (first, a) => materials.flatMap fun (second, b) => limits.map fun bound =>
    { id := s!"pair-{first}-{second}-{bound.perFile}-{bound.total}", limits := bound,
      files := [{ path := "a.py", contents := a }, { path := "b.py", contents := b }] }

private def fixtures : List Fixture := single ++ pairs ++ [
  { id := "zero-remaining-allows-empty", limits := ⟨24, 24⟩,
    files := [{ path := "a.py", contents := .ascii 24 }, { path := "b.py", contents := .ascii 0 }] },
  { id := "later-raw-failure-before-pending-depth", limits := ⟨1024, 1024⟩,
    files := [{ path := "a.py", contents := .deep }, { path := "b.py", contents := .ascii 500 }] },
  { id := "later-fingerprint-failure-before-pending-depth", limits := ⟨1024, 1024⟩,
    files := [{ path := "a.py", contents := .deep }, ⟨"b.py", .ascii 20, .changed⟩] },
  { id := "later-decoding-failure-before-pending-depth", limits := ⟨1024, 1024⟩,
    files := [{ path := "a.py", contents := .deep }, { path := "b.py", contents := .badEncoding }] },
  { id := "pending-depth-after-all-acquisition-succeeds", limits := ⟨1024, 1024⟩,
    files := [{ path := "a.py", contents := .deep }, { path := "b.py", contents := .ascii 1 }] },
  { id := "repair-failure-then-reuse-cache", limits := ⟨24, 24⟩, prepared := false,
    files := [{ path := "a.py", contents := .ascii 30 }], attempts := [{},
      { replacement := some ("a.py", .ascii 24) }, { replacement := some ("a.py", .ascii 31) },
      { cancelled := true }, {}] },
  { id := "cancelled-first-load-can-retry", limits := ⟨24, 24⟩,
    files := [{ path := "a.py", contents := .ascii 24 }], attempts := [{ cancelled := true }, {}] },
  { id := "first-path-error-wins", limits := ⟨24, 48⟩,
    files := [{ path := "z.py", contents := .ascii 30 }, { path := "a.py", contents := .badEncoding }] },
  { id := "next-raw-read-uses-decoded-charge", limits := ⟨32, 40⟩,
    files := [{ path := "a.py", contents := .latin1 4 }, { path := "b.py", contents := .ascii 13 }] }]

private def source (prepared : Bool) (original current : File) : Source :=
  let bytes := current.contents.bytes
  let fingerprint := !prepared || match original.fingerprint with
    | .matching => original.contents.bytes == bytes
    | .changed | .missing => false
  ⟨current.path, bytes.length, fingerprint, current.contents.decoded,
    match current.contents with | .deep => true | _ => false⟩

private def timeline (observe : Limits → Bool → Bool → List Source → Observation) (item : Fixture) : List Observation :=
  let initial := item.files.mergeSort (fun left right => left.path ≤ right.path)
  let rec go (files : List File) (cached : Bool) : List Attempt → List Observation
    | [] => []
    | action :: later =>
      let updated := match action.replacement with
        | none => files
        | some (path, contents) => files.map fun file => if file.path == path then { file with contents } else file
      let observed := observe item.limits cached action.cancelled (initial.zipWith (source item.prepared) updated)
      observed :: go updated observed.cached later
  go initial false item.attempts

private def resultJson (result : Observation) : Lean.Json :=
  let (kind, path) := match result.outcome with
    | .built => ("built", none)
    | .cached => ("cached", none)
    | .failed (.rawLimit path) => ("raw_limit", some path)
    | .failed (.fingerprint path) => ("fingerprint", some path)
    | .failed (.encoding path) => ("encoding", some path)
    | .failed (.decodedFileLimit path) => ("decoded_file_limit", some path)
    | .failed .decodedTotalLimit => ("decoded_total_limit", none)
    | .failed .analysisDepth => ("analysis_depth", none)
    | .failed .cancelled => ("cancelled", none)
  Lean.Json.mkObj [("kind", .str kind), ("path", Lean.toJson path), ("cached", Lean.toJson result.cached)]

private def bytesJson (contents : Material) : Lean.Json := Lean.toJson (contents.bytes.map UInt8.toNat)

private def fileJson (file : File) : Lean.Json :=
  let fingerprint := match file.fingerprint with
    | .matching => "matching"
    | .changed => "changed"
    | .missing => "missing"
  Lean.Json.mkObj [("path", .str file.path), ("bytes", bytesJson file.contents), ("fingerprint", .str fingerprint)]

private def attemptJson (action : Attempt) : Lean.Json := Lean.Json.mkObj [
  ("cancelled", Lean.toJson action.cancelled), ("replacement", (action.replacement.map fun (path, contents) =>
    Lean.Json.mkObj [("path", .str path), ("bytes", bytesJson contents)]).getD .null)]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("per_file", Lean.toJson item.limits.perFile), ("total", Lean.toJson item.limits.total),
  ("prepared", Lean.toJson item.prepared), ("files", .arr (item.files.toArray.map fileJson)),
  ("attempts", .arr (item.attempts.toArray.map attemptJson)),
  ("expected", .arr ((timeline load item).toArray.map resultJson))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def acquireWith (measure : Nat → Nat → Source → Except Failure Nat) (defer : Bool)
    (perFile remaining : Nat) (pendingDepth : Bool) : List Source → Except Failure Nat
  | [] => if pendingDepth then .error .analysisDepth else .ok remaining
  | current :: rest => (measure perFile remaining current).bind fun next =>
    let depth := pendingDepth || current.depthFailure
    if depth && !defer then .error .analysisDepth else acquireWith measure defer perFile next depth rest

def sensitivity : IO UInt32 := do
  let via (measure : Nat → Nat → Source → Except Failure Nat) (defer : Bool) :=
    loadWith fun limits => acquireWith measure defer limits.perFile limits.total false
  for (name, broken) in [
      ("acquisition_charge_raw_bytes", via (fun per remaining input =>
        (charge per remaining input).map (fun _ => remaining - input.rawBytes)) true),
      ("acquisition_skip_decoded_per_file_limit", via (fun per remaining input =>
        match charge per remaining input with
        | .error (.decodedFileLimit _) =>
          let size := input.decodedBytes.getD 0
          if size ≤ remaining then .ok (remaining - size) else .error .decodedTotalLimit
        | result => result) true),
      ("acquisition_decode_before_fingerprint", via (fun per remaining input =>
        if input.decodedBytes.isNone then charge per remaining { input with fingerprintMatches := true }
        else charge per remaining input) true),
      ("acquisition_report_analysis_failure_early", via charge false),
      ("acquisition_use_cache_before_cancel", fun limits cached cancelled sources =>
        load limits cached (cancelled && !cached) sources),
      ("acquisition_cache_failed_build", fun limits cached cancelled sources =>
        let result := load limits cached cancelled sources
        { result with cached := result.cached || !cancelled })] do
    match fixtures.find? (fun item =>
      (timeline broken item).map (Lean.Json.compress ∘ resultJson) !=
        (timeline load item).map (Lean.Json.compress ∘ resultJson)) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"exception acquisition corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.ExceptionAcquisition
