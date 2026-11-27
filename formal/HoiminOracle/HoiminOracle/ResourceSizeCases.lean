import HoiminOracle.ResourceSizeModel
import Lean.Data.Json

namespace HoiminOracle.ResourceSize

private structure Fixture where
  id : String
  request : Request

private def unitInputs : List (String × String) :=
  units.flatMap fun (unit, multiplier) =>
    let largest := maxBytes / multiplier
    [("zero", 0), ("one", 1), ("largest", largest), ("overflow", largest + 1)].map
      fun (label, amount) => (s!"{unit}-{label}", s!"{amount}{unit}")

private def literalInputs : List (String × String) := [
  ("bytes-without-unit", "17"), ("bare-maximum", toString maxBytes),
  ("bare-overflow", toString (maxBytes + 1)), ("plus", "+1B"), ("minus", "-1B"),
  ("leading-space", " 1B"), ("trailing-space", "1B "), ("plus-only", "+"),
  ("underscores", "1_000B"), ("fraction", "1.5KB"), ("non-ascii-digit", "１KB"),
  ("lowercase-unit", "1kb"), ("unknown-unit", "1TB"), ("leading-zeros", "0001KiB"),
  ("missing-digits", "B"), ("repeated-suffix", "1KiBB"), ("embedded-plus", "1+2"),
  ("long-leading-zeros", "000000000000000000001B"), ("empty", "")]

private def singleFieldCases : List Fixture :=
  fields.flatMap fun field =>
    (unitInputs ++ literalInputs).map fun (label, input) =>
      ⟨s!"{field.flag}-{label}", { sizes := [(field, input)] }⟩

private def fieldPairs : List (Field × Field) :=
  fields.zipIdx.flatMap fun (first, index) => (fields.drop (index + 1)).map (first, ·)

private def conflictCases : List Fixture :=
  fieldPairs.flatMap fun (first, second) =>
    [⟨s!"{first.flag}-{second.flag}-both-malformed",
        { sizes := [(second, "malformed-second"), (first, "malformed-first")] }⟩,
     ⟨s!"{first.flag}-{second.flag}-zero-and-malformed",
        { sizes := [(first, "0"), (second, "malformed-second")] }⟩,
     ⟨s!"{first.flag}-{second.flag}-both-zero",
        { sizes := [(second, "0"), (first, "0")] }⟩]

private def fixtures : List Fixture := singleFieldCases ++ conflictCases ++ [
  ⟨"defaults", {}⟩,
  ⟨"malformed-size-before-zero-jobs", { jobs := 0, sizes := [(.reserve, "?")] }⟩,
  ⟨"zero-jobs-before-zero-processes-and-size",
    { jobs := 0, processes := 0, sizes := [(.reserve, "0")] }⟩,
  ⟨"zero-processes-before-jobs-cap", { jobs := 257, processes := 0 }⟩,
  ⟨"jobs-cap-before-zero-size", { jobs := 257, sizes := [(.memory, "0")] }⟩,
  ⟨"jobs-cap-passes-with-one-process", { jobs := 256, processes := 1 }⟩,
  ⟨"process-cap-inclusive", { processes := 4294967295 }⟩]

private def semantic (field : String) : Lean.Json :=
  Lean.Json.mkObj [("kind", .str "invalid_limit"), ("field", .str field)]

private def resultJson : Except Rejection Limits → Lean.Json
  | .error (.invalidBytes field value) => Lean.Json.mkObj [
      ("kind", .str "invalid_bytes"), ("flag", .str s!"--{field.flag}"), ("value", .str value)]
  | .error (.zeroSize field) => semantic (field.flag.replace "-" "_")
  | .error .zeroJobs => semantic "jobs"
  | .error .invalidProcesses => semantic "max_processes"
  | .error (.jobsAboveMaximum jobs) => Lean.Json.mkObj [
      ("kind", .str "jobs_above_maximum"), ("jobs", Lean.toJson jobs), ("maximum", Lean.toJson (256 : Nat))]
  | .ok limits => Lean.Json.mkObj [
      ("kind", .str "accepted"),
      ("sizes", Lean.Json.mkObj (limits.sizes.map fun (field, bytes) => (field.flag, Lean.toJson bytes))),
      ("jobs", Lean.toJson limits.jobs), ("max_processes", Lean.toJson limits.processes)]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("sizes", .arr (item.request.sizes.toArray.map fun (field, value) =>
    .arr #[.str field.flag, .str value])),
  ("jobs", Lean.toJson item.request.jobs), ("max_processes", Lean.toJson item.request.processes),
  ("expected", resultJson (resolve item.request))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def clamp (value : String) : Option Nat := (scaledBytes value).map (min maxBytes)

private def binaryIsDecimal (value : String) : Option Nat :=
  parseBytes (((value.replace "KiB" "KB").replace "MiB" "MB").replace "GiB" "GB")

private def reverseDiagnostics (request : Request) : Except Rejection Limits :=
  match resolveWith parseBytes fields.reverse request with
  | .error issue => .error issue
  | .ok _ => resolve request

private def different (first second : Except Rejection Limits) : Bool :=
  match first, second with
  | .ok a, .ok b => a != b
  | .error a, .error b => a != b
  | _, _ => true

def sensitivity : IO UInt32 := do
  for (name, candidate) in [
    ("clamp_size_overflow", resolveWith clamp fields),
    ("binary_units_as_decimal", resolveWith binaryIsDecimal fields),
    ("allow_digit_separators", resolveWith (fun value => parseBytes (value.replace "_" "")) fields),
    ("reverse_size_diagnostics", reverseDiagnostics)] do
    match fixtures.find? (fun item => different (candidate item.request) (resolve item.request)) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"resource size corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.ResourceSize
