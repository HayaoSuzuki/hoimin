# Install a private GitHub Release wheel

Use `gh` authenticated as an account with read access to `tokyogas-tech/hoimin`.
Check with `gh auth status --hostname github.com`; if login is needed, run
`gh auth login --hostname github.com` and complete the organization SSO steps
when required. In automation, provide `GH_TOKEN` with access to this repository
(a fine-grained token needs Contents: read). Never print the token.

`gh release download` handles authenticated asset downloads. A private asset URL
passed to unauthenticated `curl`, `wget`, or uv does not inherit the `gh` login.
Download with `gh` first, then give uv the local wheel path.

## Download and install

Run the following in Bash (on Windows, use Git Bash). It requires `gh`, `uv`, and
a published release. Wheels require Python 3.14. Use a shell matching the native
architecture; for example, Apple Silicon requires an arm64 shell, not Rosetta.
Linux wheels target glibc-based x86_64 systems, not Alpine/musl.

Set `HOIMIN_RELEASE_TAG` to a published tag to select a particular version.
Otherwise, resolve the latest published release once and record the selected tag.
The subshell keeps its temporary-directory cleanup separate from the mutation
workflow's cleanup trap.

```bash
(
  set -euo pipefail
  hoimin_repo=tokyogas-tech/hoimin
  hoimin_tag="${HOIMIN_RELEASE_TAG:-}"
  if [[ -z "$hoimin_tag" ]]; then
    hoimin_tag=$(gh release view --repo "$hoimin_repo" --json tagName --jq .tagName)
  fi
  readonly hoimin_tag
  case "$(uname -s):$(uname -m)" in
    Darwin:arm64) hoimin_pattern='hoimin-*-macosx_*_arm64.whl' ;;
    Linux:x86_64) hoimin_pattern='hoimin-*-manylinux*_x86_64.whl' ;;
    MINGW*:x86_64|MSYS*:x86_64) hoimin_pattern='hoimin-*-win_amd64.whl' ;;
    *) echo 'No release wheel documented for this OS/architecture.' >&2; exit 1 ;;
  esac
  hoimin_download_dir=$(mktemp -d)
  readonly hoimin_download_dir
  cleanup_hoimin_download() {
    if ! rm -rf -- "$hoimin_download_dir" || [[ -e "$hoimin_download_dir" ]]; then
      printf 'Cleanup failed; retained path: %s\n' "$hoimin_download_dir" >&2
      return 1
    fi
  }
  trap cleanup_hoimin_download EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  gh release download "$hoimin_tag" --repo "$hoimin_repo" \
    --pattern "$hoimin_pattern" --dir "$hoimin_download_dir"
  shopt -s nullglob
  hoimin_wheels=("$hoimin_download_dir"/*.whl)
  if [[ ${#hoimin_wheels[@]} -ne 1 ]]; then
    echo 'Expected exactly one platform wheel.' >&2
    exit 1
  fi
  hoimin_wheel="${hoimin_wheels[0]}"
  printf 'Installing hoimin from release %s\n' "$hoimin_tag"
  uv tool install --reinstall --python 3.14 "$hoimin_wheel"
  uv tool run --python 3.14 hoimin --version
)
```

`--reinstall` replaces an existing hoimin tool installation with the selected
wheel. Ensure the reported version matches the selected tag. If the executable is
not on `PATH`, run `uv tool update-shell` and open a new shell before using the
skill's plain `hoimin` commands.

## Run with uvx instead

To avoid a persistent tool installation, replace the final two uv commands in the
subshell with:

```bash
uvx --python 3.14 --from "$hoimin_wheel" hoimin --help
```

Run the plan and every verify in that same subshell before cleanup, replacing each
`hoimin` command with `uvx --python 3.14 --from "$hoimin_wheel" hoimin`.
Keep the downloaded wheel available for the entire loop; resolve neither the
latest tag nor a new wheel between plan and verify.

If access, release lookup, download, or compatibility fails, stop setup and report
the cause. A 404 can mean a missing release or insufficient repository access.
Do not silently switch to `uvx hoimin`, PyPI, or a Git source build. A Git source
URL builds from source even when Release wheels exist. Use a working-tree build
when testing unreleased hoimin changes.

References: [gh release download](https://cli.github.com/manual/gh_release_download),
[gh auth login](https://cli.github.com/manual/gh_auth_login),
[uv tools](https://docs.astral.sh/uv/guides/tools/).
