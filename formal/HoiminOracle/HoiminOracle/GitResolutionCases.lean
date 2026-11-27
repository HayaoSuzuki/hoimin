import HoiminOracle.GitResolutionModel
import Lean.Data.Json

namespace HoiminOracle.GitResolution

private def comparisonArgs : Comparison → List String
  | .head => ["HEAD"]
  | .mergeBase oid => ["--merge-base", oid]

private def decorate (paths : List String) (args : List String) : List String :=
  if paths.isEmpty then args else
    "--literal-pathspecs" ::
      (args.map (fun arg => if arg == "--find-renames" then "--no-renames" else arg) ++ "--" :: paths)

private def diffOptions : List String :=
  ["--no-color", "--no-ext-diff", "--no-textconv", "--src-prefix=a/", "--dst-prefix=b/", "--find-renames",
    "--inter-hunk-context=0", "--diff-algorithm=myers", "--no-indent-heuristic", "-l0", "--relative"]

private def args : Request → List String
  | .worktree => ["rev-parse", "--is-inside-work-tree"]
  | .head => ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]
  | .revision name => ["rev-parse", "--verify", "--end-of-options", name ++ "^{commit}"]
  | .inventory base => ["diff", "--name-status", "-z"] ++ diffOptions ++ comparisonArgs base
  | .patch base context paths =>
    decorate paths (["diff", s!"--unified={context}"] ++ diffOptions ++ comparisonArgs base)
  | .numstat base paths => decorate paths
    (["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--find-renames", "-l0", "--relative"] ++
      comparisonArgs base)
  | .indexed => ["ls-files", "-z"]
  | .untracked => ["ls-files", "--others", "--exclude-standard", "-z"]

private def scopes : List (String × Option (List String)) :=
  [("all", none), ("empty", some []), ("selected", some [selectedPath, untrackedPath]),
    ("only-untracked", some [untrackedPath])]

private def matrix : List (String × Input) :=
  let inputs : List (String × Input) := [
    ("head", { context := 2, changedPatch := true }),
    ("indexed", { headStatus := some 1 }),
    ("base", { revision := some "topic", headStatus := some 128, changedPatch := true, context := 7 })]
  inputs.flatMap fun (name, input) => scopes.map fun (suffix, scope) =>
    (name ++ "-" ++ suffix, { input with scope })

private def fixtures : List (String × Input) := matrix ++ [
  ("worktree-false", { worktree := .exited 0 (utf8 "false\n"), revision := some "topic" }),
  ("worktree-status-before-valid-reply", { worktree := .exited 2 (utf8 "true\n") }),
  ("worktree-ascii-trim", { worktree := .exited 0 (utf8 " \ttrue\r\n") }),
  ("worktree-unexpected-encoding", { worktree := .exited 0 [255] }),
  ("base-sha256-and-ascii-trim", {
    revision := some "HEAD~2",
    oid := .exited 0 ([32,9] ++ List.replicate 64 70 ++ [13,10]),
    scope := some [selectedPath] }),
  ("base-option-shaped-revision", { revision := some "--help" }),
  ("base-status-before-valid-oid", { revision := some "broken", oid := .exited 23 (List.replicate 40 65) }),
  ("base-invalid-oid-length", { revision := some "broken", oid := .exited 0 (List.replicate 39 65) }),
  ("base-invalid-oid-character", { revision := some "broken", oid := .exited 0 (122 :: List.replicate 39 65) }),
  ("base-invalid-utf8", { revision := some "broken", oid := .exited 0 (255 :: List.replicate 39 65) }),
  ("base-invalid-embedded-newline", {
    revision := some "broken",
    oid := .exited 0 (List.replicate 20 65 ++ [10] ++ List.replicate 19 65) }),
  ("base-invalid-even-with-empty-scope", { revision := some "broken", oid := .exited 0, scope := some [] }),
  ("scoped-empty-inventory", { scope := some [selectedPath], inventory := .empty }),
  ("scoped-related-rename-falls-back", {
    scope := some [selectedPath], inventory := .renamed, changedPatch := true, context := 12 }),
  ("scoped-unrelated-rename-skips-diff", { scope := some [untrackedPath], inventory := .renamed }),
  ("malformed-inventory-even-with-empty-scope", { scope := some [], inventory := .malformed }),
  ("inventory-command-fails", { scope := some [selectedPath], failAt := some (.inventory .head) }),
  ("patch-command-fails", { failAt := some (.patch .head 0 []) }),
  ("scoped-patch-command-fails", {
    scope := some [selectedPath],
    failAt := some (.patch .head 0 [selectedPath]) }),
  ("numstat-command-fails", { failAt := some (.numstat .head []), changedPatch := true }),
  ("invalid-patch-hunk", { corruptPatch := true }),
  ("indexed-command-fails", { headStatus := some 1, failAt := some .indexed }),
  ("untracked-command-fails-after-patch", { changedPatch := true, failAt := some .untracked }),
  ("untracked-command-fails-after-indexed", { headStatus := some 1, failAt := some .untracked }),
  ("head-execution-failure", { headStatus := some 2 }),
  ("corrupt-head-even-with-empty-scope", { headStatus := some 128, scope := some [] }),
  ("head-terminated-by-signal", { headStatus := none })]

