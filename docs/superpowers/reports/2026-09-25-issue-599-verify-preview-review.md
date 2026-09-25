# Issue 599: Verify preview review and verification

## Design reviews before implementation

1. **Selection semantics:** traced `resolve_verify_selection`, `validate_requested_candidates`, shell candidate selection and core `CandidateLoaded`. Explicit selection uses discovery order while ranked selection uses its resolved vector. A saved-sequence sort would be wrong because descriptor validation excludes sequence. The design uses IDs from the existing validation rediscovery and requires explicit order parity tests.
2. **Side-effect and output boundaries:** read both dispatch paths, workspace validation-manifest construction, and metrics argument conversion. Added explicit writer-failure exit 2 and validation-before-output rules; retained read-only validation and rejected metrics at parsing. JSONL now explicitly means one preview object, avoiding confusion with run events.
3. **Compatibility and scope:** checked actual plan/ranking versions and `VerifiedPlan` construction sites (`rg` found only its production constructor). Kept independent preview schema 1, plan/ranking 4 and normal execution behavior; corrected the proposed help wording. Confirmed truncation success does not claim complete mutation execution. No additional selector or Lean state model is introduced.

## Plan reviews before implementation

1. **Spec coverage:** mapped every output/side-effect requirement to the two tasks. Added explicit saved-sequence tampering and borrowed writer failure to review focus, and parity checks against serial `mutant_started` order rather than completion order.
2. **Interface and feasibility:** inspected internal validator's `Result<(), PlanError>` and both dispatches. Its return can carry discovered IDs without changing error precedence; the plan explicitly places collection after validation. Repository search found no JSON Schema helper, so corrected the plan to check Python validator availability and report its limits.
3. **Isolation and completeness:** scanned for placeholders (no matches), checked that all edits live in the issue worktree, and checked preview metadata can be projected from the already parsed manifest. Added per-child TMPDIR instead of process-global environment changes and prohibited a manifest reread. Design/plan are committed before code edits.

## Execution evidence

Design and implementation plan prepared before production or test implementation. Implementation and test review results will be appended as the work is performed; no success is asserted in advance.

## Implementation review 1: dispatch and artifact boundary

Read the complete diff for CLI parsing, both dispatch paths and shared preparation. Both dry-run arms return before shell calls and metrics assignment. Public tests exercise real subprocesses, an empty dedicated TMPDIR, unchanged plan bytes and an external test marker; normal verification then creates the marker as a negative control. Truncated preview also succeeds with an impossible runtime free-space reserve, proving it does not reach worker preflight. Six focused tests pass after implementation.

## Test review 1: ordering expectations and valid fixtures

The initial explicit-order test changed saved sequences to 0 and 99; execution showed existing validation requires a permutation of 1..=N. This tested malformed input rather than order independence. Replaced those numbers with a valid reversed permutation (1 and 2 in saved-rank order), preserving disagreement with discovery order. Strict/diverse expected positions are hand-written; normal `mutant_started` order is separately compared. Six focused tests now pass. This corrects the fixture, not the production validation contract.

## RED/GREEN evidence

`CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test plan verify_preview -- --nocapture`: initial two public tests failed with `unexpected argument '--dry-run'`; expanded six-test run failed all six before production changes. After implementation and valid sequence-fixture correction, 6 passed, 0 failed (73 filtered). Logs: `/tmp/issue599-red.log` and `/tmp/issue599-green.log` in the execution workspace. No Python production or automated test code changed.

## Implementation review 2: selection and validation correspondence

Compared `VerifyPreview::new` with the unchanged `resolve_verify_selection` and core candidate filters. Ranked output uses the resolved ID vector directly; explicit output uses validated rediscovery order, with all descriptor comparisons still preceding success. Metadata comes from the same manifest instance, avoiding a validation/read race. The projection clones only selected IDs/paths, not source text or mutation payloads; its ID lookup scans retained candidates. Error precedence and normal verification selection remain unchanged. Root independently reviewed production code at 99655f0 and found no blocking issue.

## Implementation review 3: output compatibility and documentation

Compared each serialized field and enum with the independent schema and README contract, including the one-object JSONL representation, null explicit offset and truncated-success exit code. JSON Schema 2020-12 validation with system Python `jsonschema 4.26.0` accepted six real CLI documents (strict, diverse-offset and explicit selectors, each JSON/JSONL); malformed-row negative controls failed validation. Normal report/event schemas remain untouched. Human rendering takes selector names from the same serde values as JSON. The touched obsolete version-3 help now says version 4.

## Test review 2: negative controls and truncation

