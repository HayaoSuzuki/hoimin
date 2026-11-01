import SliceTupleModel
import Lean.Data.Json

open SliceTuple

def lists : Nat → List (List Item)
  | 0 => [[]]
  | n+1 => [Item.scalar, .slice].flatMap fun item => (lists n).map (item :: ·)

def main (args : List String) : IO UInt32 := do
  let domain := [1, 2, 3].flatMap lists
  let broken := domain.filter fun items => eligible items != brokenEligible items
  unless broken.head? == some [.slice] do return 2
  IO.eprintln s!"alphabet=2 lengths=1..3 cases={domain.length} mismatches={broken.length} first={repr (broken.head?)}"
  let rows := domain.zipIdx.map fun (items, i) =>
    let inner := String.intercalate ", " (items.map fun item => if item == Item.scalar then "1" else ":")
    Lean.Json.mkObj [
      ("id", .str s!"slice-tuple-{i}"), ("mode", .str "strict"),
      ("source", .str ("def f(x):\n    return x[" ++ inner ++ ",]\n")),
      ("expected_candidate_count", Lean.toJson (if eligible items then 1 else (0 : Nat)))]
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path (String.join (rows.map fun row => row.compress ++ "\n"))
    return 0
  | _ => return 1
