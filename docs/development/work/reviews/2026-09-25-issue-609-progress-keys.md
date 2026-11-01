# Issue 609 review and validation record

Design and plan were committed as `00b374e` before production or test edits. Each contains three separate self-reviews. Scope: allocation behavior of adjacent-report comparison; no schema or state policy change.

## Implementation self-reviews

1. **Identity and hash collision correctness.** Compared every old field and enum branch against the new types: path remains a Camino path, original/replacement/operator remain string values, symbol keeps its Option discriminant, and candidate ID stays the selected identity for Matching. Derived Eq and Hash operate on referent values, so distinct allocations compare equally and hash collisions still invoke full tuple equality. No digest, pointer equality, normalization or omitted field was introduced. The different-ID path still constructs the content index instead of dropping comparison information.
2. **Ownership and lifetime paths.** Traced key construction, HashMap insertion, duplicate union and all three inconclusive insertions. Keys contain only borrowed fields and Copy never clones text. Both indexes remain alive until the comparison ends; returned Comparison and ProgressResult do not contain borrowed keys. Candidate-set eligibility already borrowed IDs and needs no change. No unsafe code, interning or report ownership changes were added.
3. **Decision and observable-output audit.** Inspected the complete production diff and the accumulator: eligibility, ordering of ambiguity exclusion, common counting, conclusive score denominators, state precedence and stall resets are unchanged. Input parsing, warning emission and output serialization are untouched. Existing Lean-generated progress decision/input consumers cover policy independently. This ownership refactor does not change the formal model, and no Lean process was launched.

## Test self-reviews

1. **Measurement validity.** Existing progress_heap measures file/history costs, which would conceal cloning for JSON document inputs. Added a separate allocator binary with exactly one test and no background task, prepares both reports before begin, and stops measurement before assertions/formatting. Fixed 16 candidates grow from 32 bytes to 256 KiB for every key field, including matching IDs. The 64-KiB allowance is below one large field clone. This is a public compare_reports allocation test, not a source-text check or a fabricated byte estimate.
2. **Negative controls and auxiliary sets.** Initial RED failed at DifferentIds (16,020 → 43,262,606 bytes). Review found immediate failure hid later scenario measurements, so the test now collects all four measurements before asserting bounds. DuplicateContent has eight repeated keys in each report; Inconclusive has sixteen equal keys across distinct IDs; MatchingIds protects the ID branch. All scenarios assert literal counters, score availability, state and stall count. This catches a fix that leaves auxiliary sets cloning or returns early to save allocations. Clippy flagged assigning_clones in fixture preparation; changed it to clone_from outside the measurement window.
3. **Semantic and public boundaries.** Added per-field fallback distinctions, None versus empty symbol and a separately allocated equal-content control whose ignored metadata changes. Added a literal Comparison for ambiguous/inconclusive unions with added/removed keys and a 0.5 score improvement. Public CLI run emits both JSON and JSONL reports that are saved without editing; a source comment shifts IDs and progress must retain the common survivor with indeterminate state. Existing tests retain stable-ID duplicate-content pairing, warnings/order, status variants and stall chain coverage. Long path strings in the allocation test are in-memory keys, not filesystem paths; public CLI fixtures use normal paths.

## Validation

- RED before production change: `cargo test --offline --locked -p hoimin-cli --test progress_compare_heap -- --nocapture` failed its allocation bound, not compilation or fixture parsing.
- Focused GREEN: progress integration tests (75), new allocation test, and both existing Lean progress oracle consumers passed.
- Full workspace: `cargo test --offline --locked --workspace` exited 0; 2,290 passed and 22 ignored across 95 integration/unit/doc-test result groups. Includes existing history/JSONL heap suites and all progress tests.
- `cargo clippy --offline --locked --workspace --all-targets --all-features -- -D warnings`: exit 0 after fixture correction.
- `cargo clippy --offline --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- Workspace fmt, vendored parser fmt and `git diff --check`: exit 0.
- Final allocation test rerun after fixture correction: exit 0. Peak requested live heap was identical for 32-byte and 256-KiB text in each scenario:

| Scenario | Small bytes | Large bytes |
| --- | ---: | ---: |
| Different IDs | 7,144 | 7,144 |
| Duplicate content | 6,776 | 6,776 |
| Inconclusive | 9,616 | 9,616 |
| Matching IDs | 7,144 | 7,144 |

These numbers measure allocation requests during comparison only, not RSS, parser memory or runtime. Hashing/equality still scans text as required by existing semantics.
- Python checks were attempted but the shared environment lacks ruff and pytest. No Python code changed; root confirmed no environment installation is needed and CI remains the Python verification authority.

## Independent review

The issue-606 agent independently reviewed the branch against 43989c2 through root coordination, covering full tuple equality, lifetimes, scalar-only output, the four allocation scenarios and public JSON/JSONL coverage. Reported no blockers. Root handles publication and CI.
