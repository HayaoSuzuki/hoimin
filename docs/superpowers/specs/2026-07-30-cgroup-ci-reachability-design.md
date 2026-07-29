# Delegated cgroup CI Reachability Design

## Decision

Add `push` restricted to `main` to the CI workflow. Keep the delegated job condition, repository
variable opt-in, and runner labels unchanged. This makes the existing condition reachable while
ensuring pull requests continue to use hosted runners and skip the delegated job.

Changing the job to `workflow_dispatch` would provide manual reachability but would not produce
automatic evidence for the exact merged revision. Removing the event/ref guard would risk sending
untrusted pull-request code to a privileged self-hosted runner. Both alternatives are rejected.

## Contract

The Python workflow contract will parse the top-level trigger block, find each job-level `if`
expression, extract literal `github.event_name == '<event>'` references, and require them to be
declared triggers. It will also pin the delegated job's main-ref guard, variable opt-in, labels, and
fail-closed `SKIP:` check.

## Operations

Maintainers confirm execution with `gh run list --workflow CI --event push --branch main`, then
inspect the selected run with `gh run view`. Success requires `linux-cgroup-v2-hard` to complete and
its log to contain no `SKIP:` marker.

A missing offline or incorrectly labelled self-hosted runner leaves the job queued; GitHub fails a
self-hosted job after 24 hours in the queue. Maintainers should inspect repository or organization
Actions runners for an online runner carrying all four labels:
`self-hosted`, `linux`, `x64`, and `cgroup-v2-delegated`.

The first actual main-push execution can only be verified after this PR is merged. The PR itself
proves trigger reachability statically and intentionally does not expose the delegated runner to
pull-request code.