private def failureText : Failure → String
  | .repository => "repository"
  | .command => "command"
  | .oidUtf8 => "oid_utf8"
  | .oidForm => "oid_form"
  | .inventory => "inventory"
  | .hunk => "hunk"

private def resultJson (input : Input) (calls : List Exchange) : Lean.Json :=
  match calls.getLast?.bind (·.failure) with
  | some failure => Lean.Json.mkObj [("kind", .str (failureText failure))]
  | none => Lean.Json.mkObj [("kind", .str "ok"), ("changed", Lean.Json.mkObj
      ((changed input calls).map fun (path, span) => (path, .arr #[Lean.Json.mkObj [
        ("start", Lean.toJson span.start), ("end", Lean.toJson span.finish)]])))]

private def fixtureJson (id : String) (input : Input) : Lean.Json :=
  let calls := execute (requests input)
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
    ("revision", Lean.toJson input.revision), ("context", Lean.toJson input.context),
    ("scope", Lean.toJson input.scope), ("responses", .arr (calls.toArray.map fun exchange =>
      Lean.Json.mkObj [("status", Lean.toJson exchange.reply.status),
        ("bytes", Lean.toJson (exchange.reply.bytes.map UInt8.toNat))])),
    ("calls", Lean.toJson (calls.map (args ∘ Exchange.request))),
    ("sources", Lean.Json.mkObj [(selectedPath, Lean.toJson (selectedContents.map UInt8.toNat)),
      (untrackedPath, Lean.toJson (untrackedContents.map UInt8.toNat))]),
    ("expected", resultJson input calls)]

def corpus : String := String.join (fixtures.map fun (id, input) => (fixtureJson id input).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, wrong) in [
    ("resolution_use_head_despite_base", fun input : Input => execute (requests { input with revision := none })),
    ("resolution_skip_inventory", fun input => execute (requests { input with scope := none })),
    ("resolution_continue_after_error", requests),
    ("resolution_use_patch_on_missing_head", fun input =>
      execute (requests { input with headStatus := some 0 })),
    ("resolution_treat_fatal_head_as_missing", fun input =>
      execute (requests (match headRoute input.headStatus with
        | .error _ => { input with headStatus := some 1 }
        | .ok _ => input))),
    ("resolution_omit_untracked", fun input => (execute (requests input)).filter (·.request != .untracked))] do
    match fixtures.find? (fun (_, input) => (wrong input).map Exchange.request !=
        (execute (requests input)).map Exchange.request) with
    | some (id, _) => IO.println s!"family={name} detected=true case={id}"
    | none =>
      IO.eprintln s!"Git resolution corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitResolution
