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

The optional Rust `contracts` feature uses the compile-time `contracts` 0.6.11
proc macro under MPL-2.0. Its reviewed, unmodified source is available from
[the published crate](https://crates.io/crates/contracts/0.6.11) and its license
is preserved in [the third-party notice](../licenses/contracts-MPL-2.0.txt).
The dependency policy permits MPL-2.0 only for this exact crate/version; upgrades
require source, generated-code and notice review. Default release archives and
wheels do not enable this feature. Include the notice and upstream source link
when distributing a custom feature-enabled build. Hoimin's own license remains
ELv2.

Python release maintenance uses returns 0.29.0 (BSD-3-Clause) and deal 4.24.6
(MIT) from the locked internal-tooling group. These packages retain their own
distribution notices and are not dependencies of the shipped CLI or wheels.

