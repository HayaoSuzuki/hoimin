### Spec Compliance

- ❌ Issues found: the production behavior matches Task 5, but the new `projection_case!` tests combine matching-wire serialization/acceptance and contradictory-wire rejection in each generated test, contrary to the binding single-behavior/AAA requirement (`crates/hoimin-core/tests/report_status.rs:53`, `:73`). See Important issue I1.
- ✅ The domain record remains outcome-only; serialization computes status with `outcome.status()`, while the closed input view requires, checks and discards the received status (`crates/hoimin-core/src/report.rs:504`, `:537`, `:558`, `:571`). No second execution/persistence classifier or status field is introduced.
- ✅ Human rows contain the full candidate ID, location, operator, quoted original/replacement and canonical verdict, with Task 2's field-level `HumanText` escaping (`crates/hoimin-cli/src/report/human.rs:88`).
- ✅ Schema 9 requires the existing seven-value status definition; Rust readers own semantic equality, exactly as R11 requires. The result schema inherits the event requirement (`docs/json-schema/run-event.schema.json:1070`, `:1096`; `docs/json-schema/run-result.schema.json:25`). Only the four current v9 goldens change; no historical v8 fixture, command, exit policy or version bump to 10 appears in this diff.
- ✅ Usage, compatibility, both repository consumer skills and the existing report/spool concepts describe the new field and schema-only semantic limit (`docs/usage.md:1083`; `docs/compatibility.md:15`; `skills/hoimin-mutation-testing/SKILL.md:31`; `skills/hoimin-mutation-improvement/SKILL.md:77`; `docs/knowledge/design/session-report.md:281`; `docs/knowledge/design/json-report-spool.md:34`).
- ⚠️ Cannot verify from this task diff: complete unchanged summary/progress/resume call-path coverage, native Linux/Windows behavior and combined metadata/catalog consistency. The diff preserves the domain shape and existing classifiers, and the reported focused tests include resume/progress. Native and combined checks remain final integration/CI scope, not Task 5 blockers (`task-5-report.md:58`, `:77`, `:81`).

### Strengths

- The private borrowed/owned wire views keep compatibility plumbing out of the public model and retain unknown-field and duplicate-field rejection (`crates/hoimin-core/src/report.rs:520`, `:551`).
- Outcome coverage uses literal classifications for complete/close-timed-out exits, timeout, OOM, process limit, cancellation, reuse and synthetic not-run. The signed-exit property derives its expected value independently rather than calling production `status()` (`crates/hoimin-core/tests/report_status.rs:89`, `:156`, `:272`).
- Raw duplicate-key tests preserve evidence before typed decoding, covering both equal and conflicting duplicates through direct/tagged serde and JSON/JSONL file readers (`crates/hoimin-core/tests/report_status.rs:229`; `crates/hoimin-cli/tests/progress.rs:3332`).
- Public progress tests invoke the executable, use a matching positive control and assert the mismatch cause as well as exit 2. Review exercises the public command entry with a usable report and required store option (`crates/hoimin-cli/tests/progress.rs:3291`; `crates/hoimin-cli/src/review/tests.rs:634`).
- Spool assertions verify actual final JSON bytes, resume assertions preserve executed/reused provenance, and parallel human tests combine an actual jobs-2 run, deterministic reverse completion and arbitrary external text (`crates/hoimin-cli/tests/report_handler.rs:1497`, `:1602`, `:1660`, `:2465`, `:2919`).

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- **I1 — binding test-structure requirement:** `crates/hoimin-core/tests/report_status.rs:53`, especially `:65` and `:73`. Every `projection_case!` expansion first serializes a valid outcome and asserts its projection, then deserializes matching input in the Assert section, then constructs a contradictory status and performs another decode/assertion. Matching acceptance and contradictory rejection are different behavioral cases; the latter's execution is also conditional on every earlier assertion succeeding. Consequently the generated tests do not meet the brief's explicit single-behavior/AAA constraint, and one failure masks the other behavior's evidence. Generate separate projection, matching round-trip and contradictory-rejection tests for each outcome, preserving the literal expected classifications and placing each test's operation in its Act phase. Positive-control validation as fixture setup in the dedicated rejection tests can remain.

#### Minor (Nice to Have)

- None.

### Assessment

**Task quality:** Needs fixes.

**Reasoning:** The implementation itself is compact, reuses the canonical classifier and escape helper, and has real boundary coverage; no product defect was found. I1 is a narrow test-only change required by the binding test contract before this task gate passes.

- **Focused unchanged-source check — duplicate-key normalization risk:** inspected only the existing `OutputEvent` decoder at `crates/hoimin-core/src/report.rs:643`; its `RawValue` envelope and typed finished-event dispatch (`:664`, `:677`) preserve duplicate status keys for the new closed decoder.
- **Focused unchanged-source check — competing classifier/provenance risk:** inspected `MutationOutcome::status()` at `crates/hoimin-core/src/resume.rs:217`; execution still delegates to the canonical result classifier, and reuse/not-run retain their existing mappings (`:223`, `:224`, `:230`).
- **Focused unchanged-source check — missing JSON-result schema migration risk:** inspected the schema version and mutant item reference in `docs/json-schema/run-result.schema.json:8`, `:25`; schema 9 references the updated event definition, so a separate result-schema hunk is unnecessary.
- **Evidence checked:** read the task brief/report and supplied change view. Initial combined output was truncated; recovered the brief/report and bounded ranges of that same diff, without rerunning git or rereading changed functions. Checked existing RED logs and final core/CLI/Python/clippy/commit logs: meaningful missing-status/human-ID failures precede the implementation; final records show core 32/4/31/29, CLI 138/146/1, Python 106 passed and four passed commit hooks. No warning was found in those inspected logs. No test or suite was rerun, and no product/branch mutation was performed.
