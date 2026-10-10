# Issue #754 dependency policy

Base: `79b6bde29b05e9904c8efff59e02d0c45377d7f7`.
Branch: `ci/issue-754-dependency-policy`.

## Design and plan

1. Inventory locked main/fuzz metadata, all features and all targets.
2. Add an explicit license/source policy with bounded repository exceptions.
3. Add pinned cargo-deny jobs alongside, not replacing, vulnerability audits.
4. Record build script inventory for manual diff review; no analyzer or new API.
5. Check actual graphs, representative rejection cases, workflow lint and OKF.

## Design self-reviews

1. Both graphs include dev/build and other-platform crates, not just Linux shipping dependencies.
2. ELv2 applies only to bounded repository crates, not arbitrary third parties.
3. Existing vulnerability audits remain authoritative; no duplicate advisories or bans.
4. Build script review is manual and explicitly cannot prove executable code safe.
5. License check success does not discharge redistribution obligations; no release contract changes.

## Plan self-reviews

1. Use locked metadata, never update lockfiles to make the policy pass.
2. Check source and license failures using temporary isolated inputs/configuration.
3. Share one policy between graphs; explain unused scoped exceptions explicitly.
4. Reuse existing tool cache conventions and pinned installation, read-only permissions.
5. Record inventory, policy, CI links and sourced OKF concept in this branch.

## Implementation self-reviews

1. All external sources except crates.io are denied; local vendored paths require diff review.
2. NCSA allowance is bounded to libfuzzer-sys 0.4.13 and repository exceptions to current series.
3. Private fuzz harness declares its existing repository license; no blanket private exemption.
4. Warnings fail except intentionally unused cross-workspace exceptions; no duplicate-version ban.
5. Jobs use all features and no target filter; existing Python and cargo-audit jobs remain intact.

## Verification self-reviews

1. Actual main and fuzz graph success must precede claims of enforcement.
2. Forbidden-license and unknown-source cases must fail for the intended reason.
3. Strict workflow tools check syntax, injection, permissions and shell behavior.
4. Check OKF source hashes, local links, whitespace and unchanged lockfiles.
5. Monitor disk, clean build artifacts and obtain independent review before PR.

## Results

Both real graphs passed cargo-deny 0.20.2 licenses/sources with warnings denied.
A temporary GPL-3.0-only package was rejected; a temporary policy excluding
crates.io rejected the current registry dependencies. Neither test ran dependency
build scripts. Initial checks exposed the cargo-deny global option placement and
Windows output decoding; both verification commands were corrected and rerun.
Strict actionlint/ShellCheck/zizmor passed (the existing Python-shell evaluation
warning remains a documented tool limitation). OKF 36 pages, local links and
current-base hashes passed. Lockfiles are unchanged; whitespace passed.
`cargo clean` reclaimed 669.9 MiB, leaving 176.5 GiB free.
Independent review found no substantive issues and reproduced both inventories
(209 packages/54 scripts and 93 packages/19 scripts).
