# Agent Mutation Skills Plan-First Design

## Goal

Update the repository-local mutation-testing skills so AI agents use a bounded, auditable
`plan` then `verify` workflow. Keep the `.agents` and `.claude` skill copies byte-identical.

## Scope

Update these mirrored skills:

- `hoimin-mutation-testing` — discover candidates and perform an initial selected verification.
- `hoimin-mutation-improvement` — improve tests against selected survivors and reverify them.

No hoimin executable behavior, report schema, or CI configuration changes are included.

## Workflow

1. Run the normal test command before planning.
2. Create `PLAN.json` in a temporary directory outside the repository with `hoimin plan`.
   Use the same root, selector, profile, operators, limits, and test argv for all verifies that
   consume that plan.
3. Supply `--fingerprint-include` only for root-relative configuration or fixture files that
   affect test behavior but are not represented by the selected production sources. Use
   `--include` separately only when the normal worker-copy policy would omit a file needed by the
   tests.
4. Read candidate IDs from `PLAN.json`; verify only explicitly selected IDs with
   `hoimin verify PLAN.json --candidate <ID>`.
5. For a surviving candidate, add the smallest behavioral test that distinguishes it from the
   original behavior. Rerun normal tests, then verify the same candidate again.

`plan` exit 4 means the manifest is a partial candidate set; IDs actually present in it remain
eligible for verification. Exit 2 produces no usable manifest. `verify` must be treated as stale
and replanned when it rejects changed source or fingerprint input, or when the target selector,
operators, profile, limits, or test argv must change.

## Progress and Stopping

Use `hoimin progress` only for reports covering the same candidate-ID set. Do not infer
improvement or saturation by comparing arbitrary partial `verify` reports. Report each selected
candidate's result and separately identify unverified candidates, including candidates omitted by
a truncated plan.

## Consistency and Validation

The four checked-in skill files remain identical in matching `.agents` and `.claude` paths. The
implementation validates this with file comparisons and checks the Markdown diff. The skill-only
commit message includes `[skip ci]`.
