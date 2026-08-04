# Issue #167 Fingerprint Properties

## Goal

Protect resume compatibility fingerprints against collection ordering/duplication noise while
proving that every execution-relevant canonical edit changes the fingerprint.

## Independent oracle

The integration test owns a canonical semantic model that does not call `fingerprint`, any
production encoder, or any production normalization helper. It uses explicit numeric tags,
little-endian length prefixes, and `BTreeSet` normalization for the four top-level set-like
collections: sources, targets, operators, and fingerprint inputs. Nested target lines and symbols
are also normalized independently. Ordered argv and all scalar compatibility fields retain their
ordering and flavor in the model.

The model establishes semantic equality or inequality before a production fingerprint assertion.
This prevents production output from serving as its own oracle.

## Generated coverage

Both property blocks use exactly 128 cases and retain default source-parallel failure persistence.
Generated valid inputs cover Unix byte and Windows UTF-16-unit argv flavors, Unicode paths,
32-byte source hashes, normalized targets, known operators, fingerprint-input paths and hashes,
all safety limits, resource mode, and mutation profile.

The invariance property inserts a duplicate into each of the four top-level set-like collections,
applies independent deterministic Fisher–Yates permutations, proves the independent model is
unchanged, and then requires the production fingerprints to match.

The sensitivity property selects one edit from: source hash byte, path component, numeric limit,
argv byte/unit, argv Unix/Windows flavor, operator, source, target, or fingerprint input. It rejects
any model-level no-op before requiring different production fingerprints. The flavor edit uses the
same numeric payload as Unix bytes and Windows units, while the model's explicit flavor tag proves
the two arguments are distinct.

## Production correction

The RED test showed that repeated identical source and target entries changed the fingerprint even
though these collections are semantic sets. Their encoded element vectors now deduplicate after
sorting, matching the existing operator and fingerprint-input behavior. Unique normalized inputs
retain their existing encoding and therefore do not require a schema-version change.

## Verification

```text
cargo test -p hoimin-core --test resume_policy
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
