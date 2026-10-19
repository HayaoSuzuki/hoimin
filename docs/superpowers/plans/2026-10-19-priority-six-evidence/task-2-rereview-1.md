# Task 2 scoped re-review, fix round 1

Scope: `3d9e33d178a451e39e182495f4a5bb3a4c5ebfa0..6d78789eca05fad577b2a8dcce83077f9389ee06`; previous findings and new breakage in this fix diff only.

1. **“An external executable name still injects controls into Clap Usage.” — ADDRESSED.** `crates/hoimin-cli/src/cli.rs:640` fixes the shared command factory's `bin_name` to trusted `hoimin`, exactly as R10 requires, before Clap builds help/Usage. This removes argv[0] as display provenance without escaping generated separators. `crates/hoimin-cli/tests/plan.rs:5903` supplies actual process argv[0] and checks the correct stream, exit status, exact trusted Usage and absence of controls other than renderer LF. `:5941` and `:5957` generate separate error/help cases for ESC, CR, LF, TAB, mixed controls and a Unicode alias.

2. **“The new example tables run multiple cases inside a single test rather than generating the required parameter cases.” — ADDRESSED.** `crates/hoimin-cli/tests/plan.rs:5765` generates distinct filename/format tests and `:5833` generates malformed-argument routes. `crates/hoimin-cli/tests/report_handler.rs:2540` generates the literal text cases for each human boundary, machine format and started-event kind; `:2806` generates the two independent diagnostic-layout cases. `crates/hoimin-cli/tests/review_cli.rs:573` generates separate filename/format cases with independently arranged review fixtures. The former scenario loops are removed; each generated test invokes one scenario helper. Assertions about several fields of that same arranged result remain together, consistent with the one-case requirement. Literal expectations, dry-run markers/plan-byte checks, machine-value checks and broad properties remain intact.

New breakage in fix diff: **None found.** The only production behavior change is the authorized trusted executable name. The test refactor retains the original example matrix and independent expected strings; metadata changes are limited to the two affected active test-source entries.

Out-of-Scope Observations: **None.** Unaffected production code was not reopened.

Evidence inspected: `/tmp/hoimin-priority-six-task2-fix1-red-argv0.log:85` shows 10 failing and 2 passing original-behavior argv[0] cases; `/tmp/hoimin-priority-six-task2-fix1-green-argv0.log:19` shows all 12 passing after the fix. The focused log reports 82 + 149 + 135 + 23 + 7 = 396 passing tests and one existing ignored case; `/tmp/hoimin-priority-six-task2-fix1-final-layout.log:9` shows the two final generated layout cases passing. Existing GREEN/focused/layout/commit logs have no warning/error matches. No tests, Cargo operations or git commands were rerun.

Limits: this is a source-and-existing-evidence re-review, not independent execution or whole-branch approval. Native Linux/Windows remain unverified.

**Fix round: All findings addressed, no new Critical/Important breakage.** Task 2 spec compliance for the reviewed findings: ✅; task quality: Approved.
