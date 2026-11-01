# Issue 632: changed-context formal correspondence

The committed 44-case corpus agrees with public `hoimin plan` for Git modifications, two separated hunks, deletions at the beginning/middle/end or across the complete file, beginning/end insertions, explicit-line intersection, unchanged files and untracked files. Every fixture runs in its own real Git repository. Context values are 0, 1, 2 and the supported maximum 1073741823; fixtures have at most eight current lines and two hunks.

## Model boundary

`ChangedContextModel.lean` defines current-file hunk coverage using natural-number intervals. A zero-length hunk is a gap after its start coordinate. Coverage is bounded by the current file length and intersected with explicit selection. `ChangedContextProofs.lean` proves coverage bounds, monotonicity in context, empty zero-context deletions and explicit-selection containment. These statements concern the model; they do not prove the Rust implementation or Git correct.

`ChangedContextCases.lean` owns fixture inputs and computes expected line sets. The Rust adapter translates generated before/after strings into real Git changes and invokes `run_with_io` with the public plan command. It compares candidate lines, paths, operator identity, original token and truncation status. It does not calculate expected intervals. Git/process/CLI/JSON failures fail the test rather than being counted as a semantic match. Python parsing beyond these simple arithmetic fixtures, Git rename heuristics, binary detection and arbitrary filesystem behavior are outside this model. Existing target tests and the issue regression tests cover additional composition behavior.

## Review record

Design and implementation-plan reviews are recorded in the issue design and plan committed before implementation.

Implementation self-review 1 checked one-based bounds against both ordinary ranges and deletion gaps. The model uses `start + 1 - context` for the left deletion boundary, avoiding a one-line shift when context is greater than one. It enumerates current-file lines rather than the context magnitude.

Implementation self-review 2 traced every expected observation back to `selected`, every adapter fixture back to serialized Lean inputs, and all new modules through the library imports, executable declaration and CI lists. Added the initially omitted library imports before validation. JSON construction uses Lean's serializer rather than hand-escaped source strings.

Implementation self-review 3 checked portability and the final integration diff. The implementation worker found that older Git doubles context in C `long`; inspection of [Git 2.43](https://github.com/git/git/blob/v2.43.0/xdiff/xemit.c#L55) confirmed that expression. Changed the maximum fixture to 1073741823 and regenerated the corpus from Lean after the public bound changed. No generated expected values were edited by hand.

Test self-review 1 mapped zero/default, positive expansion, each deletion edge, overlap, empty selection and explicit intersection to corpus rows. Added positive untracked and negative unchanged controls so selecting all files cannot pass.

Test self-review 2 checked failure sensitivity: five deliberately wrong alternatives ignore context, drop deletions, treat a deletion gap as a changed line, union explicit selectors, or omit file bounds. All five disagree with the model. The Rust corpus check rejects duplicate rows and validates sorted, positive, in-file expectations. This is model sensitivity, not a claim that every possible Rust mutation is detected.

Test self-review 3 checked real execution and persistence boundaries with the implementation worker. The baseline CLI rejected `--changed-context` with exit 2 on the generated deletion-middle-1 fixture; its expected lines are 2 and 3. The implemented public adapter passes all 44 rows. Separate issue tests exercise multiline operands, symbol restriction, diff-base and plan-to-verify configuration retention. A fresh independent review found no blocking issue.

## Verification

Run from `formal/HoiminOracle`, wrapping each command separately with:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 \
  --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/context-check.json -- COMMAND
```

Commands:

```sh
lake build +HoiminOracle.ChangedContextModel:o
lake build +HoiminOracle.ChangedContextProofs:o
lake build +HoiminOracle.ChangedContextCases:o
lake build +ChangedContextAuditMain:o
lake exe generate_changed_context -- --check corpus/changed-context.jsonl
lake exe generate_changed_context -- --sensitivity
lake exe generate_changed_context -- --stats
```

Generation uses `lake env lean -j1 -DElab.async=false --run ChangedContextAuditMain.lean --output corpus/changed-context.jsonl`. The freshness check compares the entire generated output with the committed file. CI uses its existing 30-second/2048-MiB guard; local checks retain the stricter 20-second limit. The monotonicity proof has a local 10000-heartbeat limit. Runs are serialized; there is no exhaustive search over the maximum context value.

From the repository root:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_changed_context_oracle
```

The adapter passed two tests including all 44 cases, and passed again after the maximum-value correction. Initial Lean compilation needed an explicit `Lean.toJson` namespace import; it was repaired before successful corpus generation. No resource timeout or memory-limit failure occurred. Local measurements are retained in the adjacent resource-statistics JSON. Full Rust workspace, formatting and lint results are recorded in the implementation-review report.
