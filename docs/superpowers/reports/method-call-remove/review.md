# Method call removal review record

Design and plan each contain five reviews. Implementation and test reviews below
were performed against the worktree based on main 3596c98.

## Implementation self-reviews

1. Registry/default boundary: all grows 69 -> 70; default 50 and all_legacy 43 are untouched. Behavioral ranking uses the existing score and leaves old identities/ranks unchanged.
2. Evaluation boundary: replacement contains receiver source exactly once, with parentheses. Attribute lookup and invocation effects are intentionally absent; module attributes remain eligible and documented.
3. Eligibility boundary: zero positional and keyword arguments uses the shared exact-arguments predicate; the recursive conversion guard rejects nested binding/suspension/generators and checks cancellation.
4. Context boundary: shared runtime role excludes full assignment-target subtrees; annotation ranges, pattern state and explicit-alias exclusions remain in force. Alias indexing is built when this operator alone is selected.
5. Source/integration boundary: candidates reuse encoded byte spans, source/candidate limits, profile filtering, IDs and saved-plan delivery. Nested eligible calls remain separate candidates. No production dependencies or default changes.

## Test self-reviews

1. RED/GREEN: initial public operator test failed with unknown operator before registry/collector edits, then passed. Adjacent conversion/body-erase baseline passed first.
2. Syntax/effects: CPython 3.14 covers binary/conditional receivers, multiline parentheses, chained calls, CRLF/CR/BOM/Latin-1 and a receiver/lookup/call trace. Original outer bytes remain unchanged.
3. Exclusions/bounds: public negative fixtures cover arguments/expansion, nested binding/suspension/generators and type/target contexts; analyzer test covers default exclusion, opt-in/exclude, line/symbol filters, maximum prefix and cancellation.
4. Oracle independence: Lean generates all expected pairs; the public adapter compares every strict case and compiles every mutant. Parsing rejects malformed JSON, unknown fields/schema/mode, duplicate IDs and missing rows. Freshness and broken controls are CI gates.
5. Behavioral evidence: same saved candidate survives a weak type-only test and is killed by a value assertion; preview/verify preserve caller source. Real-source trials record selected outcomes without claiming upstream-suite performance. Python CI contract test caught generator ordering mismatch; registry order was corrected to match Lake/CI.

## Validation and final review

Independent fresh-context review found no critical, important or minor findings.
Full tests caught two stale inventories: README count (69 -> 70) and the initial
cross-producer registry. The latter now points to the dedicated 23-case corpus;
its preexisting deferred/covered distinction is preserved. Both failures were
observed before their fixture updates.

Final checks (2026-10-07): `cargo test --offline --workspace` passed 2696 tests,
with 22 intentionally ignored; the seven new public tests passed again after
the Clippy single-character-pattern correction. `cargo fmt --all -- --check` and
`cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` passed.
`pytest tests/test_ci_workflow.py -q`: 142 passed. OKF checks parsed 25 concept YAML
headers, checked four reserved-file structures, and verified the new source link/hash.
Guarded Lean freshness and all nine sensitivity checks passed again before commit.
Adjacent pre-change baseline: 4 conversion and 6 body-erasure tests passed.

PR worktree is retained for review. Cargo and Lean build outputs are removed before
the next feature starts; no existing root-workspace files are cleaned.
