# Agent skills

hoimin distributes two English-language [Agent Skills](https://agentskills.io/)
under `skills/`:

- `hoimin-mutation-testing`: choose focused Python mutation candidates, create a
  disk-bounded plan, and verify selected IDs against a fresh baseline.
- `hoimin-mutation-improvement`: improve behavioral tests for surviving
  candidates, preserve plan identity, and report unverified work honestly.

Each skill includes its own PyPI setup reference and can be installed separately.
Skills are instructions for an agent; installing them does not install hoimin,
run tests, or grant permission to execute commands. The repository's
[Elastic License 2.0](../LICENSE) also applies to the skills.

## Install with GitHub CLI

Use GitHub CLI 2.90.0 or later with `gh skill --help` available. See the official
[skill command reference](https://cli.github.com/manual/gh_skill) for this preview
feature. Upgrade the CLI if it reports `unknown command "skill"`.

Inspect a skill before installation. For reproducible installation, replace
`COMMIT_SHA` with the reviewed full commit SHA that contains `skills/`:

```console
gh skill preview HayaoSuzuki/hoimin hoimin-mutation-testing@COMMIT_SHA
gh skill install HayaoSuzuki/hoimin hoimin-mutation-testing --agent codex --scope project --pin COMMIT_SHA
gh skill install HayaoSuzuki/hoimin hoimin-mutation-improvement --agent codex --scope project --pin COMMIT_SHA
```

For Claude Code, use `--agent claude-code` instead of `--agent codex`. Use
`--scope user` only when you want the skills available across projects. Install
into the project under test; hoimin's distribution repository does not maintain
local `.agents/skills` or `.claude/skills` mirrors.

An unpinned install chooses the latest tagged release, falling back to the default
branch only if no release exists. Releases from before this layout change do not
contain `skills/`; until a release includes it, use a reviewed commit or
`--pin main` explicitly. A main pin follows a moving branch, whereas a commit pin
identifies fixed content. Preview and install the same reviewed revision.

For a local checkout, install without changing the distribution source:

```console
gh skill install . hoimin-mutation-testing --from-local --agent codex --scope project
gh skill install . hoimin-mutation-improvement --from-local --agent codex --scope project
```

The CLI copies the skill and adds source-tracking metadata to the installed copy.
Keep install-generated `metadata.github-*` fields out of committed distribution
files. Check installed skills with `gh skill list --agent codex`. Pinned installs
remain pinned unless explicitly changed; use `gh skill update --help` before
updating them.

## Maintainer checks and releases

Edit the canonical files under `skills/`. Preserve the disk limits, cleanup
ownership, native test argv, replan conditions, fresh-baseline requirement, and
candidate-set restrictions on progress comparisons. Keep bundled references
inside each skill so installing a single skill leaves no missing sibling links.

From the repository root, validate packaging without publishing:

```console
gh skill publish --dry-run
uv run --frozen --no-sync pytest tests/test_skills.py
```

hoimin's existing release workflow owns product tags and GitHub Releases. Skill
files are fetched from the tagged repository tree; no wheel or separate skill
archive is required. After a product release includes `skills/`, users can install
that release's skills by pinning its tag. Do not run `gh skill publish --tag` as
part of routine skill validation: it creates a separate release and can conflict
with hoimin's version allocation and publication workflow.

For the mutation workflow itself, follow the [usage guide](usage.md), including
[plan/verify](usage.md#plan-and-verify-with-an-agent) and
[resource limits](usage.md#limits-and-defaults). Writing clear tests follows the
[development guide](development.md#test-structure).
