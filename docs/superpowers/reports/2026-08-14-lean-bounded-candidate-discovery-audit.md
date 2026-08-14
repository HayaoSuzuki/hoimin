# Bounded candidate discovery Lean audit

## Verdict

Issue #309 found no same-premise Rust mismatch. The current analyzer returns
the expected bounded prefix, propagates producer overflow, shares one limit
across ordered targets, finishes a truncated non-final spool, assigns
contiguous global sequences, and publishes an incomplete plan with exit code
4. This branch adds formal definitions, proofs, sensitivity witnesses, a
Lean-owned corpus, and Rust correspondence tests. It does not change production
Rust behavior.

## Audited contract

For a successful discovery and limit `k`, `reference` filters ineligible
candidates, orders candidates by the modeled production key, and removes
duplicate identities. `bounded` returns `reference.take k` and marks the result
truncated exactly when `k < reference.length`.

Ordered target discovery passes the unused portion of the same global limit to
each target. A target that observes overflow ends discovery and finishes the
spool. A complete final target also finishes the spool. Stored sequence values
are `1` through the retained count.

## Kernel-checked claims

Lean 4 checks these theorems for arbitrary natural-number inputs:

- `bounded_candidates_eq_reference_take`;
- `bounded_length_le_limit`;
- `bounded_truncated_iff`;
- `bounded_zero`;
- `producerWindow_length_le`;
- `producerWindow_preserves_limit_prefix`;
- `merge_candidates_eq_window_reference_take`;
- `merge_candidates_length_le`;
- `merge_truncated_iff_window_or_producer_overflow`;
- `sequences_contiguous`;
- `target_count_le_limit`;
- `truncation_stops_later_targets`.

`three_producer_merge_matches_unbounded_reference` closes the concrete
three-producer correspondence case used by Rust. The generic theorems prove
each producer retains at most `k+1`, preserves its first `k`, and that the merge
uses the ordered reference of those windows with exact local-overflow
propagation. They do not claim a verified implementation of Rust's binary heap.

The model treats candidate identity, eligibility, producer, order key, and
emission index as explicit inputs. It does not formalize Ruff parsing or the
binary heap implementation. Rust adapters connect those abstractions to the
real analyzer.

## Corpus and correspondence

Lean owns eight JSONL rows:

| Mode | Count | Evidence |
| --- | ---: | --- |
| `strict` | 1 | real two-file `hoimin plan`: complete retained descriptor, truncation flag, candidate-limit diagnostic, exit 4 |
| `internal-fixture` | 6 | `CandidatePrefix`, three real producers, eligibility, ordered targets, terminal spool, zero limit |
| `model-only` | 1 | `usize::MAX` natural-number boundary |

The Rust parser requires schema 1, the exact eight case IDs, the assigned mode
and scenario for each ID, known producer names, unique IDs, contiguous expected
sequences, and coherent target fields. Tests reject unknown fields, duplicated
rows, and crossed strict/model-only modes.

Internal observations cover:

- out-of-order insertion with a duplicate identity;
- a real source that activates token, AST, and type-annotation producers and
  matches Lean-owned paths, spans, originals, replacements, operators, lines,
  and columns;
- the exact full-output prefix and the separate producer-window bound;
- focused eligibility filtering and zero-limit overflow from their corpus rows;
- two complete targets with one global sequence space;
- a truncated non-final target that returns a finished spool and prevents the
  caller from issuing the later request.

## Refutation sensitivity

The executable detected all nine broken families:

| Broken family | Result |
| --- | --- |
| producer retains `k` instead of `k+1` | detected |
| emission order replaces production order | detected |
| capacity applies before eligibility | detected |
| deduplication occurs after truncation | detected |
| producer overflow disappears at merged length `k` | detected |
| producer-order concatenation discards a globally earlier candidate | detected |
| each target receives a fresh limit | detected |
| discovery continues after terminal truncation | detected |
| public incomplete projection omits its boundary | detected |

The first sensitivity run expected target discovery to stop as soon as the
first target filled the limit exactly. The implementation cannot know that the
reference has more candidates until it probes the next target with zero
remaining capacity. The model now records two target reads and still rejects a
capacity-reset implementation because it would retain two candidates. This was
a model-premise correction, not a Rust defect.

## Resource measurements

Each retained Lean command ran alone under a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling. The sandboxed monitor
attempt returned exit 126 with `monitor_error` because it could not inspect the
process table. Repeating the commands in the approved monitoring environment
kept the limits unchanged.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof build, one Lake job | 307 | 2,800 | 0 / `child_exit` |
| external proof consumer | 590 | 54,176 | 0 / `child_exit` |
| sensitivity | 300 | 3,072 | 0 / `child_exit` |
| fixed cases | 562 | 50,032 | 0 / `child_exit` |
| corpus freshness | 282 | 3,072 | 0 / `child_exit` |

No retained command reached the time or RSS limit. The 54,176 KiB consumer
sample remains 732,256 KiB below the ceiling.

## Verification

The final gate uses:

```text
lake build
lake env lean /tmp/hoimin-bounded-discovery-proof-consumer.lean
lake exe generate_bounded_candidate_discovery -- --cases
lake exe generate_bounded_candidate_discovery -- --sensitivity
lake exe generate_bounded_candidate_discovery -- --check corpus/bounded-candidate-discovery.jsonl
cargo test -p hoimin-cli --test lean_bounded_candidate_discovery_oracle
cargo test --workspace --all-features
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Lean built 65 jobs. The corpus adapter passed four tests. The full Rust run and
strict clippy gate passed after replacing two test-only narrowing casts with
checked conversions.

## Exclusions

This audit does not prove parser completeness, candidate replacement validity,
filesystem safety, cancellation timing, plan ranking, or scheduler execution.
Existing tests cover those contracts. The `usize::MAX` row stays model-only
because a corpus `u64` cannot establish behavior on every Rust pointer width.
