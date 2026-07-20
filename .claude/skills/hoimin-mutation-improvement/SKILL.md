---
name: hoimin-mutation-improvement
description: Use when iterating on hoimin mutation-test survivors and test improvements until the current target's progress is saturated or complete.
---

# Improve Python Tests with hoimin

Iterate on survivors only while ordered hoimin reports show meaningful progress. `saturated` is a stopping decision, not proof that every remaining mutant is equivalent or that testing is complete.

## Set up a comparable series

1. Inspect the production-code selector, test argv, profile, operators, and limits. Keep them unchanged for this loop.
2. Create a temporary directory outside the repository. Save every complete `hoimin run --format json` result there in oldest-to-newest order.
3. Run the normal test command before each mutation run. Do not use a report whose baseline failed or whose run is incomplete in the progress history.
4. Default to `--profile focused`. Do not use persistent reports or `--session` / `--resume` unless the user requests them.

The first complete report establishes the baseline. If it has no survivor, report success immediately. Otherwise select one useful survivor, add or strengthen a behavioral test for its contract, and rerun normal tests before collecting the next report.

## Decide after each complete report

After two or more usable reports, pass all of them in oldest-to-newest order:

```console
hoimin progress --format json report-001.json report-002.json
```

Read `latest.state` from the JSON result. Decide from this field, **終了コードではなく**.

| `latest.state` | Action |
| --- | --- |
| `improving` | Progress reset the stall count. Select one remaining survivor and continue. |
| `stalled` | Try one more focused behavioral-test improvement; it has not yet reached patience. |
| `saturated` | Stop. The default is three consecutive comparable stalls; report residual survivors, attempted contracts, and this stop reason. |
| `regressing` | Stop and diagnose the previous test change or target drift. Do not hide regression by adding another test. |
| `indeterminate` | Repair the baseline, incomplete run, or changed selection and rebuild a comparable history. Do not count it as a stall. |

If a complete report has zero survivors, finish without waiting for `saturated`. Never change production code only to kill a survivor, and never continue from a failed baseline, incomplete report, or cancellation.

## Final report

State the production target, test argv, report count, final `latest.state`, tests added or strengthened, and unresolved survivors with their rationale. Keep temporary reports out of the repository unless the user asks to retain them.
