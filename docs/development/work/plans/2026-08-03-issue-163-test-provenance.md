# Issue #163 Narrow Test Provenance

Parent: [#163](https://github.com/tokyogas-tech/hoimin/issues/163)

## Decision

The repository uses `// pins: issue #NNN` only where an assertion or operation preserves a
surprising policy or a former defect whose expected outcome is not self-evident. Ordinary exact
assertions, generators, fixture constructors, and straightforward schema values remain unannotated.
The comment sits immediately before the outcome it explains and names a precise former-defect
issue instead of the consolidation/property umbrella when one exists.

This task audits only tests added by consolidation Tasks 2–15. It does not perform #163's proposed
repository-wide backfill. Older deterministic tests remain unchanged when a new generated
regression from Tasks 2–15 carries the required provenance.

## Annotated tests

| Test | Provenance | Why the outcome needs context |
| --- | --- | --- |
| `target::git::tests::generated_hostile_zero_context_diffs_match_ranges` | [issue #101](https://github.com/tokyogas-tech/hoimin/issues/101) | Patch-body lines that lexically look like `---`/`+++`, `@@`, or binary metadata must remain content inside the active zero-context hunk. The independent destination-range equality is the non-obvious boundary that prevents content from redirecting later hunks. |
| `compare_property_killed_to_survived_uses_id_despite_duplicate_content` | [issue #102](https://github.com/tokyogas-tech/hoimin/issues/102) | Duplicate content keys intentionally produce zero ambiguity when stable candidate IDs still pair the reports; the regression must be counted rather than discarded. |
| `adversarial_schedule_regression_111_ordered_cancellation` | [issue #111](https://github.com/tokyogas-tech/hoimin/issues/111) | Cancellation during ordered collection must drain and terminate rather than resume spool reads and later report a selected candidate missing. |
| `adversarial_schedule_regression_112_empty_ordered_spool` | [issue #112](https://github.com/tokyogas-tech/hoimin/issues/112) | An ordered verification selecting candidates must reject a zero-record spool instead of finalizing successfully with an empty result. |
| `adversarial_schedule_regression_113_persistence_interruption` | [issue #113](https://github.com/tokyogas-tech/hoimin/issues/113) | Cancellation while persistence is in flight must retain the already classified result instead of converting it to `NotRun` and diverging from the session database. |

The Git property is pinned to #101 rather than umbrella #165 because its assertion protects hunk
header anchoring. The separate #105 ambiguity concerns binary filenames containing ` and `; its
deterministic regression predates Tasks 2–15 and is outside this narrow backfill.

## Tasks 2–15 audit and exclusions

| Task | Tests reviewed | Decision |
| ---: | --- | --- |
| 2 / #155 | Real CLI session ownership contention and report/session parity | Excluded: typed contention failure, integrity, and parity assertions are direct acceptance outcomes. |
| 3 / #153 | First real SIGINT lifecycle | Excluded: exit 130, incomplete report, incomplete session, and descendant reaping are explicit signal-test outcomes. |
| 4 / #158 | Real binary help/version and JSON stream routing | Excluded: successful stdout and diagnostic stderr routing are ordinary CLI assertions; #107 context is not needed to understand them. |
| 5 / #154 | Real run-to-progress workflow | Excluded: consuming two real reports and comparing their progress is self-describing. |
| 6 / #156 | Changed-selection run/plan wiring | Excluded: selecting only the edited function and matching run/plan IDs are ordinary exact assertions. |
| 7 / #161 | Two-lock-by-five-operation session matrix | Excluded: every typed success and timeout bound is explicit in the matrix and has no narrower former-defect issue. |
| 8 / #162 | Linux backend fault boundaries | Excluded: test names state abnormal exit, root-before-descendant cleanup, and the no-numeric-PID signal policy directly; Windows rows remain separately tracked follow-ups. |
| 9 / #165 | Hostile Git diff and C-quote properties | Annotated only the #101 hunk-boundary equality. The encoder/decoder round trip and arbitrary-input totality are conventional properties. |
| 10 / #168 | Progress algebra | Annotated only the #102 stable-ID/duplicate-content regression. Self-comparison, reversal, and permutation assertions are standard algebraic properties. |
| 11 / #164 | Adversarial machine schedules | Annotated the generated #111, #112, and #113 regression operations. General terminal-state, accounting, unique-ID, and sequence-validity invariants remain unannotated. |
| 12 / #157 | Timeout, survivor, OOM, and process-limit reports | Excluded: each status, termination, count, completeness flag, and exit code follows directly from the named fixture. |
| 13 / #160 | JSON, JSONL, and SQLite golden eras | Excluded: optional-field values, era boundaries, and schema versions are straightforward compatibility assertions and are explicitly documented beside the corpus. |
| 14 / #166 | Candidate spool replay and size boundaries | Excluded: replay suffixes and newline accounting are self-evident from the test names; generators/helpers are never provenance targets. |
| 15 / #167 | Fingerprint invariance and sensitivity | Excluded: canonical-model equality/difference, normalized-target construction, and field edits are ordinary property assertions. |

Existing deterministic tests such as `ordered_candidate_filter_rejects_an_empty_spool` and
`cancellation_during_result_persistence_reports_the_classified_result` were evaluated but not
modified: they predate Tasks 2–15, and the new generated #112/#113 regressions above provide the
prospective provenance required by this consolidation without starting a repository-wide backfill.

## Verification

```text
cargo test -p hoimin-cli target::git
cargo test -p hoimin-cli --test progress compare_property
cargo test -p hoimin-core --test machine adversarial_schedule
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
