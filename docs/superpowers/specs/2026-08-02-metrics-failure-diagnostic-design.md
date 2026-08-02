# Metrics failure diagnostic design

## Problem

When `--metrics` is requested, metrics finalization and warning emission are
currently gated on a successful run result. A hard infrastructure or transition
error therefore leaves no metrics file and suppresses every queued metrics
warning without explaining the missing sidecar.

## Design

Choose the explicit-diagnostic option rather than serializing partial metrics.
The current metrics schema has no completeness marker, so writing a normal-looking
partial document would let dashboards mistake incomplete observations for a
complete run.

Create the run identifier before entering the fallible run future so diagnostics
also have an identifier when the future returns an error. Move the metrics
finalization decision into a small helper:

- with a run failure, do not write the sidecar and queue a `metrics.incomplete`
  warning that includes the run failure;
- with a successful run, retain the current finish-and-write behavior and
  `metrics.state`/`metrics.write` warnings;
- without `--metrics`, perform no metrics finalization.

After that decision, emit all queued metrics warnings whenever `--metrics` was
requested, regardless of run success. Metrics remain observational: neither a
write failure nor this diagnostic changes the run result.

## Tests

Unit tests cover the failed-run decision: no file is written and an explicit
`metrics.incomplete` warning is queued while pre-existing warnings are retained.
Existing end-to-end tests continue to cover successful sidecar creation and
warning emission for write failures.
