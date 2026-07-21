# Exact Fingerprint File Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable `--fingerprint-file PATH` support that fingerprints exactly one root-relative regular file without matching nested files of the same name.

**Architecture:** Preserve `--fingerprint-include` glob semantics and add exact paths as a second selector in normalized configuration. Refactor the CLI-side resolver to collect glob and exact paths into one ordered map before reading and hashing each selected file once; run and plan/verify continue consuming the shared `fingerprint_inputs` records.

**Tech Stack:** Rust 2024 (MSRV 1.85), `clap`, `camino`, `ignore`, `blake3`, `serde`, `tokio`, SQLite via `rusqlite`.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-13-fingerprint-file` on branch `issue-13-fingerprint-file`.
- Add repeatable `--fingerprint-file PATH` to `run` and `plan`; `verify` must not accept it.
- Resolve `PATH` relative to `--root`, treat glob metacharacters literally, and select exactly one regular file.
- Reject empty, NUL, absolute, parent, Windows drive/UNC, outside-root, missing, directory, symlink, non-UTF-8, and unreadable inputs before baseline/session/candidate discovery.
- Keep `--fingerprint-include` glob semantics, workspace copy behavior, mutation behavior, session tables, and public schema versions unchanged.
- Merge glob and exact selections by normalized `/`-separated root-relative path before reading and hashing each file once.
- Preserve original exact selectors as `fingerprint_files`; preserve the merged path/hash list as `fingerprint_inputs`.
- Existing plan manifests deserialize with `fingerprint_files` defaulting to an empty list.
- Use error codes `fingerprint.file.invalid_path`, `fingerprint.file.not_found`, and `fingerprint.file.unsupported_file`.
- Use TDD for every behavior change and commit only Issue #13 files from this worktree.

---

## File Structure

- Modify `crates/hoimin-core/src/config.rs` — store exact selectors in raw, run, and plan configuration and preserve old manifest deserialization.
- Modify `crates/hoimin-core/tests/plan_config.rs` — prove run/plan conversion and serde compatibility.
- Modify `crates/hoimin-cli/src/cli.rs` — parse the shared repeated option for `run` and `plan`.
- Modify `crates/hoimin-cli/tests/cli_config.rs` — verify parsing and command-surface constraints.
- Modify `crates/hoimin-cli/src/fingerprint_inputs.rs` — validate and resolve exact paths, merge them with glob paths, deduplicate, then hash.
- Modify `crates/hoimin-cli/tests/fingerprint_inputs.rs` — cover exact matching, path safety, special filenames, file types, and merged selections.
- Modify `crates/hoimin-cli/src/shell.rs` — pass exact selectors during run preparation.
- Modify `crates/hoimin-cli/src/report/human.rs` — render exact selector provenance.
- Modify `crates/hoimin-cli/tests/report_handler.rs` — verify human provenance.
- Modify `crates/hoimin-cli/tests/run_e2e.rs` — verify early errors, JSON/JSONL provenance, session invalidation, and nested-worktree isolation.
- Modify `crates/hoimin-cli/src/plan.rs` — store and re-resolve exact selectors for plan/verify.
- Modify `crates/hoimin-cli/tests/plan.rs` — verify manifest compatibility and exact-file change behavior.
- Modify `README.md` — document exact-file semantics and independent copy policy.

### Task 1: Add exact selectors to core configuration

**Files:**
- Modify: `crates/hoimin-core/src/config.rs:228-383,474-543`
- Modify: `crates/hoimin-core/tests/plan_config.rs:1-70`

**Interfaces:**
- Produces `RawRunConfig.fingerprint_files: Vec<String>`.
- Produces `RunConfig.fingerprint_files: Vec<String>`.
- Produces `PlanConfig.fingerprint_files: Vec<String>` with `#[serde(default)]`.
- Preserves `RunConfig::{into_plan_config}` and `PlanConfig::into_run_config` round trips.

- [ ] **Step 1: Write failing core conversion and compatibility tests**

Extend the `RawRunConfig` fixture in `plan_config.rs` with:

```rust
fingerprint_files: vec!["pyproject.toml".to_owned()],
```

Add assertions after both conversions:

