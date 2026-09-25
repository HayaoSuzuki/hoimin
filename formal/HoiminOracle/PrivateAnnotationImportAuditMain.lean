import HoiminOracle.PrivateAnnotationImportModel
import Lean.Data.Json

open HoiminOracle.PrivateAnnotationImport Lean

namespace PrivateAnnotationImportAudit

def sourceFor (cls alias endpoint : String) (overwrite : Bool) : String :=
  let sourceEndpoint := endpoint == "source"
  let imported := if sourceEndpoint then "Sequence" else "Iterable"
  let outer := if sourceEndpoint then "Iterable" else "Sequence"
  let original := if sourceEndpoint then alias ++ "[int]" else "Sequence[int]"
  s!"from typing import {outer}\nclass {cls}:\n    from typing import {imported} as {alias}\n" ++
    (if overwrite then s!"    {mangle cls alias} = tuple\n" else "") ++
    s!"    value: {original} # target\n    probe: {alias}\n"

def cases : List Json := ["C", "_C", "___"].flatMap fun cls =>
  ["Alias", "__Alias", "__Alias__"].flatMap fun alias =>
    ["source", "destination"].flatMap fun endpoint =>
      [false, true].map fun overwrite => Json.mkObj [
        ("schema", toJson (1 : Nat)), ("mode", toJson "strict"),
        ("id", toJson s!"{cls}-{alias}-{endpoint}-{overwrite}"),
        ("class", toJson cls), ("alias", toJson alias),
        ("endpoint", toJson endpoint), ("overwrite", toJson overwrite),
        ("source", toJson (sourceFor cls alias endpoint overwrite)),
        ("original", toJson (if endpoint == "source" then alias ++ "[int]" else "Sequence[int]")),
        ("replacement", toJson (if endpoint == "source" then "Iterable[int]" else alias ++ "[int]")),
        ("expected_key", toJson (mangle cls alias)),
        ("expected_binding_is_typing", toJson (allowed cls alias overwrite)),
        ("candidate_allowed", toJson (allowed cls alias overwrite)),
        ("candidate_count", toJson (if eligible cls alias overwrite then (1 : Nat) else 0)),
        ("run_check", toJson (cls == "C" && alias == "__Alias" && overwrite)),
        ("broken_raw_key_allowed", toJson (brokenAllowed cls alias overwrite))]

def corpus : String := String.join (cases.map (fun c => c.compress ++ "\n"))

def main (args : List String) : IO Unit := do
  unless sensitivity do throw (IO.userError "private alias sensitivity failed")
  unless mangle "_C" "__Alias" == "_C__Alias" &&
      mangle "___" "__Alias" == "__Alias" &&
      mangle "C" "__Alias__" == "__Alias__" do
    throw (IO.userError "private alias boundary checks failed")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale private annotation import corpus")
  | ["--sensitivity"] => IO.println "raw_key=true leading_class_underscores=true trailing_dunder=true"
  | ["--stats"] => IO.println s!"fixed_cases={cases.length} classes=3 aliases=3 endpoints=2 overwrite=2 max_writes=2 transitions=0"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end PrivateAnnotationImportAudit

def main := PrivateAnnotationImportAudit.main
