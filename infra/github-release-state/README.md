# Release branch and tag protection

This Pulumi YAML project manages two rulesets for `HayaoSuzuki/hoimin`:

| Ruleset | Target | Protection |
| --- | --- | --- |
| `hoimin-release-state-protection` | `refs/heads/hoimin-release-state` | No deletion or non-fast-forward updates |
| `release-tags-protection` | `refs/tags/v*` | No deletion or updates to existing version tags |

Both rulesets have no bypass actors. New version tags remain creatable. The
provider is pinned to the existing infrastructure's GitHub version 6.15.0.

The release workflow creates and advances this ref directly as part of an atomic
version-tag reservation. `tools/release.py` checks ancestry before pushing and
uses `--force-with-lease` as compare-and-swap protection; the resulting update is
a fast-forward. Do not require a pull request, merge queue, or status checks on
this automation ref. Ordinary fast-forward writes by identities with repository
write permission remain possible; this ruleset does not restrict writers to one
workflow. See the [release protocol](../../docs/development/releases/index.md).

## Preview

Install Pulumi and authenticate GitHub CLI with an identity that has repository
Administration read/write permission. From the repository root:

```sh
cd infra/github-release-state
export GITHUB_TOKEN="$(gh auth token)"
export PULUMI_CONFIG_PASSPHRASE=""
gh api repos/HayaoSuzuki/hoimin/rulesets
```

The project uses its own local backend, separate from the historical
[`infra/github`](../github/README.md) project targeting the former owner. The
`production` stack is initialized in the committed backend; it currently contains
only stack metadata, with no deployed GitHub resources. Open it explicitly:

```sh
pulumi stack --stack production
```

Obtain the latest committed state instead of initializing another stack. Do not run `pulumi login`. This stack manages public rules only;
keep credentials in the environment and never add secrets to the empty-passphrase
state. Commit the generated `Pulumi.production.yaml` and canonical state under
`state/.pulumi/` after initialization and each update, including failed updates.
Run one update at a time across clones; local backend locks do not coordinate
separate checkouts.

If either ruleset already exists, import it instead of creating a duplicate.
Match the observed name to the corresponding resource and replace its ID below;
run only the import commands for rulesets that already exist:

```sh
GITHUB_OWNER=HayaoSuzuki pulumi import --stack production github:index/repositoryRuleset:RepositoryRuleset releaseState 'hoimin:STATE_RULESET_ID' --generate-code=false
GITHUB_OWNER=HayaoSuzuki pulumi import --stack production github:index/repositoryRuleset:RepositoryRuleset releaseTags 'hoimin:TAGS_RULESET_ID' --generate-code=false
```

Review the preview before applying. A new deployment creates one provider and two
rulesets, with no main-branch rule changes:

```sh
pulumi preview --stack production --refresh --diff
pulumi up --stack production --refresh --diff
```

## Verify

```sh
gh api "repos/HayaoSuzuki/hoimin/rulesets/$(pulumi stack output releaseStateRulesetId --stack production)"
gh api repos/HayaoSuzuki/hoimin/rules/branches/hoimin-release-state
gh api "repos/HayaoSuzuki/hoimin/rulesets/$(pulumi stack output releaseTagsRulesetId --stack production)"
pulumi preview --stack production --refresh --diff
```

Check both exact ref conditions, active enforcement and empty bypass lists.
The branch rule prohibits deletion and non-fast-forward updates; the tag rule
prohibits deletion and any update but permits creation. A normal release should
still reserve a new tag and advance the state ref atomically. Retrying an existing
reservation reuses its tag without updating it. Avoid testing destructive pushes against the
production state branch. Local YAML checks are not evidence of a deployed rule.
Pulumi `protect: true` also prevents accidental removal of the managed ruleset.