The initial truncation test covered successful preview and metadata but lacked actual execution parity. Added a normal JSONL verification before the impossible runtime-reserve variant, comparing `mutant_started` IDs to preview and asserting normal execution retains exit 2 for incompleteness. Its marker is asserted present before removal, establishing a working test-command control. All six focused tests pass after this addition. The runtime-reserve variant then exercises both owned and borrowed dispatch without runtime artifacts.

## Test review 3: malformed input and rendered fields

Audited the tests against the acceptance criteria and schema fields. Added explicit checks that the six-candidate fixture contains the intended equal-score tier and a lower tier, human output contains path/line, and explicit preview leaves TMPDIR/session absent. Extended validation parity to malformed JSON, missing manifests and empty diverse selection, beyond schema/rank/descriptor/source/fingerprint/offset/limit/unknown-ID cases. Retained independent literal order expectations and the normal-command marker negative control. Clippy found byte-counting and statement-style issues only in new tests; replaced byte counting with line-count plus terminal-newline checks and added the required semicolon.

## Execution decisions and limits

- Native execution was authorized by the task; parent/root provides the independent review and handles push/PR. No agent was spawned by this implementer.
- Existing validator rejects non-permutation sequences; the explicit-order fixture now uses a valid reversed permutation. Runtime candidate order remains discovered, not saved sequence.
- JSON Schema validation uses system Python because the repository virtualenv lacks jsonschema. No dependency change was made.
- No Lean model or Lean rebuild was added: the selection algorithm and state transitions are unchanged. Existing Rust oracle suites are part of workspace verification.
- Observation platform: macOS arm64. No Linux/Windows execution is claimed. Temporary-directory snapshots plus dispatch review establish the tested side-effect boundary; they are not a syscall trace.

## Final verification

- `CARGO_BUILD_JOBS=2 cargo test --workspace`: **2271 passed, 0 failed, 22 ignored**, across 94 summary blocks (`/tmp/issue599-workspace-tests.log`). The root baseline was 2265 passed/22 ignored; this branch adds six tests.
- `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test plan`: **78 passed, 1 ignored** before final test-review additions. Final `... --test plan verify_preview -- --nocapture`: **6 passed** after all additions and helper extraction (`/tmp/issue599-preview-final.log`).
- `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --all-features -- -D warnings`: **passed** (`/tmp/issue599-clippy-final.log`). Extracted the shared JSONL started-ID reader after Clippy's test-length warning; literal expected orders remain in each test.
- `cargo fmt --all -- --check` and `git diff --check`: **passed**.
- JSON Schema 2020-12: schema itself and **6 actual CLI JSON/JSONL outputs passed**, malformed row controls rejected, using system Python `jsonschema 4.26.0`.
- OKF: **29 Markdown pages** passed YAML and reserved-file checks; **5 new source records** passed hash, link and matching-footnote checks. All links in the three edited concept/catalog pages resolve. Confirmed the root index directly links the selection contract and both catalogs; a preliminary check incorrectly assumed a design-subindex link, and the corrected traversal check passed.
- Root-run Python gates on this branch: Ruff formatting **19 files unchanged**, Ruff check **passed**, pytest **104 passed** (11.55 s). Root used the isolated locked quality environment and the known process-monitor permission escalation. Logs: `/tmp/hoimin-599-ruff-format.log`, `/tmp/hoimin-599-ruff-check.log`, `/tmp/hoimin-599-pytest.log`.

All three design, plan, implementation and test review passes are recorded above (12 total). No unresolved implementation finding or deferred minor remains from these self-reviews. The root agent handles push and PR; the independent review and additional gates below completed after this implementation handoff.


## Independent review and additional root-run gates

The final independent reviewer approved the complete change with no findings. The root agent built the macOS arm64 release wheel with maturin and ran `tests/wheel_smoke.py`; both commands exited successfully. Logs: `/tmp/hoimin-599-wheel-build.log` and `/tmp/hoimin-599-wheel-smoke.log`. The built wheel is `hoimin-0.1.0-py3-none-macosx_11_0_arm64.whl`.

The unchanged vendored parser passed its formatting check and dedicated Clippy check in the root baseline. The Clippy log is `/tmp/hoimin-vendor-clippy.log`; this branch does not modify the parser.

Root-run `CARGO_BUILD_JOBS=2 cargo test --workspace --all-features` completed with **2256 passed, 0 failed, 22 ignored**, across 94 summary blocks. This includes the core and CLI contracts feature gates. Feature-dependent test configuration explains the different count from the default workspace run. Log: `/tmp/hoimin-599-contracts.log`. No production or test code changed after these final gates; this follow-up records evidence only.
