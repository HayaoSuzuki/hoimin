import HoiminOracle.BufferedComparisonModel
import Lean.Data.Json

namespace HoiminOracle.BufferedComparison

private def stream (chunks : List (List UInt8)) (fault : Option Fault := none) : Stream :=
  chunks.foldr (fun chunk rest =>
    match chunk with
    | [] => rest
    | a :: as' => .chunk a as' rest) (.terminal fault)

private def inputs : List (String × Stream) :=
  let a : UInt8 := 97
  let b : UInt8 := 98
  let c : UInt8 := 99
  let d : UInt8 := 100
  let x : UInt8 := 120
  [ ("empty", stream [])
  , ("interrupted", stream [] (some .interrupted))
  , ("io-error", stream [] (some .ioError))
  , ("a", stream [[a]])
  , ("b", stream [[b]])
  , ("ab", stream [[a, b]])
  , ("a-b", stream [[a], [b]])
  , ("abc", stream [[a, b, c]])
  , ("a-bc", stream [[a], [b, c]])
  , ("ab-c", stream [[a, b], [c]])
  , ("a-b-c", stream [[a], [b], [c]])
  , ("abx", stream [[a, b, x]])
  , ("a-b-x", stream [[a], [b], [x]])
  , ("abcd", stream [[a, b, c, d]])
  , ("a-interrupted", stream [[a]] (some .interrupted))
  , ("a-io-error", stream [[a]] (some .ioError))
  , ("ab-io-error", stream [[a, b]] (some .ioError))
  , ("a-b-io-error", stream [[a], [b]] (some .ioError))
  ]

private def cases : List (String × Stream × Stream) :=
  inputs.flatMap fun (name, left) => inputs.map fun (other, right) =>
    (s!"{name}-versus-{other}", left, right)

private def sideName : Side → String
  | .left => "left"
  | .right => "right"

private def faultName : Fault → String
  | .interrupted => "interrupted"
  | .ioError => "io_error"

private def chunks : Stream → List (List UInt8)
  | .terminal _ => []
  | .chunk first rest next => (first :: rest) :: chunks next

private def streamJson (input : Stream) : Lean.Json :=
  Lean.Json.mkObj [
    ("chunks", Lean.toJson ((chunks input).map (List.map UInt8.toNat))),
    ("fault", Lean.toJson (input.fault.map faultName))]

private def eventJson : Event → Lean.Json
  | .fill side => Lean.Json.mkObj [("kind", .str "fill"), ("side", .str (sideName side))]
  | .consume side count => Lean.Json.mkObj [
      ("kind", .str "consume"), ("side", .str (sideName side)), ("count", Lean.toJson count)]

private def outcomeJson (outcome : Outcome) : Lean.Json :=
  let field := fun kind => ("kind", Lean.Json.str kind)
  Lean.Json.mkObj (match outcome with
    | .equal => [field "equal"]
    | .different => [field "different"]
    | .exhausted => [field "exhausted"]
    | .failed side fault => [field "failed", ("side", .str (sideName side)),
        ("fault", .str (faultName fault))])

def corpus : String :=
  String.join (cases.map fun (id, left, right) =>
    let expected := run left right
    (Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("mode", .str "internal-fixture"),
      ("id", .str id),
      ("left", streamJson left),
      ("right", streamJson right),
      ("events", .arr (expected.events.toArray.map eventJson)),
      ("outcome", outcomeJson expected.outcome)
    ]).compress ++ "\n")

def sensitivity : IO UInt32 := do
  let variants : List (String × (Stream → Stream → Step)) :=
    [ ("compare_whole_buffers", fun left right =>
        match left, right with
        | .chunk a as' _, .chunk b bs' _ =>
          if a :: as' = b :: bs' then step left right else ⟨pairFills, .stop .different⟩
        | _, _ => step left right)
    , ("discard_longer_buffer_remainder", fun left right =>
        let current := step left right
        match current.decision, left, right with
        | .advance _ _, .chunk _ _ leftTail, .chunk _ _ rightTail =>
          { current with decision := .advance leftTail rightTail }
        | _, _, _ => current)
    , ("either_eof_means_equal", fun left right =>
        match left, right with
        | .terminal none, .chunk .. | .chunk .., .terminal none => ⟨pairFills, .stop .equal⟩
        | _, _ => step left right)
    , ("left_eof_skips_right_fill", fun left right =>
        match left with
        | .terminal none => ⟨[.fill .left], .stop .equal⟩
        | _ => step left right)
    , ("right_error_overrides_left_error", fun left right =>
        match left, right with
        | .terminal (some _), .terminal (some fault) =>
          ⟨[.fill .right], .stop (.failed .right fault)⟩
        | _, _ => step left right)
    , ("advance_past_mismatch", fun left right =>
        match left, right with
        | .chunk a as' leftTail, .chunk b bs' rightTail =>
          let leftBytes := a :: as'
          let rightBytes := b :: bs'
          let count := min leftBytes.length rightBytes.length
          ⟨pairFills ++ [.consume .left count, .consume .right count],
            .advance (dropBuffer leftBytes leftTail count) (dropBuffer rightBytes rightTail count)⟩
        | _, _ => step left right)
    ]
  for (name, variant) in variants do
    let altered := fun left right =>
      runWith variant (left.bytes.length + right.bytes.length + 1) left right
    let witness := (cases.find? (fun (_, left, right) =>
      (altered left right).outcome != (run left right).outcome)).orElse fun () =>
      cases.find? (fun (_, left, right) => altered left right != run left right)
    if let some (id, _, _) := witness then
      IO.println s!"buffered comparison sensitivity: {name}: {id}"
    else
      IO.eprintln s!"buffered comparison missed variant: {name}"
      return 1
  IO.println s!"buffered_comparison_cases={cases.length}"
  return 0

end HoiminOracle.BufferedComparison
