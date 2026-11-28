import HoiminOracle.GitCaptureModel
import Lean.Data.Json

namespace HoiminOracle.GitCapture

private inductive InputStep
  | block (byte : UInt8) (count : Nat)
  | ioError

private structure Fixture where
  id : String
  stream : Stream
  limit : Nat
  input : List InputStep

private def step : InputStep → Step
  | .block byte count => .bytes (List.replicate count byte)
  | .ioError => .ioError

private def streamName : Stream → String
  | .stdout => "stdout"
  | .stderr => "stderr"

private def alphabet : List InputStep := [.block 97 1, .block 255 2, .block 0 3, .ioError]

private def schedules : Nat → List (List InputStep)
  | 0 => [[]]
  | depth + 1 => alphabet.flatMap fun event => (schedules depth).map (event :: ·)

private def matrix : List Fixture :=
  let traces := (List.range 4).flatMap schedules
  ([.stdout, .stderr] : List Stream).flatMap fun stream => (List.range 5).flatMap fun limit =>
    traces.zipIdx |>.map fun (events, index) => ⟨s!"{streamName stream}-{limit}-{index}", stream, limit, events⟩

private def fixtures : List Fixture := matrix ++ [
  ⟨"stdout-exact-buffer-size", .stdout, 8192, [.block 7 8192]⟩,
  ⟨"stdout-exact-after-allocation-growth", .stdout, 24577,
    [.block 97 8192, .block 0 8192, .block 255 8192, .block 98 1]⟩,
  ⟨"stdout-over-limit-in-later-read", .stdout, 16385,
    [.block 97 8192, .block 0 8192, .block 255 4, .ioError]⟩,
  ⟨"stdout-read-error-after-exact-limit", .stdout, 16384,
    [.block 97 8192, .block 98 8192, .ioError]⟩,
  ⟨"stderr-real-limit-exact", .stderr, 65536, List.replicate 8 (.block 255 8192)⟩,
  ⟨"stderr-real-limit-plus-one", .stderr, 65536,
    List.replicate 7 (.block 97 8192) ++ [.block 0 8192, .block 255 1]⟩,
  ⟨"stderr-io-after-allocation-growth", .stderr, 24577,
    List.replicate 4 (.block 97 8192) ++ [.block 0 8192, .ioError]⟩,
  ⟨"stderr-read-larger-than-buffer", .stderr, 10333, [.block 97 24000, .block 0 7]⟩]

private def inputJson : InputStep → Lean.Json
  | .block byte count => Lean.Json.mkObj [
    ("kind", .str "block"), ("byte", Lean.toJson byte.toNat), ("count", Lean.toJson count)]
  | .ioError => Lean.Json.mkObj [("kind", .str "io_error")]

private def runs (bytes : List UInt8) : List (UInt8 × Nat) :=
  (bytes.foldl (fun runs byte =>
    match runs with
    | (previous, count) :: rest =>
      if byte == previous then (previous, count + 1) :: rest else (byte, 1) :: runs
    | [] => [(byte, 1)]) []).reverse

private def resultJson (result : Capture) : Lean.Json := Lean.Json.mkObj [
  ("reads", Lean.toJson result.reads),
  ("outcome", match result.result with
    | .error .overflow => Lean.Json.mkObj [("kind", .str "overflow")]
    | .error .ioError => Lean.Json.mkObj [("kind", .str "io_error")]
    | .ok bytes => Lean.Json.mkObj [("kind", .str "ok"), ("runs", .arr (bytes |> runs |>.toArray.map
        fun (byte, count) => Lean.toJson (byte.toNat, count)))])]

private def expected (item : Fixture) : Capture :=
  capture item.stream item.limit (item.input.map step)

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
  ("stream", .str (streamName item.stream)), ("limit", Lean.toJson item.limit),
  ("input", .arr (item.input.toArray.map inputJson)), ("expected", resultJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def stopDraining (item : Fixture) : Capture :=
  let correct := expected item
  if item.stream == .stderr then
    let cutoff := capture .stdout item.limit (item.input.map step)
    { correct with reads := min correct.reads cutoff.reads }
  else correct

private def wrongRetention (item : Fixture) : Capture :=
  let correct := expected item
  if item.stream == .stderr then
    let bytes := item.input.flatMap fun event => match step event with
      | .bytes chunk => chunk
      | .ioError => []
    { correct with result := correct.result.map fun found =>
      if item.limit < bytes.length then (bytes.reverse.take item.limit).reverse ++ stderrMarker else found }
  else correct

def sensitivity : IO UInt32 := do
  for (name, broken) in [
    ("capture_clip_stdout", fun item : Fixture =>
      if item.stream == .stdout then capture .stderr item.limit (item.input.map step) else expected item),
    ("capture_reject_exact_stdout_limit", fun item =>
      if item.stream == .stdout then expected { item with limit := item.limit - 1 } else expected item),
    ("capture_stop_draining_stderr", stopDraining),
    ("capture_keep_stderr_suffix", wrongRetention),
    ("capture_drop_stderr_truncation_marker", fun item =>
      let correct := expected item
      if item.stream == .stderr then { correct with result := correct.result.map (·.take item.limit) }
      else correct)] do
    match fixtures.find? (fun item =>
      (resultJson (broken item)).compress != (resultJson (expected item)).compress) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"Git capture corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitCapture
