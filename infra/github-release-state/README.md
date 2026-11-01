# Release-state branch protection

This Pulumi YAML project manages `hoimin-release-state-protection` for
`HayaoSuzuki/hoimin`. It targets only `refs/heads/hoimin-release-state`, prohibits
branch deletion and non-fast-forward updates, and has no bypass actors. The
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

If a ruleset with this name already exists, import its ID instead of creating a
duplicate (replace `RULESET_ID` with the observed numeric ID):

```sh
GITHUB_OWNER=HayaoSuzuki pulumi import --stack production github:index/repositoryRuleset:RepositoryRuleset releaseState 'hoimin:RULESET_ID' --generate-code=false
```

Review the preview before applying. A new deployment creates one provider and one
ruleset, with no main-branch rule changes:

```sh
pulumi preview --stack production --refresh --diff
pulumi up --stack production --refresh --diff
```

## Verify

```sh
gh api "repos/HayaoSuzuki/hoimin/rulesets/$(pulumi stack output releaseStateRulesetId --stack production)"
gh api repos/HayaoSuzuki/hoimin/rules/branches/hoimin-release-state
pulumi preview --stack production --refresh --diff
```

Check the exact branch condition, active enforcement, empty bypass list, deletion
rule and non-fast-forward rule. A normal release should still reserve its tag and
advance the state ref atomically. Avoid testing destructive pushes against the
production state branch. Local YAML checks are not evidence of a deployed rule.
Pulumi `protect: true` also prevents accidental removal of the managed ruleset.