```rust
assert_eq!(plan.fingerprint_files, ["pyproject.toml"]);
let run = plan.into_run_config(OutputConfig::default());
assert_eq!(run.fingerprint_files, ["pyproject.toml"]);
```

Add a legacy manifest/config deserialization test that removes the new property:

```rust
#[test]
fn plan_config_defaults_missing_fingerprint_files() {
    let run = RunConfig::try_from(fixture_raw_config()).unwrap();
    let mut value = serde_json::to_value(run.into_plan_config()).unwrap();
    value.as_object_mut().unwrap().remove("fingerprint_files");
    let plan: PlanConfig = serde_json::from_value(value).unwrap();
    assert!(plan.fingerprint_files.is_empty());
}
```

- [ ] **Step 2: Run the tests and verify the new field is missing**

Run: `cargo test -p hoimin-core --test plan_config`

Expected: compilation fails because `fingerprint_files` does not exist.

- [ ] **Step 3: Thread the field through core configuration**

Add the following fields and moves in `config.rs`:

```rust
// RawRunConfig
pub fingerprint_files: Vec<String>,

// RunConfig
pub fingerprint_files: Vec<String>,

// PlanConfig
#[serde(default)]
pub fingerprint_files: Vec<String>,
```

In `TryFrom<RawRunConfig> for RunConfig`:

```rust
fingerprint_includes: raw.fingerprint_includes,
fingerprint_files: raw.fingerprint_files,
fingerprint_inputs: Vec::new(),
```

Move the field unchanged in both `into_plan_config` and `into_run_config` beside `fingerprint_includes`.

- [ ] **Step 4: Run core tests and formatting**

Run: `cargo test -p hoimin-core --test plan_config && cargo fmt --check`

Expected: PASS.

- [ ] **Step 5: Commit the core contract**

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/plan_config.rs
git commit -m "feat: store exact fingerprint file selectors"
```

### Task 2: Parse `--fingerprint-file` for run and plan

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs:70-203,240-260,420-560`
- Modify: `crates/hoimin-cli/tests/cli_config.rs:1-340`

**Interfaces:**
- Consumes `RawRunConfig.fingerprint_files` from Task 1.
- Produces `RawMutationArgs.fingerprint_file: Vec<String>` and `RunArgs.fingerprint_files: Vec<String>`.
- Keeps the option unavailable on `verify` because `RawVerifyArgs` remains unchanged.

- [ ] **Step 1: Write failing CLI tests**

Extend the repeated-selector test with:

```rust
"--fingerprint-file", "pyproject.toml",
"--fingerprint-file", "config/settings[prod].toml",
```

Assert:

```rust
assert_eq!(
    config.fingerprint_files,
    ["pyproject.toml", "config/settings[prod].toml"]
);
```

Add `--fingerprint-file pyproject.toml` to the existing plan acceptance case and assert the parsed plan config retains it. Extend the verify rejection case with an invocation containing `--fingerprint-file` and assert `parse_from(...)` returns an error.

- [ ] **Step 2: Run the focused CLI tests and verify Clap rejects the option**

Run: `cargo test -p hoimin-cli --test cli_config -- fingerprint_file`

Expected: FAIL because `--fingerprint-file` is unknown or the field is absent.

- [ ] **Step 3: Add the shared CLI option and conversion**

In `RawMutationArgs` add:

```rust
/// Add one exact root-relative file to the session fingerprint; may be repeated.
#[arg(long, value_name = "PATH")]
fingerprint_file: Vec<String>,
```

In public `RunArgs` add:

```rust
pub fingerprint_files: Vec<String>,
```

Map `raw.mutation.fingerprint_file` into `RunArgs`, then set:

```rust
fingerprint_files: args.fingerprint_files,
```

when constructing `RawRunConfig`.

- [ ] **Step 4: Run CLI configuration tests**

Run: `cargo test -p hoimin-cli --test cli_config && cargo fmt --check`

Expected: PASS, including `plan` acceptance and `verify` rejection.

- [ ] **Step 5: Commit the CLI surface**

```bash
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: parse exact fingerprint files"
```

### Task 3: Resolve and merge exact fingerprint paths

