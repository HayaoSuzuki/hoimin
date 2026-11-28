import HoiminOracle.GitResolutionModel
import Lean.Data.Json

namespace HoiminOracle.GitResolution

private def decorate (paths : List String) (args : List String) : List String :=
  if paths.isEmpty then args else
    "--literal-pathspecs" ::
      (args.map (fun arg => if arg == "--find-renames" then "--no-renames" else arg) ++ paths)

private def diffOptions : List String :=
  ["--no-color", "--no-ext-diff", "--no-textconv", "--src-prefix=a/", "--dst-prefix=b/", "--find-renames",
    "--inter-hunk-context=0", "--diff-algorithm=myers", "--no-indent-heuristic", "-l0", "--relative"]

private def args : Request → List String
  | .worktree => ["rev-parse", "--is-inside-work-tree"]
  | .head => ["show-ref", "--verify", "--hash", "--", "HEAD"]
  | .symbolicHead => ["symbolic-ref", "HEAD"]
  | .headReference => ["show-ref", "--verify", "--quiet", "--", "HEAD"]
  | .revision name => ["rev-parse", "--verify", "--end-of-options", name ++ "^{commit}"]
  | .peelHead oid => ["rev-parse", "--verify", "--end-of-options", oid ++ "^{commit}"]
  | .mergeBase base head => ["merge-base", "--all", base, head]
  | .inventory base => ["diff", "--name-status", "-z"] ++ diffOptions ++ [base, "--"]
  | .patch base context paths =>
    decorate paths (["diff", s!"--unified={context}"] ++ diffOptions ++ [base, "--"])
  | .numstat base paths => decorate paths
    (["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--find-renames", "-l0", "--relative"] ++
      [base, "--"])
  | .indexed => ["ls-files", "-z"]
  | .untracked => ["ls-files", "--others", "--exclude-standard", "-z"]

private def scopes : List (String × Option (List String)) :=
  [("all", none), ("empty", some []), ("selected", some [selectedPath, untrackedPath]),
    ("only-untracked", some [untrackedPath])]

private def matrix : List (String × Input) :=
  let inputs : List (String × Input) := [
    ("head", { context := 2, changedPatch := true }),
    ("indexed", { headStatus := some 128 }),
    ("base", { revision := some "topic", changedPatch := true, context := 7 })]
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
  ("inventory-command-fails", { scope := some [selectedPath], failAt := some (.inventory headCommitOid) }),
  ("patch-command-fails", { failAt := some (.patch headCommitOid 0 []) }),
  ("scoped-patch-command-fails", {
    scope := some [selectedPath],
    failAt := some (.patch headCommitOid 0 [selectedPath]) }),
  ("numstat-command-fails", { failAt := some (.numstat headCommitOid []), changedPatch := true }),
  ("invalid-patch-hunk", { corruptPatch := true }),
  ("indexed-command-fails", { headStatus := some 128, failAt := some .indexed }),
  ("untracked-command-fails-after-patch", { changedPatch := true, failAt := some .untracked }),
  ("untracked-command-fails-after-indexed", { headStatus := some 128, failAt := some .untracked }),
  ("head-execution-failure", { headStatus := some 2 }),
  ("corrupt-head-even-with-empty-scope", { headCommit := .exited 128, scope := some [] }),
  ("head-terminated-by-signal", { headStatus := none }),
  ("head-symbolic-read-error", { headStatus := some 128, symbolicStatus := some 128 }),
  ("head-symbolic-status-one", { headStatus := some 128, symbolicStatus := some 1 }),
  ("head-symbolic-terminated", { headStatus := some 128, symbolicStatus := none }),
  ("head-reference-present", { headStatus := some 128, referenceStatus := some 0 }),
  ("head-reference-lookup-error", { headStatus := some 128, referenceStatus := some 128 }),
  ("head-reference-terminated", { headStatus := some 128, referenceStatus := none }),
  ("exact-lookup-status-one", { headStatus := some 1 }),
  ("exact-head-invalid-utf8", { headBytes := [255] }),
  ("exact-head-invalid-id", { headBytes := utf8 "--help\n" }),
  ("exact-head-sha256", {
    headBytes := List.replicate 64 98 ++ [10], headCommit := .exited 0 (List.replicate 64 99 ++ [10]) }),
  ("explicit-base-with-unborn-head", { revision := some "topic", headStatus := some 128 }),
  ("merge-no-common-ancestor", { revision := some "topic", merge := .exited 1 }),
  ("merge-empty-success", { revision := some "topic", merge := .exited 0 }),
  ("merge-multiple-ancestors", {
    revision := some "topic", merge := .exited 0 (utf8 (baseOid ++ "\n" ++ mergeBaseOid ++ "\n")) }),
  ("merge-invalid-id", { revision := some "topic", merge := .exited 0 (utf8 "invalid\n") }),
  ("merge-command-failed", { revision := some "topic", merge := .exited 128 })]

private def failureText : Failure → String
  | .repository => "repository"
  | .command => "command"
  | .oidUtf8 => "oid_utf8"
  | .oidForm => "oid_form"
  | .inventory => "inventory"
  | .hunk => "hunk"
  | .missingHead => "missing_head"
  | .noMergeBase => "no_merge_base"
  | .multipleMergeBases => "multiple_merge_bases"

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
        | .error _ => { input with headStatus := some 128 }
        | .ok _ => input))),
    ("resolution_index_without_confirming_unborn_head", fun input =>
      (execute (requests { input with symbolicStatus := some 0, referenceStatus := some 1 })).filter
        (fun call => call.request != .symbolicHead && call.request != .headReference)),
    ("resolution_diff_against_unpeeled_head", fun input =>
      execute (requests { input with headCommit := .exited 0 input.headBytes })),
    ("resolution_diff_against_requested_base_without_merge", fun input =>
      execute (requests { input with merge := input.oid })),
    ("resolution_omit_untracked", fun input => (execute (requests input)).filter (·.request != .untracked))] do
    match fixtures.find? (fun (_, input) => (wrong input).map Exchange.request !=
        (execute (requests input)).map Exchange.request) with
    | some (id, _) => IO.println s!"family={name} detected=true case={id}"
    | none =>
      IO.eprintln s!"Git resolution corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.GitResolution
