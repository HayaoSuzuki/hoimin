import HoiminOracle.SessionHeaderModel
import Lean.Data.Json

namespace HoiminOracle.SessionHeader

private structure Case where
  id : String
  reads : List Read
  file : Option (List UInt8) := none

private def header (pageHigh pageLow write read : UInt8) : List UInt8 :=
  magic ++ [pageHigh, pageLow, write, read]

private def wal : List UInt8 := header 16 0 2 2

private def fileCase (id : String) (bytes : List UInt8) : Case :=
  ⟨id, [.bytes bytes, .bytes []], some bytes⟩

private def files : List Case :=
  ((List.range (headerSize + 1)).map fun length =>
    fileCase s!"wal-prefix-{length}" (wal.take length)) ++
  ((List.range 16).map fun index =>
    fileCase s!"magic-byte-{index}-changed" (wal.set index 255)) ++
  ([0, 1, 2, 3, 255].flatMap fun write =>
    [0, 1, 2, 3, 255].map fun read =>
      fileCase s!"write-version-{write.toNat}-read-version-{read.toNat}" (header 16 0 write read)) ++
  [ fileCase "zero-page-size" (header 0 0 2 2)
  , fileCase "65536-byte-page-size-marker" (header 0 1 2 2)
  , fileCase "arbitrary-page-size" (header 255 255 2 2)
  , fileCase "wal-trailing-bytes" (wal ++ List.replicate 25 255)
  , fileCase "later-wal-magic-does-not-count" (List.replicate 20 1 ++ wal) ]

private def streams : List Case :=
  [ ⟨"one-byte-chunks", wal.map (fun byte => .bytes [byte]), none⟩
  , ⟨"version-byte-arrives-last", [.bytes (wal.take 19), .bytes (wal.drop 19)], none⟩
  , ⟨"interruption-after-partial-read", [.bytes (wal.take 9), .failed .interrupted,
      .bytes (wal.drop 9)], none⟩
  , ⟨"interruptions-before-and-after-partial-read", [.failed .interrupted, .bytes (wal.take 15),
      .failed .interrupted, .failed .interrupted, .bytes (wal.drop 15)], none⟩
  , ⟨"interrupted-then-empty", [.failed .interrupted, .bytes []], none⟩
  , ⟨"explicit-unexpected-eof-after-prefix", [.bytes (wal.take 19), .failed .unexpectedEof,
      .bytes (wal.drop 19)], none⟩
  , ⟨"empty-chunk-is-terminal", [.bytes [], .bytes wal], none⟩
  , ⟨"permission-error-after-prefix", [.bytes (wal.take 19), .failed .permissionDenied,
      .bytes (wal.drop 19)], none⟩
  , ⟨"malformed-magic-does-not-hide-read-error", [.bytes [0], .failed .invalidData], none⟩
  , ⟨"permission-error-before-any-byte", [.failed .permissionDenied], none⟩
  , ⟨"error-after-full-header-is-not-read", [.bytes wal, .failed .invalidData], none⟩
  , ⟨"one-read-copies-only-header", [.bytes (wal ++ List.replicate 50 255),
      .failed .permissionDenied], none⟩ ]

private def cases := files ++ streams

private def Fault.name : Fault → String
  | .interrupted => "interrupted"
  | .unexpectedEof => "unexpected_eof"
  | .permissionDenied => "permission_denied"
  | .invalidData => "invalid_data"

private def bytesJson (bytes : List UInt8) : Lean.Json := Lean.toJson (bytes.map UInt8.toNat)

private def readJson : Read → Lean.Json
  | .bytes bytes => Lean.Json.mkObj [("kind", .str "bytes"), ("data", bytesJson bytes)]
  | .failed fault => Lean.Json.mkObj [("kind", .str "failed"), ("fault", .str fault.name)]

private def outcomeJson : Outcome → Lean.Json
  | .immutable => Lean.Json.mkObj [("kind", .str "immutable")]
  | .ordinary => Lean.Json.mkObj [("kind", .str "ordinary")]
  | .failed fault => Lean.Json.mkObj [("kind", .str "failed"), ("fault", .str fault.name)]
  | .incompleteScript => .null

def corpus : String :=
  String.join (cases.map fun item =>
    let observation := run [] item.reads
    (Lean.Json.mkObj
      [ ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id)
      , ("reads", .arr (item.reads.map readJson).toArray)
      , ("file", (item.file.map bytesJson).getD .null)
      , ("outcome", outcomeJson observation.outcome)
      , ("requests", Lean.toJson observation.requests) ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Case → Observation)) :=
    [ ("skip_magic_nul", fun item => readWith (fun bytes =>
        bytes.take 15 == magic.take 15 && bytes[18]? == some 2 && bytes[19]? == some 2) onFault [] item.reads)
    , ("either_version_byte", fun item => readWith (fun bytes =>
        bytes.take 16 == magic && (bytes[18]? == some 2 || bytes[19]? == some 2)) onFault [] item.reads)
    , ("validate_page_size_during_routing", fun item => readWith (fun bytes =>
        isWal bytes && bytes[16]? == some 16 && bytes[17]? == some 0) onFault [] item.reads)
    , ("interruption_is_fatal", fun item => readWith isWal (fun fault =>
        if fault == .interrupted then .finish (.failed fault) else onFault fault) [] item.reads)
    , ("eof_is_fatal", fun item => readWith isWal (fun fault =>
        if fault == .unexpectedEof then .finish (.failed fault) else onFault fault) [] item.reads)
    , ("treat_io_error_as_short_header", fun item => readWith isWal (fun fault =>
        if fault == .interrupted then .retry else .finish .ordinary) [] item.reads) ]
  for (name, variant) in variants do
    if let some item := cases.find? (fun item => decide (variant item ≠ run [] item.reads)) then
      IO.println s!"session header sensitivity: {name}: {item.id}"
    else
      IO.eprintln s!"session header missed variant: {name}"
      return 1
  unless cases.all (fun item => decide ((run [] item.reads).outcome ≠ .incompleteScript)) do
    IO.eprintln "incomplete session header trace"
    return 1
  IO.println s!"session_header_cases={cases.length} files={files.length} streams={streams.length}"
  return 0

end HoiminOracle.SessionHeader
