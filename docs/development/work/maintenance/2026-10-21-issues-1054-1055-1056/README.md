# Git capture and streaming plan/report inputs

Issues: #1054, #1055 and #1056. Comparison base:
`f55f42c66b21861ae5302c057b4b600ec5d765c1` (merged #1050–#1053).
Checks run on macOS arm64 with two Cargo jobs, sequential Cargo processes,
one shared target directory and a linked existing Python environment.

## Design and resource policy

Git stdout capture has a 64 MiB per-command budget. A command exceeding it fails
explicitly, closes its pipes and retains ownership through termination/reap;
no partial selection reaches a caller. Stderr retains a 64 KiB prefix and a
41-byte marker while draining excess. Explicit capacity growth keeps each Vec
inside its budget. Two 8 KiB read buffers and bounded diagnostic string copies
are additional memory. The same policy applies to whole-repository rename and
unmerged diff inspection. `Legacy` identifies those patch semantics and remains.
There are no spool files or new settings. The deliberate tradeoff is rejecting
otherwise valid individual Git outputs above 64 MiB.

Verify streams the typed current plan once. The schema probe, intentional
successful-probe error and obsolete-shape reparsing are removed. Old/future
versions and obsolete shapes still fail; diagnostics need not prioritize the
version over malformed or obsolete fields. Tests using old shapes now assert
rejection rather than diagnostic ordering. Current schema-first/schema-last,
escaped names, duplicates, unknown fields and trailing input remain covered.

Progress decodes JSON from a buffered reader and JSONL through a physical-line
Read adapter. No whole-document Vec or raw-line Vec is retained. A strict run
header struct lets progress skip opaque configuration fields without serde's
internally tagged intermediate tree. Raw baseline/mutant/summary events convert
once into typed payloads before structural validation and usable-result creation.
The run header and shared core event envelope both require a string `kind` before
enum dispatch. All six core event variants reject object/array/null/number tags.

Verify and progress share an initial-descriptor-length reader. It permits at most
the initially observed bytes, then probes one extra byte to reject append; early
EOF rejects truncation. Progress reuses the same length budget after routing and
rewinding. Complete successful reads, including ignored whitespace, contribute
to the fingerprint. This prevents following an indefinitely appending file,
without imposing review's 64 MiB cap on valid large inputs. It does not provide
an atomic snapshot or detect every in-place rewrite.

Decoded candidate/result payloads, largest-string parser scratch, JSONL sequence
state and one current core RawValue event remain necessary or existing storage.
Progress retains adjacent report pairs. Review history, database behavior and
its existing input cap remain. Plan/report schemas, package floor and formal
proof/model sources are unchanged. No compatibility reader or migration is added.

## Regression and measurement evidence

The tests use bounded fixtures, not sustained benchmarks or mass-generated files.
Measurements count requested live Rust allocation, not RSS, allocator metadata
or OS caches. No general latency improvement is claimed.

- Plan schema diagnostic regression: before removal, three tests failed because
  the version probe still took precedence over closed-shape and duplicate errors.
- Public verify allocation regression: the original implementation allocated a
  manifest-sized raw buffer once; streaming requires zero such allocations before
  source validation. The negative control failed with `left: 1, right: 0`.
- A 6,505,533-byte valid plan fixture (512 candidates with 4 KiB source fragments,
  plus 2 MiB trailing whitespace) used a 4,439,175-byte streaming peak versus
  10,936,516 bytes when retaining the raw input. This is a same-decoder bounded
  allocation comparison, not a production-RSS claim.
- The initial progress heap regression with a 4 MiB ignored configuration field
  failed at 8,408,048 bytes versus a 22,451-byte small-input control. The final gate
  also includes a 4 MiB same-line whitespace suffix; JSON and JSONL must stay
  within 128 KiB of their small controls. Exact final observations are in
  [validation.json](validation.json).
- Temporarily restoring unbounded Git `read_to_end` while preserving the new
  call signature failed all five bounded-capture tests. The final reader covers
  exact boundaries/native bytes, a million-byte unterminated line, NUL-delimited
  targets, invalid UTF-8 stderr flooding and a real stdout-overflow child with
  simultaneous stderr output and owned termination/reap. Rename pairing with a
  large line and existing many-target/unmerged cases are retained.
- A fresh-context review found the run-tag enum's externally tagged object form.
  Its rejection test failed before the string-only boundary fix. Inspection then
  found the same preexisting core-envelope behavior: public progress accepted an
  object-valued baseline tag with exit 0. The all-six-event regression failed
  before the fix; the full core wire suite then passed (145 tests).
- Deterministic tests append between report routing and decoding, check early
  truncation and the one-byte growth probe, hash complete JSON/JSONL including
  whitespace, and reject multiple events on one line or multiline events.

## Verification and review

Final command results and resource observations are recorded in
[validation.json](validation.json). Python's initial sandbox run encountered six
OS-permission failures (uv cache creation and process inspection); the affected
release/resource-guard test modules passed after rerunning with ordinary OS
permissions. Those release tests mock publication operations.

Independent read-only reviews covered Git lifecycle/capture and plan/report
strictness. The string-tag findings were fixed and reviewed again; no remaining
Critical or Important findings were reported. Formatting, lint, fuzz-workspace,
documentation and final Rust verification are recorded separately from negative
controls. No large benchmark, release build or new formal-proof run was performed.
