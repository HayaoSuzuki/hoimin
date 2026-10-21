# Rust dependency policy

`deny.toml` and the **Rust dependency policy** jobs in
[dependency-audit.yml](../.github/workflows/dependency-audit.yml) check licenses
and sources with cargo-deny 0.20.2. Run the same commands locally:

```console
cargo deny --manifest-path Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources
cargo deny --manifest-path fuzz/Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources
```

Both lockfiles are checked without updating them. With no target filter, the
graph includes other OS/architecture dependencies, normal/build/dev dependencies
and all features. The first graph includes the math crate, core, pure Python analyzer and CLI; the second
is the separate development-only fuzz harness. The local vendored parser patch
is included in both. These are Cargo graphs, not assertions that every crate
ships in every archive. Python dependencies remain covered by uv audit and are
outside this Rust license policy.

## Dependency updates

Cargo requirements such as `"1.8"` already permit minor and patch updates
through the 1.x series. General 0.x dependencies use explicit ranges such as
`">=0.4, <1"` to permit minor updates as well. Exact versions remain recorded
in the root and fuzz `Cargo.lock` files. Update both workspaces with:

```console
cargo update
cargo update --manifest-path fuzz/Cargo.toml
```

Renovate updates Cargo lockfiles within the declared ranges and performs lockfile
maintenance. Builds and CI use `--locked` to reproduce the committed dependency
graph.

The Ruff crate family stays in the vendored parser's 0.6 series because its AST
types must agree across the parser and analyzer. The vendored package retains
its upstream dependency requirements. `creusot-std` stays pinned to the verifier
in `formal/creusot/toolchain.json`; update the proof environment and artifacts
together when changing that version.

The version ranges do not widen the license exceptions below. A new resolved
version of `contracts` or `libfuzzer-sys` still requires the corresponding source
review and scoped exception update.

## Licenses and sources

Unlisted licenses and unknown registry/git sources fail. The allow-list selects
MIT or Apache-2.0 for dual-licensed dependencies, the LLVM exception where needed,
and the Unicode data licenses. It is based on these dependency graphs rather
than another project's policy. crates.io is the only allowed registry; there
are no git-source exceptions. Local paths are reviewed in repository diffs;
the source check does not prove their origin or safety.

The repository's Elastic-2.0 license is allowed only for `hoimin-budget-math`, `hoimin-core`,
`hoimin-python` and `hoimin-cli` in the 0.13 series and the private `hoimin-fuzz` 0.0.0 harness. The
harness declares the license that already governs repository code. This does
not allow unrelated ELv2 dependencies. `libfuzzer-sys` 0.4.13 has a scoped NCSA
allowance for its bundled libFuzzer. Update these bounded exceptions explicitly
when changing their versions. Private workspace packages are not blanket-skipped.

Warnings fail CI. Unused license exceptions alone are allowed because the same
policy serves two graphs with different roots; ordinary unused licenses and
sources still fail. Do not loosen the global list to resolve a single exception.
Record the crate, version range, upstream license evidence, reason and review
condition next to an exception. cargo-deny success does not fulfill attribution,
redistribution or other license obligations; see [licensing](licensing.md).
Existing cargo-audit remains the vulnerability check. Duplicate crate versions
are not banned here.

## Build script review

[The current inventory](dependency-build-scripts.md) lists `custom-build` targets
from locked Cargo metadata for both graphs. It includes platform-specific crates
and development dependencies. On a dependency/feature/toolchain update, regenerate
the list using `cargo metadata --locked --all-features --format-version 1`
(with `--manifest-path fuzz/Cargo.toml` for fuzz), select packages whose target
`kind` contains `custom-build`, sort by name/version and review the diff.
Update the inventory in the same PR. Metadata collection does not execute these
scripts. This policy has no build-script allow-list or custom analyzer.

Review additions/removals and why code execution during build is needed. This
manual inventory does not certify the contents of build scripts, proc macros or
dependencies. A dependency version change still needs upstream review even when
the list of script names is unchanged. Renovate updates use the same checks and
review procedure; SBOM records remain separate release evidence.

`contracts` 0.6.11 has an exact-version MPL-2.0 exception for optional budget
postconditions and scalar snapshots. Its proc-macro source and return rewriting
were reviewed before adoption; it has no custom build script. This exception
does not admit other MPL dependencies. See [licensing](licensing.md) for the
preserved notice and custom-build distribution guidance.

`creusot-std` and `creusot-std-proc` 0.14.0 have exact-version
LGPL-2.1-or-later exceptions for verification. `cfg(creusot)` selects them;
ordinary all-features builds exclude them, while unfiltered dependency audits
include them. Neither package has a custom build script; the latter executes
as a proc macro during verification. Review its generated artifacts separately
from the shipping dependency graph. [Licensing](licensing.md#creusot-verification-dependencies)
records the upstream license, preserved notice, distribution boundary and
upgrade conditions. The global allow-list is unchanged.
