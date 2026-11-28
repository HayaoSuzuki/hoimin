import HoiminOracle.LineCoverageModel
import Lean.Data.Json

namespace HoiminOracle.LineCoverage

private structure Fixture where
  id : String
  selected : List Entry
  queries : List Entry
  targets : List Target := []

private def bounds : List Span :=
  (List.range 4).flatMap fun start => (List.range 4).map (Span.mk start)

private def matrix : List Fixture :=
  bounds.flatMap fun first => bounds.map fun second => {
    id := s!"pair-{first.start}-{first.finish}-{second.start}-{second.finish}"
    selected := [⟨.first, first⟩, ⟨.first, second⟩]
    queries := bounds.map (Entry.mk .first)
  }

private def maxLine : Nat := 4294967295

private def fixtures : List Fixture := matrix ++ [
  { id := "empty-index", selected := [],
    queries := [⟨.first, ⟨0, 0⟩⟩, ⟨.second, ⟨1, 1⟩⟩],
    targets := [⟨.first, [⟨1, 1⟩]⟩] },
  { id := "three-unsorted-intervals", selected := [
      ⟨.first, ⟨7, 9⟩⟩, ⟨.first, ⟨1, 4⟩⟩, ⟨.first, ⟨4, 7⟩⟩, ⟨.first, ⟨1, 4⟩⟩],
    queries := [⟨.first, ⟨1, 9⟩⟩, ⟨.first, ⟨1, 10⟩⟩] },
  { id := "separate-files", selected := [⟨.first, ⟨1, 2⟩⟩, ⟨.second, ⟨3, 4⟩⟩],
    queries := [⟨.first, ⟨1, 4⟩⟩, ⟨.second, ⟨3, 4⟩⟩],
    targets := [⟨.first, [⟨1, 4⟩]⟩, ⟨.second, [⟨3, 4⟩]⟩] },
  { id := "all-slices-covered", selected := [⟨.first, ⟨1, 2⟩⟩, ⟨.first, ⟨5, 8⟩⟩],
    queries := [⟨.first, ⟨2, 5⟩⟩],
    targets := [⟨.first, [⟨1, 1⟩, ⟨5, 6⟩]⟩, ⟨.first, [⟨7, 8⟩]⟩] },
  { id := "covered-then-uncovered-slice", selected := [⟨.first, ⟨1, 3⟩⟩],
    queries := [⟨.first, ⟨2, 4⟩⟩],
    targets := [⟨.first, [⟨1, 2⟩]⟩, ⟨.first, [⟨2, 4⟩]⟩, ⟨.first, [⟨1, 3⟩]⟩] },
  { id := "uncovered-then-covered-slice", selected := [⟨.first, ⟨1, 3⟩⟩],
    queries := [⟨.first, ⟨2, 4⟩⟩],
    targets := [⟨.first, [⟨2, 4⟩]⟩, ⟨.first, [⟨1, 2⟩]⟩] },
  { id := "empty-slice-disqualifies-only-its-file", selected := [
      ⟨.first, ⟨1, 3⟩⟩, ⟨.second, ⟨1, 3⟩⟩],
    queries := [],
    targets := [⟨.first, [⟨1, 3⟩]⟩, ⟨.first, []⟩, ⟨.second, [⟨1, 3⟩]⟩] },
  { id := "reversed-query", selected := [⟨.first, ⟨1, 4⟩⟩],
    queries := [⟨.first, ⟨3, 2⟩⟩], targets := [⟨.first, [⟨3, 2⟩]⟩] },
  { id := "typed-zero-is-filtered-only-for-changes", selected := [
      ⟨.first, ⟨0, 2⟩⟩, ⟨.first, ⟨3, 4⟩⟩],
    queries := [⟨.first, ⟨0, 4⟩⟩], targets := [⟨.first, [⟨0, 1⟩]⟩] },
  { id := "adjacent-at-max", selected := [
      ⟨.first, ⟨maxLine, maxLine⟩⟩, ⟨.first, ⟨maxLine - 2, maxLine - 1⟩⟩,
      ⟨.first, ⟨maxLine, maxLine⟩⟩],
    queries := [⟨.first, ⟨maxLine - 2, maxLine⟩⟩, ⟨.first, ⟨maxLine - 3, maxLine⟩⟩] },
  { id := "gap-before-max", selected := [
      ⟨.first, ⟨maxLine - 2, maxLine - 2⟩⟩, ⟨.first, ⟨maxLine, maxLine⟩⟩],
    queries := [⟨.first, ⟨maxLine - 2, maxLine⟩⟩, ⟨.first, ⟨maxLine, maxLine⟩⟩] },
  { id := "wide-gap", selected := [⟨.first, ⟨1, 1⟩⟩, ⟨.first, ⟨maxLine, maxLine⟩⟩],
    queries := [⟨.first, ⟨1, maxLine⟩⟩, ⟨.second, ⟨1, maxLine⟩⟩] }]

private def pathName : Path → String
  | .first => "src/a.py" | .second => "src/b.py"

private def spanJson (span : Span) : Lean.Json := Lean.Json.mkObj [
  ("start", Lean.toJson span.start), ("end", Lean.toJson span.finish)]

private def entryJson (entry : Entry) : Lean.Json := Lean.Json.mkObj [
  ("path", .str (pathName entry.path)), ("range", spanJson entry.span)]

private def normalized (item : Fixture) : List Entry :=
  ([.first, .second] : List Path).flatMap fun path =>
    (normalizedChanges (rangesAt item.selected path)).map (Entry.mk path)

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("selected", .arr (item.selected.toArray.map entryJson)),
  ("queries", .arr (item.queries.toArray.map entryJson)),
  ("targets", .arr (item.targets.toArray.map fun target => Lean.Json.mkObj [
    ("path", .str (pathName target.path)), ("lines", .arr (target.spans.toArray.map spanJson))])),
  ("expected", Lean.Json.mkObj [
    ("queries", .arr (item.queries.toArray.map fun query =>
      let spans := rangesAt item.selected query.path
      Lean.Json.mkObj [("covers", Lean.toJson (covers spans query.span)),
        ("contains_start", Lean.toJson (coveredAt spans query.span.start))])),
    ("normalized_changes", .arr ((normalized item).toArray.map entryJson)),
    ("covered_paths", .arr ((targetPaths item.selected item.targets).toArray.map
      fun path => .str (pathName path)))])]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def onlyEndpoints (spans : List Span) (query : Span) : Bool :=
  query.start ≤ query.finish && coveredAt spans query.start && coveredAt spans query.finish

private def requireOneSpan (spans : List Span) (query : Span) : Bool :=
  query.start ≤ query.finish &&
    spans.any fun span => span.start ≤ query.start && query.finish ≤ span.finish

private def acceptAnySlice (item : Fixture) : List Path :=
  ([.first, .second] : List Path).filter fun path =>
    item.targets.any fun target => target.path == path &&
      !target.spans.isEmpty && target.spans.all (covers (rangesAt item.selected path))

def sensitivity : IO UInt32 := do
  let queryDiffers (candidate : List Span → Span → Bool) (item : Fixture) :=
    item.queries.any fun query =>
      let spans := rangesAt item.selected query.path
      candidate spans query.span != covers spans query.span
  for (name, broken) in [("check_endpoints_only", queryDiffers onlyEndpoints),
      ("require_one_input_span", queryDiffers requireOneSpan),
      ("accept_any_target_slice", fun item => acceptAnySlice item != targetPaths item.selected item.targets)] do
    match fixtures.find? broken with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"line coverage corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.LineCoverage
