import HoiminOracle.SelectorSyntaxModel
import Lean.Data.Json

namespace HoiminOracle.SelectorSyntax

private structure Fixture where
  id : String
  source : Bool
  args : List Argument

private def paths : List (String × String) := [
  ("relative", "src/calc.py"), ("colon", "src/a:b.py"), ("drive", "C:/repo/calc.py"),
  ("windows-separators", "C:\\repo\\calc.py"), ("reverse-slash", "src\\calc.py"),
  ("empty", ""), ("literal", " ./src//cálc.py "), ("absolute", "/root/src/calc.py"),
  ("colon-only", ":")]

private def ranges : List (String × String) := [
  ("single", "1"), ("single-max", "4294967295"), ("full", "1-4294967295"),
  ("max-plus-one", "4294967296"), ("overflow-start", "4294967296-4294967295"),
  ("overflow-end", "1-4294967296"), ("reversed", "3-2"), ("equal", "2-2"),
  ("zero", "0"), ("zero-start", "0-1"), ("zero-end", "1-0"),
  ("negative", "-1"), ("empty", ""), ("absent-end", "1-"),
  ("extra-dash", "1-2-3"), ("plus", "+3"), ("plus-range", "+3-+5"),
  ("leading-zeroes", "0002-0003"), ("space", "1 -2"), ("fullwidth", "１"),
  ("underscore", "1_000"), ("fraction", "1.5")]

private def lineCases : List Fixture :=
  paths.flatMap fun (pathName, path) =>
    ranges.map fun (rangeName, range) =>
      ⟨s!"line-{pathName}-{rangeName}", false, [.line s!"{path}:{range}"]⟩

private def symbolCases : List Fixture :=
  ([("plain", "calc"), ("dotted", "pkg.calc"), ("empty", ""), ("literal", " pkg ")]) |>.flatMap
    fun (moduleName, module) =>
      ([("plain", "total"), ("nested", "Widget.total"), ("colon", "Outer:Inner.call"),
          ("colon-only", ":"), ("empty", ""), ("literal", " mé thod ")]) |>.flatMap
        fun (qualnameName, qualname) =>
          [false, true].map fun source =>
            ⟨s!"symbol-{moduleName}-{qualnameName}-{source}", source, [.symbol s!"{module}:{qualname}"]⟩

private def fixtures : List Fixture := lineCases ++ symbolCases ++ [
  ⟨"source-only", true, []⟩,
  ⟨"no-selector", false, []⟩,
  ⟨"line-without-colon", false, [.line "src/calc.py"]⟩,
  ⟨"symbol-without-colon", true, [.symbol "pkg.calc"]⟩,
  ⟨"empty-line-argument", false, [.line ""]⟩,
  ⟨"empty-symbol-argument", true, [.symbol ""]⟩,
  ⟨"lines-validated-before-symbols", true, [.symbol "bad-symbol", .line "bad-line"]⟩,
  ⟨"second-line-invalid-before-symbol", false, [.line "src/calc.py:2",
    .symbol "bad-symbol", .line "src/calc.py:3-2"]⟩,
  ⟨"bad-symbol-before-source-requirement", false, [.line "src/calc.py:2", .symbol ":"]⟩,
  ⟨"symbol-source-required-with-line", false, [.line "src/calc.py:2", .symbol "calc:total"]⟩,
  ⟨"first-symbol-error", true, [.symbol "calc:total", .symbol "bad-first", .symbol "bad-second"]⟩,
  ⟨"ordered-interleaving", true, [.line "src/second.py:4-8", .symbol "z:Widget.total",
    .line "src/first.py:2", .symbol "a:total", .line "src/first.py:2"]⟩]

private def outcomeJson : Except Issue Selection → Lean.Json
  | .ok selection => Lean.Json.mkObj [
      ("kind", .str "accepted"),
      ("lines", .arr (selection.lines.toArray.map fun line => Lean.Json.mkObj [
        ("path", .str line.path), ("start", Lean.toJson line.start), ("end", Lean.toJson line.finish)])),
      ("symbols", .arr (selection.symbols.toArray.map fun symbol => Lean.Json.mkObj [
        ("module", .str symbol.module), ("qualname", .str symbol.qualname)]))]
  | .error (.line value) => Lean.Json.mkObj [("kind", .str "invalid_line"), ("value", .str value)]
  | .error (.symbol value) => Lean.Json.mkObj [("kind", .str "invalid_symbol"), ("value", .str value)]
  | .error .symbolNeedsSource => Lean.Json.mkObj [("kind", .str "symbol_needs_source")]
  | .error .missing => Lean.Json.mkObj [("kind", .str "missing")]

private def caseJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("source", Lean.toJson item.source),
  ("args", .arr (item.args.toArray.map fun arg => match arg with
    | .line value => .str s!"--line={value}"
    | .symbol value => .str s!"--symbol={value}")),
  ("posix", outcomeJson (resolve .posix item.source item.args)),
  ("windows", outcomeJson (resolve .windows item.source item.args))]

def corpus : String := String.join (fixtures.map fun item => (caseJson item).compress ++ "\n")

private def symbolsFirst (source : Bool) (arguments : List Argument) : Except Issue Selection := do
  let _ ← (symbolArgs arguments).mapM fun value => require (.symbol value) (parseSymbol value)
  resolve .posix source arguments

private def differs (first second : Except Issue Selection) : Bool :=
  match first, second with
  | .ok a, .ok b => a != b
  | .error a, .error b => a != b
  | _, _ => true

def sensitivity : IO UInt32 := do
  for (name, candidate, correct) in [
    ("split_line_at_first_colon", resolveWith (parseLineWith (splitFirst ":") .posix) parseSymbol,
      resolve .posix),
    ("split_symbol_at_last_colon", resolveWith (parseLine .posix) (parseSymbolWith (splitLast ":")),
      resolve .posix),
    ("validate_symbols_before_lines", symbolsFirst, resolve .posix),
    ("reject_native_windows_separators", resolve .posix, resolve .windows)] do
    match fixtures.find? (fun item => differs (candidate item.source item.args) (correct item.source item.args)) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"selector syntax corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.SelectorSyntax
