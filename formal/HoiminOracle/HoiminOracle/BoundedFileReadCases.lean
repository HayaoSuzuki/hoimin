import HoiminOracle.BoundedFileReadModel
import Lean.Data.Json

namespace HoiminOracle.BoundedFileRead

private structure Case where
  id : String
  limit : Nat
  observed : Nat
  bytes : List UInt8

private def payload (size : Nat) : List UInt8 :=
  (List.range size).map fun index => UInt8.ofNat (37 + 73 * index)

private def cases : List Case :=
  let observed := [0, 1, 3, 4, 5, 8, 9, 32]
  let lengths := [0, 1, 3, 4, 5, 8, 9, 16]
  [0, 1, 4, 8].flatMap (fun limit => observed.flatMap fun before => lengths.map fun after =>
    ⟨s!"limit-{limit}-observed-{before}-read-{after}", limit, before, payload after⟩) ++
    [ ⟨"first-artifact-byte-over-real-limit", 64 * 1024 * 1024, 64 * 1024 * 1024 + 1, []⟩ ]

private def bytesJson (bytes : List UInt8) : Lean.Json := Lean.toJson (bytes.map UInt8.toNat)

private def resultJson (result : Result) : Lean.Json :=
  Lean.Json.mkObj (match result with
    | .tooLargeAtOpen => [("kind", .str "too_large_at_open")]
    | .tooLargeWhenRead => [("kind", .str "too_large_when_read")]
    | .accepted bytes => [("kind", .str "accepted"), ("bytes", bytesJson bytes)])

def corpus : String :=
  String.join (cases.map fun item =>
    let expected := acquire item.limit item.observed item.bytes
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("mode", .str "internal-fixture"),
      ("id", .str item.id),
      ("limit", Lean.toJson item.limit),
      ("observed", Lean.toJson item.observed),
      ("bytes", bytesJson item.bytes),
      ("consumed", Lean.toJson expected.consumed),
      ("result", resultJson expected.result)
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × Policy) :=
    [ ("skip_metadata_check", { ordinary with admitMetadata := fun _ _ => true })
    , ("reject_exact_metadata_length", { ordinary with admitMetadata := fun limit size => size < limit })
    , ("reject_exact_payload_length", { ordinary with admitPayload := fun limit size => size < limit })
    , ("no_extra_byte", { ordinary with readLimit := fun limit _ => limit })
    , ("use_open_length_as_limit", { ordinary with readLimit := fun _ observed => observed + 1 })
    , ("read_past_extra_byte", { ordinary with readLimit := fun limit _ => limit + 2 })
    ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => acquireWith variant item.limit item.observed item.bytes !=
        acquire item.limit item.observed item.bytes) then
      IO.println s!"bounded file read sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"bounded file read missed variant: {name}"
      return 1
  IO.println s!"bounded_file_read_cases={cases.length}"
  return 0

end HoiminOracle.BoundedFileRead
