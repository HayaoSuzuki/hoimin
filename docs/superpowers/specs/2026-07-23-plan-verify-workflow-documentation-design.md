# Plan/Verify Workflow Documentation Design

## Context

Issue #21 asks the user documentation to make several existing CLI behaviors explicit:
`verify` accepts more than one candidate, inherits execution and resource settings from its
plan manifest, and cannot override those settings. This is especially important on macOS,
where best-effort memory enforcement may need to be authorized when the plan is created, and
for large candidate selections that may exceed the default five-minute total timeout.

The README already contains the primary plan/verify walkthrough and the mutation-progress
guidance. Clap field documentation is the source of `hoimin verify --help`, so the same core
constraints can be discoverable at the command line without duplicating the full guide.

## Scope

This change is documentation-only in intent:

- expand the README plan/verify walkthrough;
- improve the `verify` command and `--candidate` help text;
- protect the documented interface with focused assertions in existing CLI tests.

It does not change argument parsing, plan manifests, report schemas, resource enforcement,
timeouts, progress comparison, or exit codes.

Because the change only documents existing behavior, its commits should use `[skip ci]` to
avoid spending CI capacity. Focused local tests still verify the affected documentation and
help contracts before completion.

## Documentation Design

### README workflow

Expand **Plan and verify with an agent** into a complete plan-to-verify workflow:

1. Show a plan command with `--allow-best-effort-memory` and `--total-timeout 15m` before the
   test-command separator. Explain that macOS commonly needs the former because hard memory
   enforcement is unavailable there.
2. Show one verify invocation with two repeated `--candidate` arguments.
3. State that `verify` restores the test argv, execution limits, timeout values, and resource
   policy from `PLAN.json`; it does not accept overrides for those plan-time settings.
4. State that changing the total timeout or resource policy requires generating a new plan.
5. Connect the existing five-minute default to batching advice: either plan with sufficient
   total-timeout headroom or split a large selection across multiple verify invocations.

The text should distinguish the plan-wide settings from verify-only output selection such as
`--format`, so readers do not infer that every verify option is inherited.

### Split-batch reports and progress

Each verify batch writes its own report. Reports are comparable by `hoimin progress` only when
they cover the identical candidate-ID set. Therefore the recommended iterative workflow is:

1. choose stable candidate batches;
2. save each batch's reports separately after every test improvement;
3. run `hoimin progress` oldest-to-newest within one batch history;
4. inspect all batch histories when summarizing overall work.

Reports from different subsets must not be mixed into one progress invocation, and their
scores or saturation states must not be arithmetically combined into a synthetic whole-plan
result. Overall completion is established by accounting for the union of planned candidate
IDs across the latest complete batch reports. If a batch's candidate membership changes, it
starts a new comparable history.

### CLI help

The `verify` subcommand help should say that it verifies one or more planned candidates and
inherits execution/resource settings from the manifest. The `--candidate` option should say
that it is repeatable. Concise help text points users toward the behavioral constraints while
the README remains the canonical workflow explanation.

## Verification

Focused tests should assert:

- parsing repeated `--candidate` values still yields all requested IDs;
- `verify --help` describes manifest inheritance and repeatable candidates;
- the README contains the documented multi-candidate and plan-time resource/timeout example.

Existing help and README contract tests should be extended instead of introducing a separate
test harness. A final formatting and diff check confirms that no runtime behavior changed.

## Files

- `README.md`: workflow, batching, and progress guidance.
- `crates/hoimin-cli/src/cli.rs`: Clap help text only.
- `crates/hoimin-cli/tests/cli_config.rs`: help and repeated-option contract coverage.
- `crates/hoimin-cli/tests/run_e2e.rs`: README example contract coverage if that is where the
  existing README assertion is most naturally extended.
