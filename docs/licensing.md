# Licensing

Starting with the commit that introduces [LICENSE](../LICENSE), hoimin is licensed
under the Elastic License 2.0 (ELv2, SPDX: `Elastic-2.0`). Releases built from
that commit or its descendants use ELv2. This change does not alter permissions
already granted for earlier versions.

You may use hoimin for commercial work, including running it locally or in
internal CI to test your company's software. You may also use it to test a SaaS
product you develop. These uses remain subject to the terms in LICENSE.

Providing third parties with access to a substantial set of hoimin's features
as a hosted or managed service requires separate permission from the licensor.
For example, a service that lets customers upload code or connect repositories
and run hoimin's mutation testing requires that permission, whether the service
is paid or free. Contact the [maintainer](https://github.com/HayaoSuzuki) to
discuss such use.

These examples explain the policy; the full terms in LICENSE govern use.
hoimin is source-available under ELv2, which is not an OSI-approved open-source
license. Third-party components retain their own licenses, including the
[vendored Ruff Python parser](../vendor/ruff_python_parser/LICENSE).

Dependency changes follow the [Rust dependency policy](dependency-policy.md).
Its allow-list check does not replace the license terms or redistribution duties.

The optional Rust `contracts` feature uses hoimin's own standard-library macros
under ELv2. The external MPL-2.0 `contracts` crate is no longer a dependency.
The [preserved third-party notice](../licenses/contracts-MPL-2.0.txt) and dated
adoption review document its former use.

Python release maintenance uses returns 0.29.0 (BSD-3-Clause) and deal 4.24.6
(MIT) from the locked internal-tooling group. These packages retain their own
distribution notices and are not dependencies of the shipped CLI or wheels.


## Creusot verification dependencies

The target-cfg dependency `creusot-std` 0.14.0 and its proc macro
`creusot-std-proc` 0.14.0 declare LGPL-2.1-or-later. The upstream
[license at the pinned source](https://github.com/creusot-rs/creusot/blob/65349025c67e69fcd7479f1f420fa5904f69bc27/LICENSE)
is preserved verbatim in [licenses/creusot-LGPL-2.1-or-later.txt](../licenses/creusot-LGPL-2.1-or-later.txt).
`pearlite-syn` 0.14.0 declares MIT OR Apache-2.0. The exact-version
cargo-deny exceptions admit these two LGPL declarations only for review of the
locked graphs; they do not establish redistribution compliance.

The normal stable build does not select `cfg(creusot)`: its source excludes
Creusot imports, logical models and contract attributes before macro expansion.
The normal all-features graph and generated shipping SBOM were checked in the
[adoption record](superpowers/reports/2026-10-19-issue-999-creusot-adoption.md).
The source distribution contains the annotated hoimin math crate and manifest;
Cargo resolves the registry packages from their checksummed lock entries when
verification selects them. It does not vendor those registry sources.

Verification builds execute upstream proc macros and use standard-library
contracts; generated Coma files and proof images therefore need their own
source, generated-content and notice review before redistribution. This work
publishes neither a proof image nor a new release, and makes no general license
compatibility conclusion about custom builds or generated proof artifacts.
When upgrading Creusot or changing cfg/features or distribution formats, review
the upstream license and macro output again, preserve applicable notices, and
update these bounded exceptions. Hoimin-authored sources remain under ELv2.
