### Finding Verdicts

Fix range `6df6dcb7deec71946845043408d4ce256dc55044..d5eb8012b87989285a9c5e439ec299c72ef667f0`.

1. **Module set receiver after reflective rebinding — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust/local_initializers.rs:99` rejects an exposed module dictionary when the initializer belongs to the module. The lexical visitor at `crates/hoimin-python/src/analyzer/rust.rs:2659` recognizes both dictionary spellings. Direct and function-body reads of the replaced module variable are covered by a CPython oracle that observes the custom receiver. An independent function-local set literal and an ordinary module set retain their swaps.
2. **Inline and external declarations of one root regex mode — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust/regex.rs:542` constructs a single covering range, orders the two components by source offset and copies their intervening text. Projection changes a literal reference at its use. The runtime cases cover both toggle directions, reordered keywords, repeated external terms, unrelated IGNORECASE, an internal scoped override, retained comments and a second consumer. Selection is checked at the covering start. This implements the clarification in `d4658222` within the existing saved-edit format.
3. **Malformed literal with unknown flags — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust/regex.rs:71` validates a known literal against ordinary and verbose grammar. Both the constructor-validity path and direct module calls use it. The direct and compiled unclosed-parenthesis cases now produce zero edits. Ordinary-only, verbose-sensitive and dynamic pattern controls retain method eligibility as specified.
4. **Copying referenced literals after their candidates have lost rank — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust.rs:346` permits skipping validation once overflow is known and the best possible key cannot win. At `rust.rs:356`, a validated strictly worse key can establish overflow before rendering. Equal keys still receive the necessary exact identity check when overflow is unknown. The implementation keeps the earliest-emitted candidate for equal rank, and later visits with better keys can still enter. `crates/hoimin-python/src/analyzer/rust/regex_tests.rs:542` checks every returned field and counts projected bytes at K=1 and K=3. The additional cross-token case at line 613 bounds repeated validation as well.
5. **Uncancellable signature preprocessing — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust/optional_keywords.rs:92` passes the supplied callback to the shared constructor at `crates/hoimin-python/src/analyzer/rust.rs:1396`. That constructor latches cancellation across import discovery, lexical indexing and finalization, and returns an error before publishing any partial index. The two named stage tests at `crates/hoimin-python/src/analyzer/rust/optional_keyword_selection_tests.rs:321` trip a one-shot callback inside the specified traversal and stop at four statements.
6. **Independent heap measurements — ADDRESSED.** `crates/hoimin-cli/tests/regex_heap.rs:24` runs one named shape/selector pair per invocation. The twelve rows preserve the four fixtures, three selectors, shared measurement mutex and K=3.
7. **Forwarding suite entry — ADDRESSED.** `crates/hoimin-python/src/analyzer/rust/statement_order.rs:114` owns the collector's single suite entry. The old forwarding method is gone, and the stopped-sink fixture calls the same entry as ordinary traversal.

### New Breakage in the Fix Diff

None found. Selection and focused-profile filtering use span and operator metadata before lazy rendering. Prefix admission retains the same start/operator/emission ordering and checks for an existing failure first. The scope-specific namespace guard and both scan interpretations are shared by their affected callers.

### Out-of-Scope Observations

The earlier native Windows disposition stands. The prior broad CLI contracts run had 29 CLI-library failures. Ten reproduced individually on the isolated base and 19 share the documented signatures and unchanged paths. The scoped fix gates did not repeat the full workspace or CLI contracts suites. The prior fixture-local Git line-ending warning remains informational.

### Design Diagnostic

| Diagnostic | Verdict | Evidence |
| --- | --- | --- |
| Modules have one describable purpose | Pass | Lexical initialization, literal mapping, regex validation and prefix admission have identifiable owners. |
| Interfaces are simpler than implementations | Pass | A domain producer supplies the proposed location and renderer. The prefix owns the retention decision. |
| Implementations can change behind their interfaces | Pass | The prefix determines the validation cutoff and handles the overflow witness without exposing the rank comparator to regex code. |
| Interface comments describe guarantees | Pass | The prefix distinguishes unvalidated from validated edits, and the name-index constructor documents all-or-cancel publication. |
| Reviews discuss design | Pass | The cross-token work counter exposed repeated validation after replacement copying had already been addressed. |
| Important decisions are hidden | Pass | Initializer ownership determines how module exposure affects that proof. The ordinary/verbose grammar check is shared and cached internally. |
| Boundaries are understandable from contracts | Pass | The root clarification, usage guide and named executable cases identify source, scope, flags and selection behavior. |
| 10–20 percent of effort invested in design improvement | Unverified | The record documents design changes but contains no contemporaneous time split. |

**7/8 supported, 8.75/10.** The unmeasured effort percentage is the only unsupported diagnostic.

### Verdict

**Fix round: all findings addressed, no new Critical/Important breakage.** The recorded affected gates passed, including 5,150 analyzer tests (5 ignored), both CPython contract targets, default selection, ranking, line selection, all 12 heap measurements, strict lint, formatting, documentation and authentic old/mixed-plan replay. No passing suite was rerun during this re-review.

