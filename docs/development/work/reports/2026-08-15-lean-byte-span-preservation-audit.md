# Mutation byte-span preservation Lean audit

## Verdict

Issue #312 found two same-premise Rust mismatches. `WorkerWorkspace::apply_mutation`
rechecked the manifest hash, byte bounds, and original bytes, but did not recheck
the candidate line and Unicode-scalar column. A directly injected candidate with
the right bytes and a wrong column was therefore written successfully.

The retained regression changes only `column` from 0 to 1 for source
`original\n`, span `[0, 8)`, original `original`, and replacement `mutated!`.
Before the fix the worker became `mutated!\n`; the contract requires rejection
with the worker remaining `original\n`. The production repair calls the existing
shared `validate_candidate` before any seek, truncation, or write and maps its
result into existing `WorkspaceError` variants.

The same write boundary also discarded the stable ID returned by that validator.
A candidate whose descriptor was otherwise valid but whose `id` changed from
the canonical value to `m1_corrupted` was therefore written successfully. The
second retained regression requires rejection with unchanged worker bytes. The
repair compares the returned canonical ID with the transported candidate ID
before writing. No public API or schema changed.

## Audited contract

For the exact immutable source snapshot used to validate a candidate:

- span addition is representable and the interval lies within the source;
- both endpoints are UTF-8 boundaries;
- original length equals span length and the source slice equals original;
- line and Unicode-scalar column identify the byte start;
- the complete identity tuple is schema, source hash, normalized path, span,
  operator, and replacement;
- full transports preserve every `MutationCandidate` field, while the session
  row intentionally preserves only stable ID and status;
- application is exactly prefix plus replacement plus suffix;
- every rejected workspace request leaves bytes unchanged;
- reset restores the original snapshot before a second candidate is applied.

Lean models the complete identity tuple rather than asserting mathematical
injectivity for a finite BLAKE3 digest. Rust fixed cases check that the actual
framed BLAKE3 ID changes for every identity-bearing field and is unchanged by
line, column, and symbol metadata.

## Kernel-checked claims

The imported model and proof modules establish:

- `accepted_span_within_source` and `accepted_span_within_counter`;
- `accepted_boundaries`;
- `accepted_original_matches`;
- `accepted_location_matches`;
- `accepted_identity_matches`;
- `complete_transport_preserves_candidate` and
  `complete_transport_preserves_identity`;
- `session_projection_preserves_identity`;
- identity sensitivity for schema, path, hash, span start/length, operator, and
  replacement;
- `application_is_exact_reference`;
- `application_preserves_prefix` and `application_preserves_suffix`;
- `rejected_application_preserves_source`;
- `reset_makes_second_application_independent`.

The external proof consumer imports only `CandidateSpanModel` and
`CandidateSpanProofs`. Fixed examples, broken variants, JSON generation, and
statistics remain outside that import path.

## Corpus and correspondence

The closed JSONL corpus has 12 rows:

| Mode | Rows | Rust observation |
| --- | ---: | --- |
| `strict` | 3 | real analyzer public plan, public verify, final report fields |
| `internal-fixture` transport | 2 | serde, candidate spool, machine effect, session stable ID |
| `internal-fixture` rejection | 4 | shared validator and real workspace bytes before/after |
| `internal-fixture` reset | 1 | real worker apply/reset/second apply |
| `model-only` | 1 | non-boundary byte slice not representable as Rust `String` original |
| `infrastructure-error` | 1 | harness classification only |

Strict fixtures are parseable Python and cover ASCII, a second-line candidate,
and a candidate whose byte start differs from its Unicode-scalar column because
of a preceding multibyte character. Public plan descriptors and public verify
report candidates match path, span, original, replacement, operator, line,
column, symbol, and stable ID.

The adapter derives UTF-8 boundary and scalar-start vectors directly from each
source and requires exact equality with the corpus arrays. It also rejects
unknown fields, duplicate IDs, crossed modes, stale harness expectations, wrong
maximum, and inconsistent expected bytes. Strict public observations recompute
the expected stable ID independently from the corpus descriptor rather than
comparing two production projections to each other. Model-only and
infrastructure rows are never promoted to public Rust evidence.

## Transport observations

The protocol-shaped candidate serde round trip, plan/report serialization,
newline-delimited candidate spool replay, and `ApplyMutation` serde round trip
all preserve the complete Rust candidate. SQLite session persistence is checked
against its actual schema: lookup returns the same stable mutant ID and status;
the report does not claim that absent span fields are stored in the session row.

## Sensitivity

All twelve issue families are detected independently. Combined families use
both required witnesses: short and long off-by-one spans, overflow and
non-boundary endpoints, and suffix loss and duplication. Other witnesses cover
byte/character offset confusion, byte columns, replacement-length validation,
location from another snapshot, an explicit transport that drops the span,
identity-field omission, apply-before-validation, truncate-before-validation,
and missing reset.

The existing property and focused tests remain the local foundation rather than
being duplicated: `candidate_policy` covers strict validation, overflow, UTF-8,
CRLF, stable framing, and context equivalence; analyzer unit tests cover batch
conversion; workspace tests cover hash/original/span checks, exact rewriting,
read-only files, reset, and filesystem safety. The new oracle tests audit their
composition.

## Production impact

The workspace now reconstructs a `CandidateDescriptor`, invokes the shared
canonical validator after verifying the worker manifest hash, and compares its
returned stable ID with the transported candidate ID before the existing byte
replacement. Location, UTF-8, normalized-path, mutation-shape, identity, and
shared validation rules can no longer be bypassed at this last write boundary.
Existing hash/original/span error codes remain stable.

## Resource measurements

Each Lean command ran alone under a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof module direct compile | 2,427 | 665,056 | 0 / `child_exit` |
| external proof consumer | 551 | 54,560 | 0 / `child_exit` |
| sensitivity | 284 | 3,184 | 0 / `child_exit` |
| fixed cases | 288 | 2,784 | 0 / `child_exit` |
| corpus freshness | 283 | 2,800 | 0 / `child_exit` |

No retained command reached either limit. The largest sample remained 121,376
KiB below the RSS ceiling.

## Exclusions

Python semantic equivalence, applying several mutations to one unreset worker,
candidate ranking, cryptographic collision resistance as a theorem, and
filesystem races owned by existing workspace audits remain out of scope.

## Verification commands

```text
lake build
lake env lean HoiminOracle/CandidateSpanProofs.lean
lake env lean /tmp/hoimin-candidate-span-proof-consumer.lean
lake exe generate_candidate_span -- --cases
lake exe generate_candidate_span -- --sensitivity
lake exe generate_candidate_span -- --check corpus/candidate-span-preservation.jsonl
cargo test -p hoimin-core --test candidate_policy --all-features
cargo test -p hoimin-cli --test lean_candidate_span_oracle --all-features
cargo test -p hoimin-cli --test workspace_handler --all-features
cargo test --workspace --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
