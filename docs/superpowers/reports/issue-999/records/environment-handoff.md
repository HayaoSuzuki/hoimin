# Environment facts for Task 2

Read-only research; no images pulled, toolchain installed or environment build attempted yet.

- Docker Engine is accessible through /Users/hayao/.rd/bin/docker after sandbox escalation, Rancher Desktop; Linux aarch64, 4 CPUs, 8321900544 bytes memory. Do not prune existing images; they belong to unrelated projects.
- Current host free disk approximately109 GiB before Task1 compilation. Task1 exclusively owns heavy Cargo builds until its review ends.
- Official pinned upstream files retrieved through gh api to `/tmp/hoimin-issue999-design-mnk8miii/upstream/`. Filenames replace `/` with `__`; inspect installer, tools_versions_urls, creusot-deps.opam, why3find.json, creusot_why3.conf, setup lib.
- `creusot-install` supports selective positional components: build-prelude, prelude, why3, cargo-creusot, creusot-rustc, provers, why3-conf, cargo-creusot-config. `--provers-parallelism` sets concurrency. Installing `provers` iterates four tools; using selective components and separately obtaining only approved Z3/CVC5 avoids silently adding extra solvers.
- creusot-deps.opam includes why3-ide unless `in-creusot-ci` is set. Verify actual upstream CI setup syntax rather than guessing.
- Official Z3 Linux x86_64 binary needs glibc2.39; use compatible pinned base. Upstream tools_versions_urls includes approved hashes for both Linux x86_64 and ARM64.
- Official manual-install guide appears to say cargo install --path cargo-creusot for creusot-rustc; actual installer source correctly uses --path creusot-rustc. Prefer pinned source when the guide contains this typo.
- Source pin and displayed version are different for Why3/Why3find: version strings alone cannot distinguish the exact approved commit. Environment build provenance and file hashes should close that gap.
- Native Linux x86_64 is the acceptance environment. ARM64/emulated local timings cannot be silently treated as hosted x86_64 timings. No extra container publication is authorized. Cold setup is part of the 30-minute cap.
- Network sandbox can fail gh api with connection errors; the equivalent approved read-only escalation succeeded. No approval review rejected actions.
