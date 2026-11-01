# Metrics Accounting Design

## Goal

Reject run metrics whose run-wide and per-worker process counts cannot all be
true at the same time.

## Design

`RunMetrics::validate` returns a public `MetricsValidationError`. Existing
validation failures retain their messages through a general invalid-contract
variant. Cross-field accounting receives dedicated variants for executed work
exceeding discovery, worker process sum overflow, and worker process sum
mismatch.

Validation first checks existing schema, identity, ordering, and duration
rules. It then requires `executed <= discovered` and computes the worker process
sum with `checked_add`. Overflow is reported before comparing the sum with
`executed`.

Both collector finalization and `write_metrics` continue to validate before a
document can be returned or persisted. CLI errors store the validation error's
display text without losing the core typed boundary.

JSON Schema retains nonnegative integer constraints for every expressible
field. Draft 2020-12 cannot compare sibling numeric values without a
nonstandard `$data` extension, so descriptions explicitly state the
cross-field invariants and identify the Rust validator as authoritative.