**Files:**
- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs:1-121`
- Modify: `crates/hoimin-cli/tests/fingerprint_inputs.rs:1-145`

**Interfaces:**
- Replaces `resolve(root, patterns)` with:

```rust
pub fn resolve(
    root: &Utf8Path,
    patterns: &[String],
    files: &[String],
) -> Result<Vec<FingerprintInputFile>, FingerprintInputError>
```

- Adds `FingerprintInputError::{InvalidPath, NotFound}` while retaining the three include variants.
- Internally selects normalized `Utf8PathBuf` values first, deduplicates in `BTreeSet`, and hashes afterward.

- [ ] **Step 1: Update existing tests to the three-argument interface**

Mechanically change existing glob calls from:

```rust
resolve(&fixture.root, &patterns)
```

to:

```rust
resolve(&fixture.root, &patterns, &[])
```

Run: `cargo test -p hoimin-cli --test fingerprint_inputs`

Expected: compilation fails because `resolve` still takes two arguments.

- [ ] **Step 2: Add failing exact-match and merged-dedup tests**

Add:

```rust
#[test]
fn exact_file_selects_only_the_named_root_relative_file() {
    let fixture = fixture_root(&[
        ("pyproject.toml", "root"),
        (".worktrees/a/pyproject.toml", "nested"),
    ]);
    let records = resolve(&fixture.root, &[], &["pyproject.toml".into()]).unwrap();
    assert_eq!(records.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["pyproject.toml"]);
    assert_eq!(records[0].hash, blake3::hash(b"root").to_hex().to_string());
}

#[test]
fn glob_and_exact_file_are_deduplicated_before_hashing() {
    let fixture = fixture_root(&[("pyproject.toml", "root")]);
    let records = resolve(
        &fixture.root,
        &["pyproject.toml".into()],
        &["./pyproject.toml".into(), "pyproject.toml".into()],
    ).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, "pyproject.toml");
}

