# Issue #350: resume-compatible operational limits

## Result

Changing `--jobs` or `--max-output` no longer creates a different session
fingerprint. A resumed command can change worker concurrency and per-process
output retention while reusing determinate `killed` and `survived` results from
the newest compatible incomplete run.

The fingerprint schema advances from 5 to 6. An incomplete run recorded with
schema 5 remains incompatible because its digest encoded both fields.

## Compatibility boundary

`--jobs` controls the current run's worker creation, scheduling, and workspace
reservation. The current configuration still validates `jobs <= max_processes`,
and current memory, process, copy, workspace, and free-space guards remain in
force. Session lookup reuses only determinate results. It reruns `timeout`,
`out_of_memory`, `process_limit`, `error`, and `not_run` results under the new
worker count.

`--max-output` limits retained stdout and stderr without stopping pipe drainage
or changing process exit classification. The session stores only a mutant ID
and status for lookup. A reused result emits synthetic output with null process
termination and null retained output, so it does not import data captured under
the earlier retention limit.

Preflight validation and the baseline still run for every resumed command.
Reports contain the current normalized configuration, including the updated
worker count and output limit.

This compatibility rule covers mutation verdicts, not identical execution or
diagnostic evidence. A different worker count can change contention, timing,
and worker assignment. A different output limit can change retained diagnostic
text. Fresh baseline success and the current configuration and resource guards
remain prerequisites for the resumed run.

All other limits remain part of the fingerprint. Candidate bounds and analyzer
timeouts can change the discovered set; baseline and mutant timeouts can change
classification; total timeout, memory, process, copy, workspace, and free-space
limits retain their existing compatibility behavior.

## Implementation

`encode_compatibility_limits` omits only `RunLimits.jobs` and
`RunLimits.max_output`. Plan and run configuration serialization still retain
both fields. No database or report schema migration is needed because the
session database stores the fingerprint schema next to each digest.

The existing repository has no Lean corpus for fingerprint byte encoding. This
change does not alter a generated corpus or add a second model.

## TDD evidence

Before the implementation change:

- changing `jobs` or `max_output` produced different core fingerprints;
- a stored schema-5 run was not rejected as an older fingerprint schema;
- the real CLI created a second run when both operational settings changed.

After the change:

- all 18 `resume_policy` tests pass;
- all 26 active `session_handler` tests pass, with one unrelated test ignored;
- the real CLI resumes the same run ID, records the current limits, and emits
  the reused mutant with null termination and output.

Final verification:

- `cargo test --workspace --all-features -- --test-threads=1` exits 0, including
  548 passing CLI unit tests with 9 ignored and all 55 `run_e2e` tests;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits
  0;
- `cargo fmt --all -- --check` and `git diff --check` exit 0.
