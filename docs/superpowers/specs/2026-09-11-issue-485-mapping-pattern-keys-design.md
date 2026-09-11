# Issue 485: Preserve unique literal keys in mapping-pattern mutations

Issue: https://github.com/tokyogas-tech/hoimin/issues/485

## Failure and contract

A mapping pattern requires unique literal keys. Flipping True/False or a complex literal's binary separator can create a duplicate, even though Ruff parsing and ast.parse accept the resulting source. CPython compile then raises SyntaxError, and the mutation is not a valid alternative program. Exclude only candidate edits that introduce a literal-key collision within the same mapping pattern. Preserve valid single-key mutations, noncolliding multi-key mutations, nested/adjacent pattern independence and ordinary dictionary-expression behavior.

The [Python mapping-pattern reference](https://docs.python.org/3/reference/compound_stmts.html#mapping-patterns) distinguishes duplicate literal keys from runtime equality of named keys. The pinned [CPython3.14 code generator](https://raw.githubusercontent.com/python/cpython/v3.14.0/Python/codegen.c), codegen_pattern_mapping_key, uses a set of folded constant values. Model that compile-time literal equality; do not attempt to execute attribute lookups or infer arbitrary runtime key values.

## Candidate-local collision analysis

Use the AST's mapping-pattern context and candidate edit to determine the replacement key and compare it with sibling literal keys. Keep the collision boundary at one mapping, restoring outer context for nested mappings and visiting value subpatterns normally. Do not suppress every boolean or binary operation in a pattern, and do not confuse pattern keys with ordinary dict keys. Keep existing candidate ordering, IDs, byte spans and selection behavior. Avoid rescanning every sibling for every candidate: build a per-mapping literal-key index or another justified bound so a wide mapping does not introduce quadratic comparison work. Any numeric normalization belongs in that shared pass.

Compare Python numeric values rather than source text or Ruff structural equality. True equals1 and False equals0; signed floating zero also compares equal, and real numeric values can equal a complex value with zero imaginary part. Ruff ComparableLiteral separates Bool from Number, and ComparableNumber compares float bits and integer representations, so it does not implement this contract. Avoid epsilon comparisons or indiscriminate integer-to-f64 conversion for equality.

Account for the actual mutation domain before adding a general arithmetic subsystem: boolean replacements are0/1; complex separator flips change the imaginary sign, and a zero-imaginary sign flip preserves the numeric key. Any bounded simplification must be proved from that domain and retain valid cases, not silently label an unsupported key as noncolliding. If exact constant handling needs a numeric dependency, prefer a justified maintained representation over improvised approximate arithmetic; send the smallest sound design choice before adding it.

Complex constant construction itself follows Python's floating conversion, including rounding of an integer real part. CPython3.14.7 compile-only probes show that9007199254740993+1j and9007199254740992-1j are distinct valid keys, but flipping the first separator creates a duplicate after rounding. The hex real literal0x20000000000001 behaves the same. Infinite float literals such as1e999 are accepted in complex keys and must be treated consistently. A400digit integer real part already fails CPython constant folding and is not valid-input acceptance evidence. Preserve radix/underscore and exact integer distinctions where relevant; a blind cast of every integer key would be unsound.

## Verification

Capture public plan RED and apply each selected candidate to temporary source, then call CPython compile rather than only ast.parse. Include the reported boolean pair and conjugate complex pair, both key positions, negative real parts, cross-numeric boolean equality, signed zero, imaginary zero, large integer/float boundaries, radix forms and infinity. Confirm original fixtures compile before using mutant failures as evidence.

Pair every exclusion with valid candidates: single key, distinct siblings, nested/adjacent mappings, value subpatterns, ordinary dict expressions and real noncolliding complex/boolean edits. Assert actual candidate IDs/spans/operators and compile every retained candidate in the bounded matrix. Run the reported false-kill case through the actual CLI with a passing baseline; rejected invalid edits must not become killed mutants. Preserve original bytes throughout.

This branch starts4adf809 and does not include the separate #468 pattern-unary correction. Scope public test operator selections to boolean and relevant binary mutations so that this issue does not silently absorb unrelated unary grammar changes. Do not claim that all historical malformed pattern candidates are resolved by this patch.

A small Lean equality/collision model may help only if its numeric representation and edit correspondence are explicit. An abstract set-uniqueness theorem alone cannot establish CPython numeric folding. Actual CPython compilation and Rust public candidate observations are required. Any Lean run uses the existing30second/2048MiB/250ms serial guard, -j1 and -DElab.async=false.

Run focused analyzer/public compilation tests, full workspace with all features, fmt and Clippy for all targets/features. No Python loader or runtime compiler dependency in production, broad pattern exclusions or unrelated resolver refactor.

## Design self-review

1. Read the mapping-pattern contract and CPython folded-constant set check. Separated compile-time literal duplicates from runtime attribute equality, and parsing from compilation.
2. Inspected Ruff numeric representation and structural comparator. Identified bool/int/float/complex equality, signed zero and large-real rounding; checked the finite mutation domain before choosing arithmetic machinery.
3. Require original compilation, real retained candidates and per-mapping restoration controls. Independent base excludes the separate unary fix; bounded native compilation establishes correspondence that an abstract uniqueness proof cannot supply.

## Selected numeric model and conversion

Use the actual edit domain to avoid general Python-number equality. Boolean replacements can only be0 or1, so index those values across bool, exact integer, float and zero-imaginary complex keys. Parser-created Ruff Int values use Big only after u64 overflow; as_u64 therefore suffices for exact integer0/1 recognition. Do not cast an ordinary integer key to f64.

A separator flip negates a complex key's imaginary component. If that component is zero, the edit preserves numeric equality and cannot introduce a duplicate into an originally valid mapping. Keep those edits. If it is nonzero, the changed value can only collide with a complex value of the same real/imaginary components; no real-only sibling can equal it. Normalize signed zero in hashed components. Build the numeric identity index once per mapping, then look up changed identities instead of rescanning siblings.

Complex construction must perform Python's integer-real conversion. Use maintained num-bigint BigUint parsing and num-traits ToPrimitive for that operation only. The selected lockfile resolves num-bigint0.4.8 and num-integer0.1.47, reusing num-traits0.2.19; the CLI declares the needed dependencies. Inspected the pinned conversion source: retained low-bit information makes the subsequent nearest-ties-to-even conversion correct, and the crate declares MSRV1.60. See [conversion implementation](https://github.com/rust-num/num-bigint/blob/num-bigint-0.4.8/src/biguint/convert.rs) and [release history](https://github.com/rust-num/num-bigint/blob/main/RELEASES.md).

The controller chose this dependency over a new top-significand/round/sticky-bit implementation. Cost: two added dependency packages and big-integer parsing allocations for complex real literals outside the small representation, in exchange for avoiding bespoke rounding maintenance. This is a numerical implementation decision, with no acceptance-test or semantic waiver. Reject an overflowing integer-real conversion as unsupported original constant folding; literal floating infinity remains a different, valid case. Tests must establish the distinction with CPython. Before BigUint parsing, reject significant digit counts that cannot yield a finite f64: decimal greater than309, binary greater than1024, octal greater than342 and hexadecimal greater than256, after ignoring leading zeros and underscores. These upper bounds retain every finite conversion; library conversion still decides rounding and overflow at the boundary. This avoids unnecessary big-integer arithmetic on huge, already non-foldable original real literals.

No new Lean model is planned: the bounded domain argument is explicit, while independent CPython compilation supplies the required numeric-folding correspondence. An abstract uniqueness theorem would not validate the parser-to-number conversion.
