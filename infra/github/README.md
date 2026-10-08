# GitHub rules managed with Pulumi

This Pulumi YAML project manages one `main-protection` repository ruleset for
`tokyogas-tech/hoimin`. The GitHub provider is pinned to version 6.15.0.

The rules require a pull request, the nine existing PR checks in
`.github/workflows/ci.yml`, and a merge queue. They prohibit branch deletion
and force pushes. The bypass list is empty, so administrators also follow
the rules. Review approval count is zero; this change adds no reviewer quota.

The queue uses merge commits, `ALLGREEN`, one concurrent build, one PR per
merge, and a 120-minute check timeout. Adjust the timeout if runner waiting
time plus CI duration exceeds it. Queue validation replaces the requirement
to update each PR branch to the latest main before entering the queue.

The required checks bind to GitHub Actions (integration ID `15368`). Manual
non-Linux CI, scheduled Rust canary, performance measurements, releases, and
the push-only `linux-cgroup-v2-hard` job are outside the PR gate. Preserve the
cgroup job's trusted push-only trigger when changing queue configuration.

## Prerequisites

- Install the [Pulumi CLI](https://www.pulumi.com/docs/install/).
- Use an identity that can manage repository rulesets. For a fine-grained
  token, grant access to this repository with Administration read/write.
  Store credentials outside Git, for example in `GITHUB_TOKEN`.
- For a private repository, merge queues require GitHub Enterprise Cloud.
  The organization API reported the `enterprise` plan during setup.
- Install GitHub CLI and authenticate with `gh auth login` using the
  identity above.

## State in Git

The backend URL in `Pulumi.yaml` is `file://./state`. Commit the canonical
state under `state/.pulumi/` and `Pulumi.production.yaml` alongside the
configuration. The state ignore file excludes locks, history, backups,
and local file metadata (`*.attrs`).
Run all commands below from `infra/github`; the backend path is relative
to the working directory. Do not run `pulumi login` for this project.

The stack uses an empty passphrase because it manages repository rules
only. Do not add secrets, tokens, or `github.ActionsSecret` resources to
this project: anyone with access to Git history could decrypt secrets
encrypted with that passphrase. Pass the GitHub token through the
environment; do not use `pulumi config set github:token`.

Before an update, obtain the latest committed state and coordinate with
the other maintainers. Run only one update at a time across all clones;
local backend locks do not coordinate separate clones. After an update
(including a failed update), inspect and commit any state changes before
another maintainer runs Pulumi. Do not resolve divergent state JSON with
a textual merge or use an old checkout to apply settings.

## First deployment

1. Merge the accompanying `.github/workflows/ci.yml` change into `main`
   **before enabling the ruleset**. It adds `merge_group: checks_requested`.
   Confirm that the nine PR checks pass. Enabling the queue before this
   workflow change reaches main can leave queued PRs waiting for checks.
2. Change directory and configure credentials for the current shell:

   ```sh
   cd infra/github
   export GITHUB_TOKEN="$(gh auth token)"
   export PULUMI_CONFIG_PASSPHRASE=""
   ```

3. Use the committed `production` stack. It initially contains no deployed
   resources. The explicit `--stack production` on each command avoids
   depending on the machine's selected stack. Do not initialize another
   stack or backend for the same ruleset.
4. Confirm `pulumi stack --stack production` opens the committed stack.
   If it is missing, check the working directory and Git checkout first.
5. Inspect existing settings before creating a rule:

   ```sh
   gh api repos/tokyogas-tech/hoimin/rulesets
   gh api repos/tokyogas-tech/hoimin/branches/main/protection
   ```

   During setup the ruleset list was empty and the protection endpoint
   returned 404 with a non-admin identity. Recheck with the deployment
   identity; a 404 alone does not establish that no protection exists.
   Existing rules combine with this ruleset. If `main-protection` already
   exists, import it into this stack instead of creating a duplicate:

   ```sh
   GITHUB_OWNER=tokyogas-tech pulumi import --stack production github:index/repositoryRuleset:RepositoryRuleset main 'hoimin:RULESET_ID' --generate-code=false
   ```

   Supply `GITHUB_OWNER=tokyogas-tech` for the import command, since CLI
   import uses a default provider. Review the next preview, including the
   transition to this project's explicit provider.
6. Review and apply:

   ```sh
   pulumi preview --stack production --refresh --diff
   pulumi up --stack production --refresh --diff
   ```

   For a new stack, expect one GitHub provider and one ruleset to be created.
   The project does not adopt the repository itself as a managed resource.

## Verify and maintain

```sh
gh api "repos/tokyogas-tech/hoimin/rulesets/$(pulumi stack output rulesetId --stack production)"
gh api repos/tokyogas-tech/hoimin/rules/branches/main
pulumi preview --stack production --refresh --diff
```

Check that enforcement is `active`, the target is `refs/heads/main`, bypass
actors are empty, and the rules include pull requests, nine required checks,
deletion protection, force-push protection, and a merge queue. Use the next
normal PR to confirm that failed required checks block merging and that
queue entry starts CI on `merge_group`. Local validation cannot establish
these live behaviors.

When adding or renaming PR jobs, update `requiredChecks` in `Pulumi.yaml`
in the same change and apply the stack after the workflow is ready. Ensure
new required jobs run on both `pull_request` and `merge_group`. GitHub
accepts skipped or neutral check conclusions; do not add conditional skips
to required jobs as a substitute for successful validation. Currently the
nine PR jobs have no job-level condition and their prerequisites are also
required checks.

The ruleset uses Pulumi `protect: true` to prevent accidental deletion by
`pulumi destroy` or resource removal. To retire it deliberately, change the
protection setting, apply that change, then review the removal separately.
After applying, commit `Pulumi.production.yaml` and changes under `state/`
with the configuration change. Keep credentials outside version control.

References: [Pulumi RepositoryRuleset](https://www.pulumi.com/registry/packages/github/api-docs/repositoryruleset/),
[GitHub merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
