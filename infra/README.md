# Repository infrastructure

- [Release branch and tag protection](github-release-state/README.md): current
  `HayaoSuzuki/hoimin` rules for the `hoimin-release-state` automation branch and immutable `v*` tags.
- [Historical main rules](github/README.md): the separate Pulumi project targeting
  `tokyogas-tech/hoimin`. Its committed state records the former-owner provider;
  it does not manage the current repository's existing main ruleset.

Choose the project for the intended repository before running Pulumi. Each local
backend has its own state and must be coordinated and committed after updates.
