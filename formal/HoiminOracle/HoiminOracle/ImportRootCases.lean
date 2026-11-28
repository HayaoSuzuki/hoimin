import HoiminOracle.ImportRootModel
import Lean.Data.Json

namespace HoiminOracle.ImportRoot

private structure Fixture where
  id : String
  paths : List String

private def alphabet : List (String × String) :=
  [("src", "src"), ("vendor", "vendor"), ("here", "."), ("parent", "..")]

private def componentCases : List Fixture :=
  alphabet.flatMap fun (firstName, first) =>
    alphabet.flatMap fun (secondName, second) =>
      alphabet.map fun (thirdName, third) =>
        ⟨s!"components-{firstName}-{secondName}-{thirdName}",
          [String.intercalate "/" [first, second, third]]⟩

private def fixtures : List Fixture := componentCases ++ [
  ⟨"absent", []⟩,
  ⟨"dot", ["."]⟩,
  ⟨"canonical-order", ["vendor", ".", "src", "src/package"]⟩,
  ⟨"aliases-stable-first", ["./vendor", "src//package/", "vendor/./", "src/package", "src/../."]⟩,
  ⟨"duplicate-root-and-nested", [".", "./", "pkg", "pkg", "pkg/./sub", "pkg/sub"]⟩,
  ⟨"case-distinct", ["Vendor", "vendor"]⟩,
  ⟨"literal-spaces", [" ", "pkg /sub"]⟩,
  ⟨"empty-is-not-dot", [""]⟩,
  ⟨"empty-argument-before-config-validation", ["../outside", ""]⟩,
  ⟨"absolute", ["/absolute"]⟩,
  ⟨"first-invalid", ["./vendor", "../first", "/second"]⟩,
  ⟨"absolute-before-escape", ["/first", "../second"]⟩,
  ⟨"escape-after-cancellation", ["pkg/../../outside"]⟩,
  ⟨"rooted-separator", ["\\outside"]⟩,
  ⟨"backslash-components", ["pkg\\sub", "pkg/sub"]⟩,
  ⟨"backslash-traversal", ["pkg\\..\\..\\outside"]⟩,
  ⟨"drive-relative", ["C:relative"]⟩,
  ⟨"drive-rooted", ["c:/outside"]⟩,
  ⟨"unc", ["\\\\host\\share\\pkg"]⟩,
  ⟨"device-prefix", ["\\\\?\\C:\\pkg"]⟩,
  ⟨"drive-prefix-after-normalization", ["pkg/../C:relative"]⟩]

private def strings (paths : List String) : Lean.Json := .arr (paths.toArray.map Lean.Json.str)

private def outcomeJson : Except Issue (List String) → Lean.Json
  | .ok paths => Lean.Json.mkObj [("kind", .str "accepted"), ("paths", strings paths)]
  | .error .missingCliValue => Lean.Json.mkObj [("kind", .str "missing_cli_value")]
  | .error (.invalid path) => Lean.Json.mkObj [("kind", .str "invalid"), ("path", .str path)]
  | .error .nonNormalized => Lean.Json.mkObj [("kind", .str "non_normalized")]

private def platformJson (platform : Platform) (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("cli", outcomeJson (fromCli platform item.paths)),
  ("persisted", outcomeJson (checked platform item.paths))]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("input", strings item.paths), ("posix", platformJson .posix item), ("windows", platformJson .windows item)]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def sortRoots (platform : Platform) (paths : List String) : Except Issue (List String) :=
  (fromCli platform paths).map (List.mergeSort · (· ≤ ·))

private def dropLeadingParent (platform : Platform) (paths : List String) : Except Issue (List String) :=
  fromCli platform (paths.map fun path => if path.startsWith "../" then (path.drop 3).toString else path)

private def broken (candidate correct : List String → Except Issue (List String)) : Option Fixture :=
  fixtures.find? fun item =>
    match candidate item.paths, correct item.paths with
    | .ok a, .ok b => a != b
    | .error a, .error b => a != b
    | _, _ => true

def sensitivity : IO UInt32 := do
  for (name, candidate, correct) in [
    ("sort_import_roots", sortRoots .posix, fromCli .posix),
    ("ignore_leading_parent", dropLeadingParent .posix, fromCli .posix),
    ("renormalize_persisted_roots", fromCli .posix, checked .posix),
    ("use_posix_separators_on_windows", fromCli .posix, fromCli .windows)] do
    match broken candidate correct with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"import-root corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.ImportRoot
