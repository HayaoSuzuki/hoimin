# Issue 475: Normalize explicit selector groups once

## Problem and decision

`resolve_explicit` normalizes a target's accumulated line ranges after every `--line` and sorts/deduplicates symbols after every `--symbol`. Sparse ranges therefore repeatedly sort and copy prefixes of lengths 1 through N.

Collect valid line ranges and symbols in the existing per-path `TargetSlice`, then normalize each nonempty line and symbol vector once after all selectors are resolved. Invalid line ranges still fail at their original position, and path lookup still occurs in selector order. The final pass runs before `into_values` and the existing postcondition check, preserving target order, inclusive adjacent-range merging, `u32::MAX` saturation, duplicate elimination, and the current interaction where a line selector narrows a whole-file selection on the same path.

The same deferred operation applies to symbols because it has identical repeated-prefix behavior. No public API, selector meaning, error message, or serialized representation changes.

## Verification and scope

A test-only observer in the real resolver counts line and symbol normalization calls. Tests cover sparse, overlapping, adjacent, duplicate, reversed-order, maximum-boundary, multiple-file, invalid, file/source, and repeated-symbol inputs. An ignored release measurement records N/2N/4N medians without a time threshold.

This change does not optimize discovered-file lookup (#474), filesystem discovery (#453), changed-range intersection, or ranking's `LineSelectionIndex`.
