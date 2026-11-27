import HoiminOracle.CandidateSelectionModel
import Lean.Data.Json

namespace HoiminOracle.CandidateSelection

open LineCoverage (Span)
open SymbolCatalog (Name Selector selected dotted)

private def current : Name := ["pkg", "subject"]
private def largest := 4294967295

private def rangeSets : List (String × List Span) := [
  ("all", []), ("reversed", [⟨4, 3⟩, ⟨9, 6⟩]),
  ("zero", [⟨0, 0⟩]), ("point", [⟨4, 4⟩]), ("interval", [⟨3, 6⟩]),
  ("nested", [⟨7, 8⟩, ⟨2, 11⟩, ⟨4, 4⟩]),
  ("same-start", [⟨3, 3⟩, ⟨3, 9⟩, ⟨3, 6⟩, ⟨3, 3⟩]),
  ("gaps", [⟨8, 10⟩, ⟨2, 4⟩]), ("mixed", [⟨6, 4⟩, ⟨8, 8⟩, ⟨9, 8⟩]),
  ("maximum", [⟨largest, largest⟩]), ("entire", [⟨0, largest⟩])]

private def selectionSets : List (String × List Selector) := [
  ("all", []), ("ancestor", [⟨["outer"], none⟩]),
  ("inner", [⟨["outer", "chosen"], none⟩]),
  ("qualified", [⟨["outer"], some ["subject"]⟩]),
  ("mismatch", [⟨["outer"], some ["ubject"]⟩]),
  ("union", [⟨["outer", "chosen"], some current⟩, ⟨["other"], none⟩, ⟨["outerish"], some ["foreign"]⟩])]

private def spans : List Span := [
  ⟨0, 0⟩, ⟨1, 1⟩, ⟨1, 3⟩, ⟨3, 5⟩, ⟨4, 4⟩, ⟨5, 5⟩, ⟨6, 6⟩, ⟨6, 8⟩,
  ⟨8, 8⟩, ⟨9, 9⟩, ⟨10, 10⟩, ⟨11, 11⟩, ⟨12, 12⟩, ⟨0, largest⟩,
  ⟨largest - 1, largest - 1⟩, ⟨largest - 1, largest⟩, ⟨largest, largest⟩]

private def scopes : List Name :=
  [[], ["outer"], ["outer", "chosen"], ["outer", "chosen", "leaf"], ["outer", "chosenish"], ["other"]]

private def spanJson (span : Span) : Lean.Json := Lean.Json.arr #[Lean.toJson span.start, Lean.toJson span.finish]

private def selectorText (selector : Selector) : String :=
  (selector.modulePath.map fun path => dotted path ++ ":").getD "" ++ dotted selector.name

private def inputs (ranges : List Span) (selectors : List Selector) : List (String × Lean.Json) :=
  [("lines", .arr (ranges.map spanJson).toArray), ("symbols", Lean.toJson (selectors.map selectorText))]

