---
type: Playbook
title: CLI reference generation and synchronization
description: Generated Rust command reference scope, handwritten guidance and synchronization
  checks.
status: draft
catalog_revision: 1e60e54813dad14f124635496ed773b299bb4b90
sources:
- id: generator
  resource: ../../crates/hoimin-cli/examples/generate_cli_reference.rs
  revision: 4f60a2440fb42ffd7ee54fe8694323b82fdd777f
  working_tree: clean
  sha256: 943ce9254bdbbf69ccb9ee1289d64a216c71dbb5b25251e4da3d1206ca9732c8
- id: development
  resource: ../development.md
  revision: 4f60a2440fb42ffd7ee54fe8694323b82fdd777f
  working_tree: modified
  sha256: d0114ecb39129c82bedc9ac1bf727aae49a786170a9b60dbe3d40408b6de14db
---

# Generated and handwritten documentation

The [CLI reference](../cli-reference.md) is generated from `root_command()`, which is used for parsing and completion.
It recursively visits public commands and records long help, defaults, enumerated values, argument counts, repetition, required arguments, conflicts, and required groups.
Hidden items are omitted; fixed terminal width and color settings keep dates and absolute paths out of the output.[^generator]

Conditional requirements and numeric parser ranges are not generally available through Clap's stable public API.
Conditions documented in help are generated, but the output is not a complete specification of runtime validation.
The [usage guide](../usage.md) explains examples, OS-specific resource limits, and plan/verify inheritance contracts.[^generator][^development]

# Regeneration and validation

Follow the [generated CLI reference procedure](../development.md#generated-cli-reference)
after changing public command definitions or help. That guide owns the generation,
freshness-check and test commands; commit the generated Markdown on the same
implementation branch.[^development]

`--check` does not rewrite the file and fails on missing content or differences.
Windows CRLF checkouts are accepted; generated output uses UTF-8 and LF.
See the [CI contract](ci-validation.md) for change selection and required checks.[^generator][^development]

# Changes requiring rechecking

When changing Clap, command definitions, generation, or CI path classification, recheck hidden-item exclusion, determinism, and reference synchronization.
This procedure does not cover site builds or JSON schema generation.[^development]

[^generator]: [Generator and tests](../../crates/hoimin-cli/examples/generate_cli_reference.rs).
[^development]: [Development guide](../development.md#generated-cli-reference).