#[test]
fn exact_file_treats_glob_metacharacters_literally() {
    let fixture = fixture_root(&[("settings[prod]*.toml", "x")]);
    let records = resolve(&fixture.root, &[], &["settings[prod]*.toml".into()]).unwrap();
    assert_eq!(records[0].path, "settings[prod]*.toml");
}
```

- [ ] **Step 3: Add failing exact-path validation tests**

Add table-driven assertions for `""`, `"/tmp/x"`, `"C:\\tmp\\x"`, `"\\\\server\\share\\x"`, `"../x"`, `"nested\\..\\x"`, and `"file\0name"`, requiring `fingerprint.file.invalid_path`. Add missing-file, directory, symlink, and Unix unreadable cases requiring `fingerprint.file.not_found` or `fingerprint.file.unsupported_file`.

Run: `cargo test -p hoimin-cli --test fingerprint_inputs`

Expected: FAIL because exact files are not resolved and the new errors do not exist.

- [ ] **Step 4: Refactor selection before hashing**

Change the error enum to include:

```rust
#[error("fingerprint.file.invalid_path: {0}")]
InvalidPath(String),
#[error("fingerprint.file.not_found: {0}")]
NotFound(String),
```

Retain `UnsupportedFile`, but make it carry an error-family prefix so exact paths emit `fingerprint.file.unsupported_file` and glob paths continue emitting `fingerprint.include.unsupported_file`. One concrete shape is:

```rust
#[error("{code}: {detail}")]
UnsupportedFile { code: &'static str, detail: String },
```

Implement `normalize_exact_path(path: &str) -> Result<Utf8PathBuf, FingerprintInputError>` by replacing `\\` with `/`, rejecting the unsafe forms above, dropping empty and `.` components, and joining remaining normal components with `/`. Do not interpret `*`, `?`, or `[`.

Refactor `resolve` to collect paths, then hash:

```rust
let mut selected = BTreeSet::new();
for pattern in patterns {
    // existing validation and walk; insert each normalized match
}
for file in files {
    selected.insert(resolve_exact(root, file)?);
}
let mut records = Vec::with_capacity(selected.len());
for path in selected {
    let bytes = std::fs::read(root.join(&path)).map_err(|error| {
        FingerprintInputError::UnsupportedFile {
            code: "fingerprint.file.unsupported_file",
            detail: format!("{path}: {error}"),
        }
    })?;
    records.push(FingerprintInputFile {
        path,
        hash: blake3::hash(&bytes).to_hex().to_string(),
    });
}
Ok(records)
```

`resolve_exact` must use `symlink_metadata`, map `NotFound` to `NotFound`, reject symlinks/directories/non-files before reading, and return the normalized relative path. Preserve include error prefixes for paths selected by glob; if a merged path is selected by both kinds, exact-file error provenance wins.

- [ ] **Step 5: Run resolver tests and lint the module**

Run: `cargo test -p hoimin-cli --test fingerprint_inputs && cargo clippy -p hoimin-cli --tests -- -D warnings`

Expected: PASS.

- [ ] **Step 6: Commit the resolver**

```bash
git add crates/hoimin-cli/src/fingerprint_inputs.rs crates/hoimin-cli/tests/fingerprint_inputs.rs
git commit -m "feat: resolve exact fingerprint files"
```

### Task 4: Integrate exact files into run, reports, and sessions

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs:500-518`
- Modify: `crates/hoimin-cli/src/report/human.rs:1-30`
- Modify: `crates/hoimin-cli/tests/report_handler.rs:880-990`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:230-430`

**Interfaces:**
- Consumes `fingerprint_inputs::resolve(root, includes, files)` from Task 3.
- Produces normalized JSON/JSONL fields `fingerprint_files` and merged `fingerprint_inputs`.
- Human output adds `fingerprint files: [...]` independently of glob provenance.

- [ ] **Step 1: Write failing run preparation and report tests**

Add an E2E test modeled on `fingerprint_include_is_reported` using:

```rust
let options = ["--fingerprint-file", "pyproject.toml"];
```

Assert JSON and JSONL contain:

```rust
serde_json::json!(["pyproject.toml"])
```

at `normalized_config.fingerprint_files`, and contain only `pyproject.toml` in `fingerprint_inputs` even after creating `.worktrees/a/pyproject.toml` under the fixture root.

Extend the human report fixture with `fingerprint_files: vec!["pyproject.toml".into()]` and assert:

```rust
stdout.text().contains("fingerprint files: [pyproject.toml]")
```

- [ ] **Step 2: Run the focused tests and verify preparation is not wired**

Run: `cargo test -p hoimin-cli --test run_e2e fingerprint_file_is_reported -- --exact && cargo test -p hoimin-cli --test report_handler human_format_includes_profile_and_fingerprint_provenance_for_normalized_runs -- --exact`

Expected: FAIL because run preparation does not pass exact selectors and human output omits them.

- [ ] **Step 3: Wire run preparation and human provenance**

In `prepare_config` in `shell.rs` call:

```rust
config.fingerprint_inputs = crate::fingerprint_inputs::resolve(
    &config.root,
    &config.fingerprint_includes,
    &config.fingerprint_files,
)?;
```

In `report/human.rs`, independently render:

```rust
if !config.fingerprint_files.is_empty() {
    writeln!(writer, "fingerprint files: [{}]", config.fingerprint_files.join(", "))?;
}
if !config.fingerprint_inputs.is_empty() {
    writeln!(writer, "fingerprint inputs: [{inputs}]")?;
}
```

Keep `fingerprint includes` conditional on `fingerprint_includes`; do not print the merged input list twice.

- [ ] **Step 4: Add early-failure and session-behavior tests**

Add a missing exact-file test parallel to `fingerprint_include_unmatched_fails_before_creating_session`; assert exit 2, `fingerprint.file.not_found`, and no SQLite file. Add a session test that:

1. runs with `--fingerprint-file pyproject.toml`,
2. marks the run incomplete,
3. changes only `.worktrees/a/pyproject.toml` and confirms the same run ID resumes,
4. marks it incomplete again,
5. changes root `pyproject.toml` and confirms a new run ID is created.

- [ ] **Step 5: Run run/report/session regression tests**

Run: `cargo test -p hoimin-cli --test report_handler && cargo test -p hoimin-cli --test run_e2e`

Expected: PASS.

- [ ] **Step 6: Commit run integration**

```bash
git add crates/hoimin-cli/src/shell.rs crates/hoimin-cli/src/report/human.rs crates/hoimin-cli/tests/report_handler.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "feat: use exact files in run fingerprints"
```

### Task 5: Integrate plan/verify and document the option

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs:100-230`
- Modify: `crates/hoimin-cli/tests/plan.rs:14-220,520-570`
- Modify: `README.md:10-85`

**Interfaces:**
- `PlanManifest.normalized_config.fingerprint_files` is supplied by `PlanConfig` from Task 1.
- `prepare_verify` calls the shared three-argument resolver and compares merged records with `manifest.fingerprint_inputs`.
- Existing version-1 manifests without `fingerprint_files` remain readable.

- [ ] **Step 1: Write failing plan manifest and nested-worktree regression tests**

Change `create_plan_emits_versioned_manifest_without_runtime_side_effects` to pass both selectors:

```rust
"--fingerprint-include", "config.toml",
"--fingerprint-file", "pyproject.toml",
```

Assert:

```rust
assert_eq!(manifest["normalized_config"]["fingerprint_files"], serde_json::json!(["pyproject.toml"]));
assert_eq!(manifest["fingerprint_inputs"].as_array().unwrap().len(), 2);
```

Add a plan/verify regression that creates root and `.worktrees/a/pyproject.toml`, plans with the exact selector, changes the nested file, and expects `prepare_verify` to succeed. Then change the root file and expect `plan.fingerprint_input.changed` before the marker command runs.

- [ ] **Step 2: Write a legacy manifest deserialization test**

Serialize a valid `PlanManifest`, remove `normalized_config.fingerprint_files`, write it, and assert `prepare_verify` accepts it when the remaining sources and fingerprint inputs are unchanged.

Run: `cargo test -p hoimin-cli --test plan -- fingerprint_file`

Expected: FAIL because verify still calls the old resolver interface or ignores exact selectors.

- [ ] **Step 3: Re-resolve both selector kinds in verify**

Replace the current call in `prepare_verify` with:

```rust
let current_inputs = fingerprint_inputs::resolve(
    &config.root,
    &config.fingerprint_includes,
    &config.fingerprint_files,
)
.map_err(|error| PlanError::coded("plan.fingerprint_input.changed", error.to_string()))?;
```

Keep the existing canonical record comparison and `plan.fingerprint_input.changed` behavior. Do not add a `verify` CLI override.

- [ ] **Step 4: Document exact and glob selection separately**

Update the plan example to use:

```console
--fingerprint-file pyproject.toml
```

Document:

- `--fingerprint-file PATH` selects exactly one `--root`-relative regular file and may be repeated.
- `--fingerprint-include GLOB` retains recursive basename matching.
- glob metacharacters in `--fingerprint-file` are literal.
- neither option copies files into workers; use `--include` independently.

Add `--fingerprint-file PATH` to the defaults/options table with default `none`.

- [ ] **Step 5: Run plan, documentation, and full verification**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

Expected: all commands PASS and the worktree contains only Issue #13 changes.

- [ ] **Step 6: Commit plan/verify and documentation**

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs README.md
git commit -m "feat: verify exact fingerprint files"
```

### Task 6: Final behavior and compatibility audit

**Files:**
- Review only: all files changed in Tasks 1–5

**Interfaces:**
- Consumes the completed feature.
- Produces verification evidence and a clean Issue #13 branch ready for review.

- [ ] **Step 1: Reproduce Issue #13 from the built binary**

Create a temporary project containing root `pyproject.toml`, nested `.worktrees/a/pyproject.toml`, a selected Python source, and a passing test. Run `target/debug/hoimin plan` with `--fingerprint-file pyproject.toml`; inspect the JSON and confirm only the root path is present. Modify the nested file and run `verify`; expect success. Modify the root file and run `verify`; expect exit 2 with `plan.fingerprint_input.changed`.

- [ ] **Step 2: Audit compatibility surfaces**

Run:

```bash
git diff main...HEAD -- crates/hoimin-core/src/resume.rs docs/json-schema
git diff main...HEAD -- ':!docs/superpowers' | rg "fingerprint_files|fingerprint-file"
```

Expected: no changes to fingerprint encoding or public schema files; every new config field, call site, report, test, and README entry is visible in the grep output.

- [ ] **Step 3: Confirm branch cleanliness and commits**

Run:

```bash
git status -sb
git log --oneline main..HEAD
```

Expected: clean `issue-13-fingerprint-file` worktree with the design, plan, and focused implementation commits only.
