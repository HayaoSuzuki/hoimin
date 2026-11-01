# Lean audit design: mutation byte-span preservation

## Decision

Model one validated candidate as source bytes, an explicit set of UTF-8 byte
boundaries, a source hash token, and the complete candidate descriptor. Lean
owns canonical validation, identity-field projection, lossless transport,
session-ID projection, application, and reset semantics. Rust owns UTF-8
decoding, BLAKE3, serde, filesystem I/O, analyzer production, and public CLI
behavior.

The durable correspondence premise begins with the exact immutable source bytes
used by `CandidateValidationContext`. A strict analyzer case must obtain its
candidate from a real producer. Internal malformed cases may inject a descriptor
at an owned validation or workspace seam, but may not be described as public
analyzer behavior.

## Pure model

Represent bytes as naturals and boundaries as a predicate `Nat → Bool` supplied
with the fixture. A descriptor contains path, start, length, original,
replacement, operator, line, Unicode-scalar column, source-hash token, stable-ID
token, sequence, and non-identity symbol metadata.

Canonical validation checks, in production order where relevant:

- the hash token matches the immutable source snapshot;
- `start + length` is representable and within the source;
- both endpoints are UTF-8 boundaries;
- `original.length = length` and the source slice equals `original`;
- line and scalar column recompute to the declared location;
- the operator is nonempty and replacement differs from original;
- the stable identity token corresponds to the complete identity tuple.

Application is defined only after validation and returns
`source.take start ++ replacement ++ source.drop (start + length)`. A rejected
application returns the original bytes. Reset always restores the original
snapshot before another candidate is validated and applied.

## Identity claim

Lean proves preservation and sensitivity of the complete identity tuple:
schema, source hash, normalized path, span start and length, operator, and
replacement. It does not claim that a finite BLAKE3 digest is mathematically
injective. Rust fixed cases verify the production framed hash changes for each
identity-bearing field and remains fixed when only symbol, line, column, or
sequence transport metadata changes.

## Transport projections

The model distinguishes what each boundary can preserve:

| Boundary | Preserved observation |
| --- | --- |
| analyzer conversion | complete candidate plus computed ID |
| protocol / plan / spool / machine effect | complete `MutationCandidate` |
| session result row | stable candidate ID only |
| workspace request | complete candidate and applied bytes |

Rust uses serde round trips for protocol-shaped, plan, spool, and machine-effect
fixtures. Session correspondence compares the ID actually persisted and loaded;
it does not claim that the session schema stores absent span fields.

## Correspondence modes

| Case | Mode | Rust boundary |
| --- | --- | --- |
| ASCII, multiline, and multibyte candidates | `strict` | public plan descriptor; selected cases also run publicly |
| analyzer conversion and complete transport round trips | `internal-fixture` | owned analyzer/core/store seams |
| stale hash/original, overflow, non-boundary, bad location | `internal-fixture` | candidate validation and workspace request; bytes compared before/after |
| reset then second candidate | `internal-fixture` | real `WorkerWorkspace` |
| arbitrary invalid byte strings | `model-only` unless passed to owned validator |
| fixture or I/O failure | `infrastructure-error` | harness diagnostic only |

Every corpus row has one mode. Model-only and infrastructure rows are excluded
from claims about public analyzer output.

## Sensitivity and bounded execution

Use separate broken functions and minimized witnesses for all twelve issue
families: byte/character offset confusion, byte column, off-by-one-short,
off-by-one-long, replacement-length validation, overflow/non-boundary,
different location snapshot, field loss in transport, identity-field omission,
apply-before-validation, truncate-before-validation, suffix loss/duplication,
and missing reset. The overflow/non-boundary family may contain two named
witnesses while retaining one issue-family total.

Fixed cases cover ASCII, multiline, multibyte UTF-8, zero-length-adjacent
boundaries where valid, changed replacement length, and every rejection class.
The generated JSONL is closed by exact ID/mode/scenario mappings and
scenario-specific unused-field checks.

## Rust repair policy

First replay identical Lean premises against existing Rust boundaries. If a
same-premise mismatch appears, retain its minimized corpus row as a failing
regression, then make the smallest production correction. Workspace rejection
tests must read the target after every error and prove it is unchanged.

## Exclusions

Python semantic equivalence, simultaneous mutations, selection/ranking,
cryptographic collision resistance as a theorem, filesystem races already owned
by workspace audits, and fields absent from the session schema are out of scope.

