# Human progress output current-comparison design

## Problem

Human progress output takes `state` from the latest input transition, but takes
statistics from the last successfully constructed comparison. When a usable
comparison is followed by an unusable input pair, the state becomes
`indeterminate` while score and count lines still describe the older pair.

## Design

Pass the ordered input reports to the human renderer. The latest decision has a
corresponding comparison exactly when the final adjacent input pair is usable
on both sides. In that case, render the final comparison's score and count
lines as today. Otherwise omit all comparison-only statistics and render only
the latest decision fields: state, stalls, patience, and saturation.

This keeps `ProgressResult` and JSON output unchanged. Omitting unavailable
human statistics is preferred to printing zeroes, which could themselves be
mistaken for measurements of the indeterminate transition.

## Tests

An end-to-end human-output regression will use three reports: a usable pair
with a real improvement followed by an incomplete report. It must show
`state: indeterminate` without the earlier score, delta, or comparison counts.
Existing tests continue to prove that a usable final pair includes all current
comparison fields.
