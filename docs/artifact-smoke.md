# Packaged artifact smoke checks

Release preview and publication use the same checks. After checksum/SBOM
validation, separate read-only jobs download `verified-release` and verify its
checksums again. Three existing platform targets run `tests/archive_smoke.py`.
Attestation requires successful archive smoke and Linux ABI jobs; publication
already requires that attestation. A failure/cancellation cannot publish.

The archive helper accepts only the known four regular members. It copies the
executable bytes to a fixed temporary path, without extracting member paths or
links. It runs that absolute path and an explicit Python executable outside the
checkout. There is no build-tree or PATH binary fallback. A missing executable,
unexpected member, link, duplicate or wrong version fails. The helper is private
CI code for these validated artifacts, not a product archive-import interface.

`tests/wheel_smoke.py` supplies the shared tiny add/pytest mutation fixture.
Version/help, nonempty JSON mutant results, at least one killed/survived result
and unchanged source bytes are required. Existing wheel installation checks on
Windows, macOS and Linux are retained rather than repeated in archive jobs.
The wheel's installed executable is also selected by absolute path.

## Linux runtime and ABI evidence

The standalone archive is built and exercised on Ubuntu 22.04. `readelf` version
requirements must contain GLIBC symbols and require no version above 2.35; the
JSON evidence lists those versions and the actual host libc. This does not claim
standalone compatibility with glibc 2.17 or every older Linux distribution.

The manylinux2014 wheel is additionally tested in the official
[`pypa/manylinux2014_x86_64` image](https://github.com/pypa/manylinux), pinned by
digest in the workflow. The job requires CPython 3.14 and actual glibc 2.17,
records `auditwheel show` ABI output, and runs the existing wheel smoke with the
verified wheel. The checkout is mounted read-only; fixture/venv execution takes
place in container temporary directories. No Rust rebuild occurs. This is one
glibc-floor runtime check, not coverage of all Linux installations.

## Retained artifacts and maintenance

- `smoke-archive-<platform>`: archive name/SHA-256, binary SHA-256, expected and
  checked version, system, architecture, Python and libc; Linux symbol versions.
- `smoke-wheel-<host>`: wheel name/SHA-256, checked version and runtime identity
  from the existing build job's outside-checkout installation smoke.
- `smoke-linux-abi`: the same wheel identity/runtime evidence at glibc 2.17,
  auditwheel output and the pinned container image identity.

Evidence is separate from `verified-release`; adding it cannot change signed
release files or their checksum set. ABI logs are retained when execution fails;
a successful JSON report is emitted only after the checks pass. Failed jobs do
not imply a successful floor check. CI retains evidence with its normal artifact
retention, rather than adding files to the public package contract.

When updating the image, inspect its upstream build and digest, verify CPython
and glibc again, and run release preview. Monitor Runner space; the Linux job
removes the image after testing. Keep existing workspace/free-space safeguards.
These changes keep the current 0.3.x version floor and artifact layout.
