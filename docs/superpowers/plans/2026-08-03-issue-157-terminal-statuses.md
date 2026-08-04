# Issue #157: Terminal mutant statuses end to end

Parent: [#157](https://github.com/tokyogas-tech/hoimin/issues/157)

## Covered on every supported runner

`run_e2e::mutation_timeout_propagates_to_the_final_report_and_exit_policy`
drives the production CLI with a project containing exactly one
`binary_add_sub` candidate. The baseline command returns normally, while the
same command observes the applied mutation and sleeps beyond
`--mutant-timeout 1s`. The final JSON report must contain exactly one mutant
with status `timeout` and termination `Timeout`, increment only the timeout
summary row, set `complete` to false, and produce exit code 4.

`run_e2e::ty_reports_a_surviving_nullable_contract_mutant` now pins the other
terminal exit-policy boundary: its exact single `survived` result exits 1.

These fixtures are integration tests in `crates/hoimin-cli/tests/run_e2e.rs`
because they cross analysis, workspace mutation, the real process handler,
reporting, and CLI exit policy.

## Hard-limit capability audit

The repository's ordinary Linux job uses the portable best-effort backend and
cannot distinguish an enforced out-of-memory or process-count failure from an
ordinary signal/exit. Pure report-policy tests and direct
`ProcessHandler` classification tests therefore do not satisfy either missing
full-stack row.

The workflow has a `linux-cgroup-v2-hard` job with the required production
backend, but it runs only when the repository Actions variable
`HOIMIN_CGROUP_V2_DELEGATED` equals `true`. On 2026-08-04,
`GET /repos/tokyogas-tech/hoimin/actions/variables` returned an empty variables
list (`total_count: 0`). Consequently no reachable repository workflow can
currently produce the required delegated cgroup-v2 evidence. A skipped job is
not coverage, and #157 must remain open.

The following narrowly scoped follow-ups should be created and linked from
#157. They are kept as proposals in this task because issue creation is handled
after implementation review.

## Proposed OOM follow-up

Title:

```text
test(linux): propagate cgroup OOM through a full CLI report
```

Body:

```markdown
Parent: https://github.com/tokyogas-tech/hoimin/issues/157

Exact missing audit row:

> Enforceable OOM termination propagates through a final run report and exit policy.

Acceptance criteria:

- Enable a reachable self-hosted runner labelled `linux`, `x64`, and
  `cgroup-v2-delegated`, and set the repository Actions variable
  `HOIMIN_CGROUP_V2_DELEGATED=true`.
- In the production `linux-cgroup-v2-hard` workflow job, execute the real
  `CARGO_BIN_EXE_hoimin run --format json` command. Do not inject a
  `ProcessFinished` event or call a report-policy helper directly.
- Use a project with exactly one selected mutant. The baseline must complete
  normally; only the applied-mutant branch may allocate memory beyond the
  configured hard cgroup limit.
- Bound the fixture and cleanup deterministically, require the cgroup capability
  probe to run without `SKIP:`, and prove the run leaves no fixture process.
- Assert exact exit code 4, `statuses == ["out_of_memory"]`, mutant termination
  `"OutOfMemory"`, `summary.counts.out_of_memory == 1`, and
  `summary.complete == false`.
- Attach a passing `linux-cgroup-v2-hard` Actions job URL before closing this
  issue or parent #157.
```

## Proposed process-limit follow-up

Title:

```text
test(linux): propagate cgroup process limit through a full CLI report
```

Body:

```markdown
Parent: https://github.com/tokyogas-tech/hoimin/issues/157

Exact missing audit row:

> Enforceable process-limit termination propagates through a final run report and exit policy.

Acceptance criteria:

- Enable a reachable self-hosted runner labelled `linux`, `x64`, and
  `cgroup-v2-delegated`, and set the repository Actions variable
  `HOIMIN_CGROUP_V2_DELEGATED=true`.
- In the production `linux-cgroup-v2-hard` workflow job, execute the real
  `CARGO_BIN_EXE_hoimin run --format json` command. Do not inject a
  `ProcessFinished` event or call a report-policy helper directly.
- Use a project with exactly one selected mutant. The baseline must complete
  normally; only the applied-mutant branch may keep enough real descendants
  alive concurrently to exceed the configured cgroup process limit.
- Bound the fixture and cleanup deterministically, require the cgroup capability
  probe to run without `SKIP:`, and prove every spawned descendant is gone.
- Assert exact exit code 4, `statuses == ["process_limit"]`, mutant termination
  `"ProcessLimit"`, `summary.counts.process_limit == 1`, and
  `summary.complete == false`.
- Attach a passing `linux-cgroup-v2-hard` Actions job URL before closing this
  issue or parent #157.
```

## Closure invariant

#157 remains open until both proposed follow-ups exist, are linked from #157,
and each has the required passing hard-backend CI link. Merging the portable
timeout and survivor coverage alone does not satisfy or close the two hard-limit
rows.

## Verification

```bash
cargo test -p hoimin-cli --test run_e2e mutation_timeout_propagates_to_the_final_report_and_exit_policy -- --exact
cargo test -p hoimin-cli --test run_e2e ty_reports_a_surviving_nullable_contract_mutant -- --exact
cargo test -p hoimin-cli --test run_e2e
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```
