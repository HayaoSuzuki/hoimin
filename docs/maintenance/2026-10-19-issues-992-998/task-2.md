# Task 2: shared report identity validation (#992)

The current JSON and JSONL report reader now rejects a candidate whose stable
ID disagrees with its identity fields. This happens before progress renders
regression or saturation results, with exit 2 and the input path and reason.
The reader preserves its existing structural diagnostic precedence.

`CandidateIdentityRef` borrows schema, source hash, path, span, operator and
replacement from candidates or descriptors. Owned identity producers and the
borrowed check use the existing domain/framing/canonicalization encoder. The
reader does not clone original/replacement payloads, load the workspace, compare
whole configurations, or hash display metadata. Source-validation scratch
accounting retains its conservative allowance for the former identity copies.

Review consumes the common guarantee, removes its report-specific hash check,
and retains plan descriptor and source validation. Common identity errors now
precede review project checks; independently valid reports from another project
still produce the project diagnostic. Display-only plan descriptor differences
still fail review even though progress accepts such report changes.

Fixture IDs are assigned after arranging identity fields. Current v8 document
and event goldens and their typed producer use genuine IDs; started references
and diagnostic references follow those IDs. The Lean progress-input adapter now
recomputes identity after varying spans, so all its non-identity classification
cases continue reaching their intended boundary. Comparison-only synthetic
values and intentionally invalid identity cases remain explicit test inputs.

## Requirements self-review (five passes)

1. Re-read #992 and the accepted spec: reject both current formats before
   comparison, exit 2, include path/reason; no arbitrary-ID exemption.
2. Enumerated every identity-bearing field. Candidate schema belongs to the
   core descriptor projection; reports use the fixed candidate schema. Both
   span components, path, hash, operator and replacement need negative cases.
3. Checked positive requirements: same-ID status changes, original/line/column/
   symbol changes and independent discovery order must remain accepted.
4. Checked exclusions: workspace, whole config and extra display equality are
   absent from progress; review retains its stronger plan/source requirements.
5. Audited ingestion producers rather than only the reported reproduction:
   progress helpers/manual IDs, goldens, typed golden producer, Lean adapter,
   and diagnostics all need coherent IDs; comparison-only tests remain useful.

## Design self-review (five passes)

1. Put identity projection and hashing in core; the common reader owns report
   consistency so progress and review cannot diverge.
2. Kept one hashing encoder, unchanged domain/tags/framing, path canonicalization
   and lowercase source hash. Existing owned API delegates to that encoder.
3. Borrowed replacement/operator/hash/path directly. No owned descriptor or
   owned identity is created for the reader check.
4. Reviewed diagnostic ordering against existing review tests. The first broad
   CLI run exposed duplicate-before-identity precedence; moved identity checks
   after existing structural checks rather than weakening the test.
5. Reviewed API depth: one projection, stable ID operation and checked match
   operation hide the format details. Software-design-philosophy diagnostic
   score: 10/10 within this scope; no failed rows or new configuration surface.

## Implementation self-review (five passes)

1. Read the full core diff against the pre-change encoder: only the borrowed
   input changes, with no changed hash bytes or owned-producer semantics.
2. Traced JSON and JSONL convergence into `validate_structure`; checks occur
   before either usable or unusable reports can reach progress comparison.
3. Read review diff: report duplicate hash removed; plan descriptor comparison
   retained; source hash/descriptor/identity checks retain their ordering.
4. Audited fixture setup calls after span/path/hash changes and checked JSONL
   started IDs plus diagnostics. Intentional forged-ID and duplicate-ID cases
   are not silently repaired.
5. Checked diff scope, formatting and accounting: no new dependencies, no
   changed budget arithmetic, no workspace access and no production payload
   clone. Existing large progress/review test files keep their established
   organization, using macros for parallel input dimensions.

## Tests self-review (five passes)

1. RED evidence uses real published candidate IDs, so it catches the missing
   validation rather than an arbitrary test-only ID mismatch.
2. Negative matrix covers JSON/JSONL and six mutable report identity dimensions;
   separate stolen-ID killed-to-survived cases exercise the regression gate.
3. Positive matrix independently accepts excluded fields, status changes and
   reordered discovery. Existing real-run fallback and ambiguity tests retain
   genuine distinct identities by changing hash/span before ID assignment.
4. Core fixed-vector assertions use a pre-existing published literal; property
   tests select existing published vectors and vary excluded display fields.
   Setup ID generation is not used as the identity oracle.
5. Reviewed broad scope output, including all Lean adapter cases and review
   descriptor checks. Initial duplicate-precedence failure was fixed, not
   removed. No full workspace test claim is made here.

## Verification evidence

All Cargo commands used `CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target`,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `RUST_TEST_THREADS=2`; tests used `--offline`.

- RED: `cargo test --offline -p hoimin-cli --test progress identity_json`:
  12 failed, 0 passed before implementation. All inconsistent identities were
  accepted with exit 0 instead of 2. Log: `task-2-red.log`.
- GREEN, same command: 12 passed. Log: `task-2-green.log`.
- Initial progress scope: 111 passed. Log: `task-2-progress.log`.
- First expanded CLI run: 715 passed, 1 failed, 25 ignored. Failure was
  `review::tests::checked_report_duplicate_error_precedes_identity_conflict`;
  preserving structural-before-identity validation fixed it.
- Final CLI command: `cargo test --offline -p hoimin-cli --lib --test progress
  --test report_handler --test review_cli --test lean_progress_input_oracle
  --test lean_survivor_review_oracle`: 901 top-level tests passed, 25 existing
  ignored probes. Breakdown: 716 unit, 127 progress, 36 report handler, 17 review
  CLI, 4 Lean progress input, 1 Lean survivor review. Log:
  `/tmp/hoimin-issues-992-998/task-2-cli-scope.log`.
- `cargo test --offline -p hoimin-core --test candidate_policy --test
  mutation_outcome`: 23 + 17 passed, including fixed vectors, canonicalization
  and borrowed-projection properties. Log: `task-2-core-scope.log`.
- `cargo fmt --all` and `git diff --check` passed.

Workspace tests belong to final integration. The existing ignored allocation/
release probes were not activated by this task. Commit hooks remain enabled.
