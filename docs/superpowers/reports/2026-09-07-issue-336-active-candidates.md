# Remove write-only candidate bookkeeping (#336)

## Change

`ShellContext::active_candidates` retained candidate copies per worker, but no
shell path read the map. Candidate loading and mutation application inserted
copies; empty loads and worker reset removed them.

The shell now passes mutation requests to the workspace and returns completion
events without maintaining that extra map. `WorkspaceTask::Apply` and
`CandidateLoaded` still own their candidates. The direct-I/O test path retains
the clone required to pass a candidate reference alongside its owned request.

This removes redundant candidate clones and map updates. Candidate selection,
event payloads, worker ownership, reset behavior, and public output are unchanged.

## Verification

The existing shell tests passed before and after this behavior-preserving
refactor:

```sh
cargo test -p hoimin-cli --all-features --lib shell::tests --quiet -- --test-threads=1
```

Both runs completed with 103 passed, 0 failed, and 1 ignored on macOS arm64.
The suite covers direct mutation application, blocking workspace ownership,
shutdown draining, and cleanup. An independent review confirmed that the
removed field had no reads and that the remaining owners retain the candidates.

Final checks passed on macOS arm64:

```sh
cargo test --workspace --all-features --quiet -- --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```
