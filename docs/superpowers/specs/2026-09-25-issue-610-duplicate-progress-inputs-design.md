# Issue 610: diagnose repeated progress inputs

## Contract

Add stderr warnings for repeated input paths and byte-identical copies. Each warning identifies the current and earlier input (one-based position and path) and the evidence: same path or identical bytes. Emit at most one duplicate warning per repeated input, in input order, after existing warning groups. Defer all warnings until every input validates, matching the existing all-or-error rendering boundary.

Do not remove inputs, alter comparisons, reset stalls differently, change exit codes, add strict mode, or change the JSON schema. Unusable reports remain adjacency barriers. Identical results from independent runs do not imply duplicate inputs. Run IDs alone are not identities; resumed reports with changed bytes remain distinct. Same-path reuse is an artifact warning, even if the file changes during a caller's execution.

The first version compares raw bytes. JSON versus JSONL, differing whitespace, and equivalent reserialization are not detected as duplicates unless the exact same path is repeated. Document this limitation. No guarantee is made about concurrent filesystem mutation between reading and duplicate confirmation.

## Implementation

Keep the public read_report API and behavior. Extract its parser to accept a buffered reader. Add an internal reader that computes a BLAKE3 fingerprint while the same bytes are consumed and validated. The CLI uses this reader; public callers of read_report do not pay for hashing. JSONL stays streaming, and no additional report bodies are retained.

A duplicate tracker stores first positions by path and candidate positions by digest. Digest matches are only a filter: reopen regular-file candidates and compare their bytes in fixed-size buffers before claiming identical bytes. This preserves equality even with digest collisions. Do not reopen nonregular files. Optional confirmation failures suppress that warning, preserving the successful read/validation result. Exact repeated paths do not need rereading. Store only compact duplicate evidence with input dispositions, without exposing it in JSON.

Alternatives: run-ID matching confuses resume with exact copies; normalized full-report matching would require a cross-format contract and more state; rereading/hashing every file separately would add avoidable I/O. The chosen single-pass fingerprint plus exact candidate confirmation bounds retained memory while satisfying the stated same-path/copy acceptance conditions.

## Validation

Test same path four times, copied bytes, both output formats, changed run IDs with equal results, same run ID with changed completion/result/metadata, repeated inputs around unusable barriers, and malformed later inputs. Include exact collision-filter rejection and byte comparisons across buffer boundaries. Keep existing history and JSONL heap gates, with expected duplicate warnings replacing their old empty-stderr assertion. Existing Lean progress oracle consumers validate unchanged decisions; formal models and corpus need no change.

## Design self-reviews

1. Identity: a digest alone is not exact equality and a run ID is reused on resume. Require exact streamed comparison after the digest filter; explicitly bound detection to bytes/path.
2. Resource use: retaining complete files would undo streaming/history fixes. Hash the existing read, retain only digest/index/path, and compare candidate files with fixed buffers. Nonregular files must never be reopened for optional detection.
3. Output/error compatibility: eager warnings would leak diagnostics before a later parse failure. Store evidence and append duplicate warnings at the existing deferred render boundary; preserve old warning order and every input position.