private def row (kind id : String) (fields : List (String × Lean.Json)) : String :=
  (Lean.Json.mkObj ([("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("kind", .str kind), ("id", .str id)] ++ fields)).compress ++ "\n"

private def intervalCorpus : List String := rangeSets.flatMap fun (rangeId, ranges) =>
  selectionSets.map fun (selectorId, selectors) =>
    let queries := spans.flatMap fun span => scopes.map fun scope => Lean.Json.mkObj [
      ("span", spanJson span), ("symbol", if scope.isEmpty then .null else .str (dotted scope)),
      ("include", Lean.toJson (admitted ranges selectors current scope span))]
    row "interval" (rangeId ++ "-" ++ selectorId) (inputs ranges selectors ++ [("queries", .arr queries.toArray)])

private structure RegexCase where
  id : String
  source : String
  start : Nat
  original : String
  replacement : String
  span : Span
  pattern : Span
  flags : Span

private def regexCase (id eol : String) (flagsFirst : Bool) : RegexCase :=
  let head := String.intercalate eol ["import re", "def outer():", "    def chosen(s):", "        return re.match("]
  if flagsFirst then
    let initial := head ++ "flags="
    let middle := "," ++ eol ++ "            pattern=r'''(?"
    let suffix := ")\\d" ++ eol ++ "é'''," ++ eol ++ "            string=s)" ++ eol
    let original := "re.A" ++ middle ++ "a"
    let replacement := "32" ++ middle ++ "u"
    ⟨id ++ "-flags-first", initial ++ original ++ suffix, initial.utf8ByteSize, original, replacement,
      ⟨4, 5⟩, ⟨5, 6⟩, ⟨4, 4⟩⟩
  else
    let initial := head ++ "pattern=r'''(?"
    let middle := ")\\d" ++ eol ++ "é'''," ++ eol ++ "            flags="
    let original := "a" ++ middle ++ "re.A"
    let replacement := "u" ++ middle ++ "32"
    ⟨id ++ "-pattern-first", initial ++ original ++ ", string=s)" ++ eol, initial.utf8ByteSize,
      original, replacement, ⟨4, 6⟩, ⟨4, 5⟩, ⟨6, 6⟩⟩

private def regexFixtures : List RegexCase :=
  [("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r")].flatMap fun (id, eol) =>
    [regexCase id eol true, regexCase id eol false]

private def regexRanges : List (List Span) := [
  [], [⟨3, 3⟩], [⟨4, 4⟩], [⟨5, 5⟩], [⟨6, 6⟩], [⟨7, 7⟩],
  [⟨6, 6⟩, ⟨3, 3⟩], [⟨6, 7⟩, ⟨2, 5⟩, ⟨4, 3⟩], [⟨6, 5⟩]]

private def regexScope : Name := ["outer", "chosen"]

private def regexObserve (inspection emission : Span → Span) (item : RegexCase)
    (ranges : List Span) (selectors : List Selector) : Bool × Bool :=
  let allowed := admitted ranges selectors current regexScope
  let inspects := allowed (inspection item.pattern) || allowed (inspection item.flags)
  (inspects, inspects && allowed (emission item.span))

private def firstLine (span : Span) : Span := ⟨span.start, span.start⟩

private def regexCorpus : List String := regexFixtures.map fun item =>
  let queries := regexRanges.flatMap fun ranges => selectionSets.map fun (_, selectors) =>
    let (inspects, emits) := regexObserve id firstLine item ranges selectors
    Lean.Json.mkObj (inputs ranges selectors ++
      [("inspects", Lean.toJson inspects), ("emits", Lean.toJson emits)])
  let edit := Lean.Json.mkObj [("start", Lean.toJson item.start), ("original", .str item.original),
    ("replacement", .str item.replacement), ("line", Lean.toJson item.span.start), ("symbol", .str (dotted regexScope))]
  row "regex" item.id [("source", .str item.source), ("edit", edit), ("queries", .arr queries.toArray)]

def corpus : String := String.join (intervalCorpus ++ regexCorpus)

def sensitivity : IO UInt32 := do
  for (family, broken) in [
      ("line_count_filtered_empty_as_all", fun ranges selectors scope span =>
        lineSelected (prepared ranges) span && selected current selectors scope),
      ("line_use_last_end_instead_of_prefix_max", fun ranges selectors scope span =>
        (ranges.isEmpty || ((prepared ranges).takeWhile (startsBefore span.finish)).getLast?.any
          (fun range => span.start ≤ range.finish)) && selected current selectors scope),
      ("line_exclude_boundaries", fun ranges selectors scope span =>
        (ranges.isEmpty || ranges.any (fun range => range.start ≤ range.finish &&
          range.start < span.finish && span.start < range.finish)) && selected current selectors scope),
      ("line_union_with_symbol", fun ranges selectors scope span =>
        lineSelected ranges span || selected current selectors scope)] do
    let witness := rangeSets.findSome? fun (rangeId, ranges) =>
      selectionSets.findSome? fun (selectorId, selectors) => spans.findSome? fun span =>
        scopes.findSome? fun scope =>
          if admitted ranges selectors current scope span != broken ranges selectors scope span
          then some s!"{rangeId}-{selectorId} scope={dotted scope} span={span.start}..{span.finish}"
          else none
    match witness with
    | some detail => IO.println s!"family={family} detected=true case={detail}"
    | none =>
      IO.eprintln s!"candidate selection missed broken family {family}"
      return 2
  for (family, inspect, emit) in [
      ("line_inspect_regex_start_only", firstLine, firstLine),
      ("line_emit_regex_by_overlap", id, id)] do
    let witness := regexFixtures.findSome? fun item =>
      regexRanges.findSome? fun ranges => selectionSets.findSome? fun (selectorId, selectors) =>
        if regexObserve id firstLine item ranges selectors != regexObserve inspect emit item ranges selectors
        then some s!"{item.id}-{selectorId}" else none
    match witness with
    | some detail => IO.println s!"family={family} detected=true case={detail}"
    | none =>
      IO.eprintln s!"regex selection missed broken family {family}"
      return 2
  return 0

end HoiminOracle.CandidateSelection
