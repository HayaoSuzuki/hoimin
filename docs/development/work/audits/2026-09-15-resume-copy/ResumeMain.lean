import ResumeModel
import Lean.Data.Json
open CopyResume Lean

structure Fixture where
  id : String
  mechanism : String
  before : Config
  after : Config

def fixtures : List Fixture := [
  ⟨"exclude_added", "exclude", ⟨true, 1024⟩, ⟨false, 1024⟩⟩,
  ⟨"exclude_removed", "exclude", ⟨false, 1024⟩, ⟨true, 1024⟩⟩,
  ⟨"exclude_unchanged_present", "exclude", ⟨true, 1024⟩, ⟨true, 1024⟩⟩,
  ⟨"exclude_unchanged_absent", "exclude", ⟨false, 1024⟩, ⟨false, 1024⟩⟩,
  ⟨"include_added", "include", ⟨false, 1024⟩, ⟨true, 1024⟩⟩,
  ⟨"include_removed", "include", ⟨true, 1024⟩, ⟨false, 1024⟩⟩,
  ⟨"output_limit_changed", "exclude", ⟨true, 1024⟩, ⟨true, 2048⟩⟩]

def status (killed : Bool) := if killed then "killed" else "survived"
def corpus := String.join (fixtures.map fun f =>
  let expected := resume (save f.before) f.after
  (Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson f.id), ("mode", toJson "strict"),
    ("mechanism", toJson f.mechanism), ("before_copied", toJson f.before.copied),
    ("after_copied", toJson f.after.copied), ("before_output", toJson f.before.outputCap),
    ("after_output", toJson f.after.outputCap),
    ("expected_initial", toJson (status f.before.copied)),
    ("expected_fresh", toJson (status f.after.copied)),
    ("expected_resumed", toJson (status expected.killed)),
    ("expected_reuse", toJson expected.reused)]).compress ++ "\n")

def main (args : List String) : IO Unit := do
  for caps in [[1024], [1024, 2048]] do
    let started ← IO.monoMsNow
    let mut checked := 0
    let mut bad := 0
    let mut overInvalidated := 0
    for oldCopied in [false, true] do
      for newCopied in [false, true] do
        for oldCap in caps do
          for newCap in caps do
            checked := checked + 1
            let before : Config := ⟨oldCopied, oldCap⟩
            let after : Config := ⟨newCopied, newCap⟩
            if resume (save before) after != brokenResume (save before) after then bad := bad + 1
            if compatible before after != brokenOutputKey (save before) after then overInvalidated := overInvalidated + 1
    IO.println s!"output_caps={caps.length} configurations={2 * caps.length} cases={checked} resume_transitions={checked} omitted_copy_mismatches={bad} output_key_mismatches={overInvalidated} elapsed_ms={(← IO.monoMsNow) - started}"
  let before : Config := ⟨true, 1024⟩
  let changed : Config := ⟨false, 1024⟩
  unless resume (save before) changed != brokenResume (save before) changed do
    throw (IO.userError "changed copy policy was not detected")
  unless compatible before ⟨true, 2048⟩ != brokenOutputKey (save before) ⟨true, 2048⟩ do
    throw (IO.userError "operational-setting over-invalidation was not detected")
  unless (resume (save before) before).reused do
    throw (IO.userError "reject-all reuse was not detected")
  IO.println "first witness: save copied=true/killed; resume copied=false; expected fresh survived/reused=false; broken killed/reused=true"
  IO.println "sensitivity: omitted-copy, output-key, reject-all detected"
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | _ => throw (IO.userError "usage: --output PATH | --check PATH")
