# Issue #349: canonical normalized-config enum JSON

Issue: https://github.com/tokyogas-tech/hoimin/issues/349

Branch: `fix/issue-349-config-json`, based on main `fdf8150`.
Worktree: `.worktrees/issue-349-config-json`.

## Scope

Issue #349 changes the serialized names of `MutantTimeout` and `CommandArg`
variants embedded in normalized run and plan configuration. New JSON uses
snake_case, matching the other public enums. Readers still accept PascalCase
JSON. The patch leaves report, plan, fingerprint, and session schema versions
unchanged.

## Cause and fix

`MutantTimeout` and `CommandArg` derived `Serialize` and `Deserialize` without
a serde naming policy. Serde therefore exposed the Rust variant names in plan
manifests and in `RunStarted.normalized_config`:

| Rust variant | Previous output | Canonical output |
| --- | --- | --- |
| `MutantTimeout::Auto` | `"Auto"` | `"auto"` |
| `MutantTimeout::Fixed(value)` | `{"Fixed": value}` | `{"fixed": value}` |
| `CommandArg::Unix(value)` | `{"Unix": value}` | `{"unix": value}` |
| `CommandArg::Windows(value)` | `{"Windows": value}` | `{"windows": value}` |

Both enums now use `#[serde(rename_all = "snake_case")]`. Each variant has an
alias for its previous PascalCase spelling. Readers accept both eras, and
writers emit the canonical form.

## Compatibility assessment

Plan manifests contain a serialized `PlanConfig`, so plan writers now use the
canonical spellings. A regression test deserializes both canonical and legacy
`PlanConfig` values to the same typed value. Old manifests continue to load.

Run reports contain a serialized `RunConfig` in
`RunStarted.normalized_config`. A public `OutputEvent::RunStarted` test checks
the exact canonical JSON paths and round-trips the complete event. A separate
legacy event test replaces both affected values with their PascalCase forms
and deserializes the event to the same typed value. Existing schema-v2 and
schema-v3 original goldens retain the PascalCase spellings and continued to
load in the CLI suites. The corresponding current report and event goldens now
use snake_case. The schema-v3 current-golden tests inspect the raw
normalized-config values.

The serialized normalized-config bytes change, but the run fingerprint does
not. `FingerprintInput::from_config` extracts typed fields and
`resume::fingerprint` encodes them with fixed binary tags: Unix arguments
use field 1, Windows arguments use field 2, automatic timeout uses byte 0, and
fixed timeout uses byte 1 plus the duration. The fingerprint schema remains
version 5. No fingerprint path serializes `RunConfig` or `PlanConfig` through
serde.

Sessions store and query the 32-byte `RunFingerprint`; the session code does
not derive an integrity key from normalized-config JSON. Existing compatible
session runs remain discoverable because the fingerprint bytes and schema
version stay the same. The full end-to-end suite, including session resume
tests, passed.

## TDD evidence

The initial focused test run was:

```console
CARGO_TARGET_DIR=/tmp/hoimin-issue-360-target \
  cargo test -p hoimin-core --test config_json
```

Before the production change, 1 test passed and 3 failed. The failures showed
the exact mismatch: serde produced `"Auto"`, `{"Fixed": ...}`, and
`{"Unix": ...}` where the tests required snake_case. The legacy
`RunStarted` test passed against the old implementation. After the minimal
serde annotations, all 4 original tests passed. The final focused file has 7
passing tests after splitting per-contract checks and adding explicit plan
manifest coverage.

The tests cover:

- read/write round trips for both `MutantTimeout` variants;
- read/write round trips for both `CommandArg` variants;
- all four legacy PascalCase aliases;
- canonical and legacy persisted `PlanConfig` values; and
- canonical and legacy `OutputEvent::RunStarted.normalized_config` values.

## Verification

All Rust commands used the isolated target directory
`/tmp/hoimin-issue-360-target`.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0 |
| `cargo test -p hoimin-core --test config_json` | 7 passed |
| `cargo test -p hoimin-core` | Exit 0; core suite passed |
| `cargo test -p hoimin-cli --test report_handler --test progress --test plan` | 118 passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo test --workspace` | Exit 0; workspace suite passed, including 54 `run_e2e` tests |
| `cargo test -p hoimin-core --features contracts` | Exit 0; core contract and integration suites passed |
| `git diff --check` | Exit 0 |

The first CLI attempt ran without the worktree `.venv`; all 38 plan tests
stopped at their shared missing-interpreter precondition. After linking the
main checkout's existing `.venv`, the same command passed. The link was used
for local verification and does not remain in the worktree.

One final workspace run hit the unrelated timing-sensitive
`rollback_contention_marks_root_for_immediate_janitor_recovery` test: the
janitor observed one preserved root instead of one reclaimed root. That test
passed 1/1 in an immediate isolated rerun. A complete workspace rerun then
passed with exit 0. An earlier complete workspace run had also passed.

We did not dispatch the Linux or Windows platform workflows. The production
change has no platform-specific branch; the local macOS run covered both
argument representations as typed JSON values.

Independent review found no blocking issues and confirmed the schema and
fingerprint compatibility assessment. Both the reviewer and coordinator reran
the seven config JSON tests, formatting, and whitespace checks. Changed files
do not overlap #348 or #368.
