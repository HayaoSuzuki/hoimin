# Four-operator implementation review

Base: f3703cd8 on 2026-10-28. The maintainer explicitly directed that the four
additions belong in the same PR 1134. Five passes below are separate inspection
focuses, not five independent reviewers. Independent workers owned nonoverlapping
builder/test files; Cargo and integration were managed by the parent with two
build jobs and no incremental/debug symbols.

## Stage 1: producers (five passes)

1. Identity: reviewed the ordinary dataclass proof separately from imported
   callable identity, preserving existing resolve behavior. Reviewed builtin
   sorted resolution and encoding's nominal versus trusted identity contract.
2. Scope: checked lambda function/class/comprehension boundaries, nested for
   target masking, matching outer loops, aliases and class-local conflicts.
   Restricted dataclass suites explicitly instead of promising arbitrary classes.
3. Edits: parser tests exercise grouped operands/defaults, Unicode and comments.
   Sort uses token-expanded spans; lambda edits Name spans right-to-left. Empty
   factories use literals with lambda-local names and no new builtin lookup.
4. Resources/cancellation: cancellation-aware target/keyword/read scans and
   callback stop paths were inspected. Avoided full codecs identity scans with
   no codecs import and rejected unrelated calls before keyword allocation.
   Removed an unused emitter source argument. No new runtime state store.
5. Red/green and counterexamples: four default-positive tests failed at zero
   candidates before implementation, then passed. Full Python-crate tests found
   one mistaken negative expectation: a module rebinding after class evaluation
   does not alter the factory captured earlier. Corrected that test to a positive
   without weakening production resolution. Independent workers each completed
   five passes and reported fixes (attribute spelling equality, codec fallback
   exclusion and cancellation polling).

## Stage 2: integration (five passes)

1. Catalog: appended four enum IDs without reordering; independently updated
   name/default/ranking and valid-Python inventory fixtures. Catalog is 101/82.
   Initial Lean producer status remains explicitly deferred, not newly proved.
2. Defaults/selection: public opt-out test now covers 39 promoted IDs, including
   all four, and compares retained candidate identities/descriptors. Explicit
   selections and saved subsets are not expanded. No family is enlarged.
3. Behavior: public plan/verify cases confirm baseline success, weak survival,
   stronger kills, complete reports and unchanged originals. Separate successful
   CPython outputs verify all three container factories, sort/read order, explicit
   callback overrides/name collisions and encoding data loss rather than errors.
4. Boundary review: fresh independent review found class-body closure lookup and
   multiline call line-selection gaps. Parent reproduced each (class: 2 vs 1
   candidate; sorted/encoding: 0 vs 1) before fixing. Added typed class namespace
   context and actual edit-start selection before allocation, with a common
   source-position selection predicate. All Python-crate tests pass afterward.
5. CLI regressions: the class-body/method-local output contract keeps class values
   [4,9] while method-local omitted captures become [7,7]; two public multiline
   plans select the actual key/errors line. Focused CLI suites pass: 88 config,
   5 default/selection, 29 execution contracts and 24 valid-Python tests.

## Stage 3: documentation (five passes)

1. Current scope: usage states guarded dataclass body limits, simple key shapes,
   nominal encoding methods and lambda allocation/introspection costs.
2. Counts/generated reference: current defaults are 82 of 101 IDs; prior 78/97
   notice is dated as historical. Generated CLI reference is regenerated.
3. History: original comparison and feasibility JSON are preserved. README and
   current OKF decision supersede temporary deferrals with implementation; old
   reconsideration explicitly links to the new current contract.
4. Discoverability: implementation/review reports are included in the audit
   source catalog and current decision, with actual base revision and hashes.
5. Review quality: recorded independent findings and red/green regressions rather
   than claiming the first review was clean. Existing runtime/security guards,
   arbitrary test commands and compatibility policy remain unchanged.

## Independent review and fixes

A fresh read-only reviewer inspected the tracked/new code and tests against
f3703cd8, without Cargo or edits. It reported two P2 findings, both accepted and
fixed in one pass with reproducing tests. It reported no other material or
optional findings. The complete workspace suite is rerun after the fixes; final
command outcomes follow below. No second review is substituted for regression
verification. Lint-driven changes keep one exhaustive ranking match and one
central collector setup, with explicit line-count expectations rather than
weakening enum exhaustiveness or splitting unrelated policy state.

## Stage 4: final verification

Final verification completed; actual passes and outcomes follow.

### Final five passes and observed results

1. Regression pass: after both independent-review fixes, `cargo test --locked
   --workspace --no-fail-fast` completed with 194 suite summaries: 4,001 passed,
   0 failed, 34 ignored. Existing local Unix-socket tests used the same required
   permission as before. The class-body and two multiline analyzer regressions
   were observed failing before fixes, then passed with the whole Python crate
   and full workspace. Original safety assertions and existing proofs remain.
2. Behavioral pass: CPython 3.15.0b1 beta additionally passes five default/opt-out/
   selection/verify tests and five four-operator executable-contract tests, covering
   eleven detailed contract scenarios. CPython 3.14.9 is the normal execution
   interpreter. Beta coverage is not stable-release certification or evidence
   of unexecuted platforms. Original synthetic comparison JSON is unchanged.
3. Quality pass: workspace all-target/all-feature Clippy passes with warnings
   denied. Workspace/vendor fmt checks, seven CLI-reference generator tests and
   its --check pass. Moved the growing executable-check fixture table out of its
   runner rather than suppressing its function-length lint. No new dependencies
   or lockfile changes; the one exhaustive ranking match keeps compiler coverage.
4. Artifact pass: 62 Python docs/boundary tests pass; the OKF checker reports 62
   pages with no missing Markdown/sources or unreachable pages. Safe YAML checks
   validate 58 concepts plus reserved-file structure. All 24 batch source hashes
   are refreshed/checked after this record is finalized. Documentation makes
   class-body/method, nominal API and metadata tradeoffs explicit. git diff --check
   passes; private untracked directories are not staged.
5. Resource/completion pass: all worker builds were centralized; two Cargo jobs,
   no incremental/debug data and at most two Rust test threads were used (beta
   checks used one test thread). Cargo target is about 5 GiB with ample free space;
   retain it for normal commit hooks, then cargo clean and record actual reclaimed
   space in the PR body. All seven issues now have product implementations in
   the same PR1134; no separate feature PR or permanent deferral is substituted.

No material reviewer finding or required local check remains unresolved.
The intentional scope limits and synthetic-evidence limits remain documented;
ignored manual benchmarks are not presented as executed performance measurements.
