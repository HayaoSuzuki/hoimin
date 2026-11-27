import HoiminOracle.GitPhysicalRowsModel
import Lean.Data.Json

namespace HoiminOracle.GitPhysicalRows

private structure Fixture where
  id : String
  source : List UInt8
  queries : List (List Span)

private def sourcesWithLength : Nat → List (List UInt8)
  | 0 => [[]]
  | n + 1 => ([120, 13, 10] : List UInt8).flatMap fun byte => (sourcesWithLength n).map (byte :: ·)

private def queriesFor (source : List UInt8) : List (List Span) :=
  let last := (layout source).length
  let endpoints := List.range (last + 2)
  let spans := endpoints.flatMap fun start => endpoints.map (Span.mk start)
  let valid := spans.filter fun span => 0 < span.start && span.start ≤ span.finish && span.finish ≤ last
  [] :: spans.map (· :: []) ++ valid.flatMap fun first =>
    (valid.filter fun second => first.finish < second.start).map fun second => [first, second]

private def matrix : List Fixture := (List.range 6).flatMap fun length =>
  (sourcesWithLength length).map fun source =>
    ⟨"bytes-" ++ String.intercalate "-" (source.map (toString ∘ UInt8.toNat)), source, queriesFor source⟩

private def fixtures : List Fixture := matrix ++ [
  ⟨"skip-cr-bearing-git-row", [120,10,97,13,98,13,99,10,100,13,101,10,102],
    [[⟨1,1⟩, ⟨3,4⟩], [⟨2,2⟩, ⟨4,4⟩], [⟨2,4⟩]]⟩,
  ⟨"adjacent-git-ranges", [13,10,120,13,121,10,122], [[⟨1,1⟩, ⟨2,2⟩, ⟨3,3⟩]]⟩,
  ⟨"overlap-rejected", [10,10,10], [[⟨1,2⟩, ⟨2,3⟩]]⟩,
  ⟨"backward-rejected", [10,10,10], [[⟨3,3⟩, ⟨1,1⟩]]⟩,
  ⟨"duplicate-rejected", [10,10,10], [[⟨2,2⟩, ⟨2,2⟩]]⟩,
  ⟨"reversed-diagnostic-before-cursor-check", [10,10], [[⟨2,2⟩, ⟨1,0⟩]]⟩,
  ⟨"first-outside-precedes-later-malformed", [10,10], [[⟨1,3⟩, ⟨0,0⟩]]⟩,
  ⟨"u32-maximum-endpoints", [10,10], [[⟨1,4294967295⟩], [⟨4294967295,4294967295⟩]]⟩,
  ⟨"non-newline-byte-values", [239,187,191,13,255,0,13,10,233,10,128],
    [[⟨1,3⟩], [⟨1,1⟩, ⟨3,3⟩], [⟨2,2⟩]]⟩,
  ⟨"many-cr-in-one-git-row", List.replicate 32 13, [[⟨1,1⟩], [⟨2,2⟩]]⟩]

private def spanJson (span : Span) : Lean.Json := Lean.Json.mkObj [
  ("start", Lean.toJson span.start), ("end", Lean.toJson span.finish)]

private def resultJson : Except Diagnostic (List Span) → Lean.Json
  | .ok spans => Lean.Json.mkObj [("kind", .str "ok"), ("ranges", .arr (spans.toArray.map spanJson))]
  | .error kind => Lean.Json.mkObj [("kind", .str "error"), ("message", .str (match kind with
    | .malformed => "invalid Git line range"
    | .outside => "Git changed-line range does not match current source bytes"))]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("source", Lean.toJson (item.source.map UInt8.toNat)), ("physical_count", Lean.toJson (physicalCount item.source)),
  ("queries", .arr (item.queries.toArray.map fun spans => Lean.Json.mkObj [
    ("git_ranges", .arr (spans.toArray.map spanJson)), ("expected", resultJson (translate item.source spans))]))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

def stats : IO Unit :=
  IO.println s!"physical_row_fixtures={fixtures.length}\nphysical_row_queries={(fixtures.map (·.queries.length)).sum}\nexhaustive_source_bytes=5\nsource_alphabet=3"

private def countCrlfTwice (source : List UInt8) : Nat :=
  let separate := (source.map classify).map fun
    | .cr => Token.cr
    | .lf => Token.lf
    | .payload => Token.payload
  (cut pythonEnd separate).length

private def addTrailingRow (source : List UInt8) : Nat :=
  physicalCount source + (if (source.getLast?.map classify).any (fun
    | .cr | .lf => true
    | .payload => false) then 1 else 0)

private def forgetSkippedCr (source : List UInt8) (ranges : List Span) : Except Diagnostic (List Span) :=
  let widths := (layout source).zipIdx.map fun (width, index) =>
    if ranges.any (fun span => span.start ≤ index + 1 && index + 1 ≤ span.finish) then width else 1
  translateFrom widths 1 ranges

private def allowRewind (source : List UInt8) (ranges : List Span) : Except Diagnostic (List Span) := do
  return (← ranges.mapM (fun span => translate source [span])).flatten

private def clipAtEof (source : List UInt8) (ranges : List Span) : Except Diagnostic (List Span) :=
  let last := (layout source).length
  translate source (ranges.map fun span =>
    if span.start ≤ last then { span with finish := min span.finish last } else span)

def sensitivity : IO UInt32 := do
  let countDiffers (candidate : List UInt8 → Nat) (item : Fixture) :=
    candidate item.source != physicalCount item.source
  let rangesDiffer (candidate : List UInt8 → List Span → Except Diagnostic (List Span)) (item : Fixture) :=
    item.queries.any fun spans =>
      (resultJson (candidate item.source spans)).compress != (resultJson (translate item.source spans)).compress
  for (name, broken) in [
      ("rows_count_lf_only", countDiffers ((·.length) ∘ layout)),
      ("rows_count_crlf_twice", countDiffers countCrlfTwice),
      ("rows_append_phantom_trailing_row", countDiffers addTrailingRow),
      ("rows_forget_skipped_cr", rangesDiffer forgetSkippedCr),
      ("rows_allow_rewind", rangesDiffer allowRewind),
      ("rows_clip_at_eof", rangesDiffer clipAtEof)] do
    match fixtures.find? broken with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git physical-row corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitPhysicalRows
