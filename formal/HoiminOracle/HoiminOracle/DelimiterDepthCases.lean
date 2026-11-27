import HoiminOracle.DelimiterDepthModel
import Lean.Data.Json

namespace HoiminOracle.DelimiterDepth

private structure Block where
  initial : String
  openings : List Char
  body : String

private def closing : Char → Char
  | '[' => ']'
  | '{' => '}'
  | _ => ')'

private def Block.render (block : Block) : String :=
  block.initial ++ String.ofList block.openings ++ block.body ++
    String.ofList (block.openings.reverse.map closing) ++ "\n"

private def Block.pairs (block : Block) (base : Nat) : List Pair :=
  let first := base + block.initial.utf8ByteSize
  let afterLast := first + 2 * block.openings.length + block.body.utf8ByteSize
  (List.range block.openings.length).map fun offset => (first + offset, afterLast - offset)

private def layout : List Block → Nat → List Pair
  | [], _ => []
  | block :: rest, base => block.pairs base ++ layout rest (base + block.render.utf8ByteSize)

private def parens (depth : Nat) : List Char := List.replicate depth '('

private def programs : List (String × List Block) :=
  (List.range 11).map (fun n =>
    let depth := 190 + n
    (s!"parentheses-{depth}", [⟨"value = ", parens depth, "1"⟩])) ++
  [("mixed-with-utf8-and-crlf", [⟨"# 雪 [({\r\nvalue = ",
      parens 197 ++ ['[', '{', '('], "1"⟩]),
   ("two-peaks", [⟨"left = ", parens 200, "1"⟩, ⟨"right = ", parens 199, "2"⟩]),
   ("literal-and-comment", [⟨"# [({\nvalue = ", parens 193, "\"雪[({)]}\""⟩]),
   ("shallow", [⟨"value = ", ['[', '('], "1"⟩])]

private structure Fixture where
  id : String
  source : String
  pairs : List Pair

private def cases : List Fixture := programs.map fun (id, blocks) =>
  ⟨id, String.join (blocks.map Block.render), layout blocks 0⟩

private def additions := (List.range 8).map (· + 1)

private def ranges (item : Fixture) : List Pair :=
  let width := item.source.utf8ByteSize
  let boundaryPairs := item.pairs.filter fun (opening, _) => 192 ≤ depthAt item.pairs opening
  let points := boundaryPairs.flatMap fun (opening, closing) =>
    [opening - 1, opening, opening + 1, closing - 1, closing, closing + 1]
  let starts := (0 :: (width / 2) :: width :: points).eraseDups
  ((0, width) :: boundaryPairs ++ starts.flatMap (fun offset =>
    [(offset, offset), (offset, min width (offset + 1))])).eraseDups

private def resultJson (item : Fixture) : Lean.Json :=
  let queries := additions.flatMap fun added => (ranges item).map fun (start, stop) =>
    Lean.Json.mkObj [("added", Lean.toJson added), ("start", Lean.toJson start), ("end", Lean.toJson stop),
      ("can_enclose", Lean.toJson (canEnclose item.pairs start stop added)),
      ("room_before", Lean.toJson (roomBefore item.pairs start added))]
  let counts := additions.map fun added =>
    (item.pairs.filter fun (opening, _) => depthAt item.pairs opening + added = limit + 1).length
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str item.id), ("source", .str item.source), ("bucket_counts", Lean.toJson counts),
    ("queries", .arr queries.toArray)]

def corpus : String := String.join (cases.map fun item => (resultJson item).compress ++ "\n")

private def observe (item : Fixture) (start stop added : Nat) : Bool × Bool :=
  (canEnclose item.pairs start stop added, roomBefore item.pairs start added)

private def observations (pairs : List Pair) (start stop added : Nat) : Bool × Bool :=
  (canEnclose pairs start stop added, roomBefore pairs start added)

def sensitivity : IO UInt32 := do
  for (family, changed) in [
      ("delimiter_reject_exact_limit", fun item start stop added => observe item start stop (added + 1)),
      ("delimiter_insertion_counts_opening", fun item start stop added =>
        ((observe item start stop added).1, depthAt item.pairs start + added ≤ limit)),
      ("delimiter_enclosure_counts_exclusive_end", fun item start stop added =>
        (canEnclose item.pairs start (stop + 1) added, (observe item start stop added).2)),
      ("delimiter_enclosure_checks_start_only", fun item start _ added =>
        let room := roomBefore item.pairs start added
        (room, room)),
      ("delimiter_ignores_later_peak", fun item start stop added =>
        let firstEnd := (item.pairs.head?.map Prod.snd).getD 0
        observations (item.pairs.takeWhile fun pair => pair.1 < firstEnd) start stop added),
      ("delimiter_round_only", fun item start stop added =>
        observations (item.pairs.filter fun pair => item.source.toUTF8[pair.1]? == some 40) start stop added)] do
    let witness := cases.findSome? fun item =>
      additions.findSome? fun added => (ranges item).findSome? fun (start, stop) =>
        if changed item start stop added != observe item start stop added
        then some s!"{item.id} start={start} end={stop} added={added}" else none
    match witness with
    | some detail => IO.println s!"family={family} detected=true case={detail}"
    | none =>
      IO.eprintln s!"delimiter depth corpus missed broken family {family}"
      return 2
  return 0

end HoiminOracle.DelimiterDepth
