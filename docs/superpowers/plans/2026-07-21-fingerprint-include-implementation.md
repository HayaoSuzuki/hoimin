# Explicit Fingerprint Inputs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow `run` (and the later `plan` command) to explicitly fingerprint root-relative non-target inputs, so an incomplete SQLite run is resumed only when those inputs are unchanged.

**Architecture:** Keep the user-supplied glob text and the resolved `path`/BLAKE3 records in the normalized core configuration. Resolve the records once, before constructing `ShellContext` or `RunState`, then feed that same immutable list to reporting and the session fingerprint. A dedicated CLI-side resolver owns filesystem/glob semantics; core remains free of filesystem I/O.

**Tech Stack:** Rust 2024 (MSRV 1.85), `clap`, `camino`, `ignore`, `blake3`, `serde`, `tokio`, SQLite via `rusqlite`.

## Global Constraints

- Add `--fingerprint-include GLOB` only to `hoimin run` in this change; its shared option group must be reusable by the following plan/verify implementation.
- Resolve globs relative to `--root`; reject NUL, absolute paths, every `..` component, invalid glob syntax, unmatched patterns, directories, symlinks, non-UTF-8 paths, and read failures with exit 2.
- Preserve the specified glob list for reporting, but sort and deduplicate resolved records by normalized root-relative path.
- Do not change workspace copy include/exclude behavior, discover implicit config files, scan Git status, or migrate SQLite tables.
- Store only BLAKE3 hashes in reports; never serialize input file contents.
- Bump `FINGERPRINT_SCHEMA_VERSION` from 3 to 4 and add the resolved record list as length-prefixed field 8, including an empty list.
- Keep public run-event and run-result schema version 2 unchanged; `normalized_config` already allows additional properties.
- Keep candidate IDs, statuses, scores, exit codes, `progress`, and existing session rows compatible except for the intentional schema-4 fingerprint mismatch.

---

## File Structure

- Modify: `crates/hoimin-core/src/config.rs` — raw/normalized include fields and the serializable `FingerprintInputFile` value object.
- Modify: `crates/hoimin-core/src/resume.rs` — schema-4 fingerprint encoding and canonical record-list encoding.
- Modify: `crates/hoimin-cli/src/cli.rs` — repeated `--fingerprint-include` parsing and raw config conversion.
- Create: `crates/hoimin-cli/src/fingerprint_inputs.rs` — pure root-local glob validation, resolution, regular-file checks, hashing, stable deduplication, and coded errors.
- Modify: `crates/hoimin-cli/src/lib.rs` and `crates/hoimin-cli/src/shell.rs` — resolve records before infrastructure/session creation and use them to build `FingerprintInput`.
- Modify: `crates/hoimin-cli/src/report/human.rs` and `README.md` — make normalized input provenance observable to a human and document its independent copy policy.
- Modify: `crates/hoimin-core/tests/resume_policy.rs`, `crates/hoimin-cli/tests/cli_config.rs`, `crates/hoimin-cli/tests/run_e2e.rs`, and create `crates/hoimin-cli/tests/fingerprint_inputs.rs` — unit, parser, report, and session compatibility coverage.

### Task 1: Add normalized input-record types and schema-4 fingerprinting

**Files:**
- Modify: `crates/hoimin-core/src/config.rs:229-466`
- Modify: `crates/hoimin-core/src/resume.rs:1-213`
- Modify: `crates/hoimin-core/tests/resume_policy.rs:1-220`

**Interfaces:**
- Produces `FingerprintInputFile { path: Utf8PathBuf, hash: String }`, `RawRunConfig.fingerprint_includes`, and `RunConfig.{fingerprint_includes,fingerprint_inputs}`.
- Produces `FingerprintInput.fingerprint_inputs` and `FINGERPRINT_SCHEMA_VERSION == 4` for shell/session callers.

- [ ] **Step 1: Write failing core tests for the added compatibility input**

Add a fixture record and assertions that its ordering does not matter, while its path, hash, addition, and removal all change the digest:

```rust
#[test]
fn fingerprint_inputs_are_canonical_and_compatibility_relevant() {
    let original = fixture_input();
    let mut reordered = original.clone();
    reordered.fingerprint_inputs.reverse();
    assert_eq!(fingerprint(&original), fingerprint(&reordered));

    let mut changed_hash = original.clone();
    changed_hash.fingerprint_inputs[0].hash.replace_range(0..1, "f");
    assert_ne!(fingerprint(&original), fingerprint(&changed_hash));

    let mut changed_path = original.clone();
    changed_path.fingerprint_inputs[0].path = "pyproject.changed.toml".into();
    assert_ne!(fingerprint(&original), fingerprint(&changed_path));

    let mut removed = original.clone();
    removed.fingerprint_inputs.clear();
    assert_ne!(fingerprint(&original), fingerprint(&removed));
}
```

Set `fixture_input().fingerprint_inputs` to two deliberately reverse-order records, for example `fixtures/case.json` and `pyproject.toml`, so canonical ordering is exercised.

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `uv run cargo test -p hoimin-core --test resume_policy fingerprint_inputs_are_canonical_and_compatibility_relevant -- --exact`

Expected: compilation fails because `FingerprintInput` has no `fingerprint_inputs` field.

- [ ] **Step 3: Add the core value object and exact field-8 encoder**

In `config.rs`, add the reusable, JSON-readable record and both configuration fields:

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FingerprintInputFile {
    pub path: Utf8PathBuf,
    pub hash: String,
}

// RawRunConfig
pub fingerprint_includes: Vec<String>,

// RunConfig
pub fingerprint_includes: Vec<String>,
pub fingerprint_inputs: Vec<FingerprintInputFile>,
```

`TryFrom<RawRunConfig>` must move `raw.fingerprint_includes` into `RunConfig` and initialize `fingerprint_inputs` to `Vec::new()`. The CLI resolver populates it later; core conversion must not perform filesystem I/O.

In `resume.rs`, make these exact changes:

```rust
pub const FINGERPRINT_SCHEMA_VERSION: u8 = 4;

pub struct FingerprintInput {
    // existing fields
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
}

encoder.field(8, &encode_fingerprint_inputs(&input.fingerprint_inputs));

fn encode_fingerprint_inputs(inputs: &[FingerprintInputFile]) -> Vec<u8> {
    let mut values = inputs.to_vec();
    values.sort();
    values.dedup();
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(value.path.as_str().as_bytes());
        out.bytes(value.hash.as_bytes());
    }
    out.bytes
}
```

Import `FingerprintInputFile` from `crate`. Use the already-normalized lower-case hexadecimal BLAKE3 string as the framed hash value; it is stable, human-readable in reports, and includes no file content.

- [ ] **Step 4: Run core regression tests**

Run: `uv run cargo test -p hoimin-core --test resume_policy`

Expected: PASS, including existing source/profile/resource-mode/safety-limit fingerprint tests and the new field-8 cases.

- [ ] **Step 5: Commit the core contract**

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/src/resume.rs crates/hoimin-core/tests/resume_policy.rs
git commit -m "feat: fingerprint explicit input files"
```

### Task 2: Parse repeated include globs without changing existing run options

**Files:**
- Modify: `crates/hoimin-cli/src/cli.rs:68-470`
- Modify: `crates/hoimin-cli/tests/cli_config.rs:1-370`

**Interfaces:**
- Consumes `RawRunConfig.fingerprint_includes` from Task 1.
- Produces `RunArgs.fingerprint_includes: Vec<String>` and a normalized `RunConfig` that retains the exact supplied pattern list.

- [ ] **Step 1: Write parser tests for repeated values and unchanged copy options**

Add this test to `cli_config.rs`:

```rust
#[test]
fn run_preserves_repeated_fingerprint_include_patterns() {
    let config = hoimin_cli::cli::parse_config_from([
        "hoimin", "run", "--file", "src/calc.py",
        "--fingerprint-include", "pyproject.toml",
        "--fingerprint-include", "fixtures/**/*.json",
        "--include", "fixtures/**", "--", "python", "-m", "pytest",
    ]).unwrap();

    assert_eq!(config.fingerprint_includes, ["pyproject.toml", "fixtures/**/*.json"]);
    assert_eq!(config.selection.includes, ["fixtures/**"]);
    assert!(config.fingerprint_inputs.is_empty());
}
```

- [ ] **Step 2: Run the parser test to verify it fails**

Run: `uv run cargo test -p hoimin-cli --test cli_config run_preserves_repeated_fingerprint_include_patterns -- --exact`

Expected: FAIL because Clap rejects `--fingerprint-include`.

- [ ] **Step 3: Thread the new raw option through CLI conversion**

Add this field beside `include`/`exclude` in both `RawRunArgs` and public `RunArgs`, and pass it to `RawRunConfig`:

```rust
/// Add a root-relative file glob to the session fingerprint; may be repeated.
#[arg(long, value_name = "GLOB")]
fingerprint_include: Vec<String>,

// in RunArgs
pub fingerprint_includes: Vec<String>,

// in RawRunConfig construction
fingerprint_includes: args.fingerprint_includes,
```

In the `Command::Run` conversion, map `raw.fingerprint_include` to `RunArgs.fingerprint_includes`. Do not add this field to `Selection`: it must never affect copy or target discovery.

- [ ] **Step 4: Run parser and formatting checks**

Run: `uv run cargo test -p hoimin-cli --test cli_config && uv run cargo fmt --check`

Expected: PASS.

- [ ] **Step 5: Commit parser support**

```bash
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: parse fingerprint include globs"
```

### Task 3: Implement a root-local, side-effect-free resolver

**Files:**
- Create: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Modify: `crates/hoimin-cli/src/lib.rs:1-12`
- Create: `crates/hoimin-cli/tests/fingerprint_inputs.rs`

**Interfaces:**
- Produces `resolve(root: &Utf8Path, patterns: &[String]) -> Result<Vec<FingerprintInputFile>, FingerprintInputError>`.
- Error `Display` begins with exactly one of `fingerprint.include.invalid_glob`, `fingerprint.include.unmatched`, or `fingerprint.include.unsupported_file`.

- [ ] **Step 1: Write resolver tests before implementation**

Create fixture files under a temp root and cover sorted/deduplicated matches plus each disallowed input:

```rust
#[test]
fn resolve_sorts_and_deduplicates_matching_regular_files() {
    let root = fixture_root(&[("pyproject.toml", "a"), ("fixtures/b.json", "b")]);
    let records = resolve(&root, &["fixtures/*.json".into(), "**/*.toml".into(), "fixtures/*.json".into()]).unwrap();
    assert_eq!(records.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["fixtures/b.json", "pyproject.toml"]);
    assert_eq!(records.len(), 2);
}

#[test]
fn resolve_rejects_unmatched_unsafe_and_non_regular_inputs() {
    let root = fixture_root(&[("file.txt", "x")]);
    std::fs::create_dir(root.join("dir")).unwrap();
    for pattern in ["missing/*.json", "/tmp/x", "../x", "dir"] {
        assert!(resolve(&root, &[pattern.into()]).is_err(), "{pattern}");
    }
}
```

On Unix, add a `std::os::unix::fs::symlink("file.txt", root.join("link.txt"))` case and assert its error code is `fingerprint.include.unsupported_file`.

- [ ] **Step 2: Run resolver tests to verify the module is missing**

Run: `uv run cargo test -p hoimin-cli --test fingerprint_inputs`

Expected: compilation fails because module `fingerprint_inputs` does not exist.

- [ ] **Step 3: Implement exact validation, matching, and hashing behavior**

Expose the resolver from `lib.rs` as `pub mod fingerprint_inputs;`. In the new module, use `ignore::overrides::OverrideBuilder` and a walk with every ignore layer disabled, so explicitly named ignored files are still considered. Implement the public error shape:

```rust
#[derive(Debug, thiserror::Error)]
pub enum FingerprintInputError {
    #[error("fingerprint.include.invalid_glob: {0}")]
    InvalidGlob(String),
    #[error("fingerprint.include.unmatched: {0}")]
    Unmatched(String),
    #[error("fingerprint.include.unsupported_file: {0}")]
    UnsupportedFile(String),
}
```

Use this sequence in `resolve`:

```rust
pub fn resolve(root: &Utf8Path, patterns: &[String]) -> Result<Vec<FingerprintInputFile>, FingerprintInputError> {
    let mut records = BTreeMap::new();
    for pattern in patterns {
        validate_pattern(pattern)?;
        let matched = resolve_one(root, pattern)?;
        if matched.is_empty() {
            return Err(FingerprintInputError::Unmatched(pattern.clone()));
        }
        for path in matched {
            let bytes = std::fs::read(root.join(&path)).map_err(|e| FingerprintInputError::UnsupportedFile(e.to_string()))?;
            records.insert(path.clone(), FingerprintInputFile { path, hash: blake3::hash(&bytes).to_hex().to_string() });
        }
    }
    Ok(records.into_values().collect())
}
```

`validate_pattern` rejects `pattern.contains('\0')`, `Utf8Path::new(pattern).is_absolute()`, and any slash- or backslash-separated component equal to `".."`; it maps `OverrideBuilder::add`/`build` failures to `InvalidGlob`. `resolve_one` must reject an entry whose `file_type` is a directory or symlink rather than silently skipping it, convert each selected relative path through `Utf8PathBuf::from_path_buf`, ensure `strip_prefix(root)` succeeds, and accept only `file_type.is_file()`. Configure the walker with `.hidden(false).ignore(false).git_ignore(false).git_global(false).git_exclude(false).parents(false).follow_links(false)`.

- [ ] **Step 4: Run resolver tests and the CLI package suite**

Run: `uv run cargo test -p hoimin-cli --test fingerprint_inputs && uv run cargo test -p hoimin-cli --test cli_config`

Expected: PASS; the resolver tests prove no filesystem write or workspace-copy behavior is introduced.

- [ ] **Step 5: Commit resolver behavior**

```bash
git add crates/hoimin-cli/src/lib.rs crates/hoimin-cli/src/fingerprint_inputs.rs crates/hoimin-cli/tests/fingerprint_inputs.rs
git commit -m "feat: resolve fingerprint input globs"
```

### Task 4: Resolve before shell/session initialization and fingerprint the records

**Files:**
- Modify: `crates/hoimin-cli/src/lib.rs:28-68`
- Modify: `crates/hoimin-cli/src/shell.rs:179-221`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:150-280`
- Modify: `crates/hoimin-cli/tests/session_handler.rs:1-70`

**Interfaces:**
- Consumes `fingerprint_inputs::resolve` and `RunConfig.fingerprint_includes`.
- Produces `prepare_run_config(config) -> Result<RunConfig, FingerprintInputError>` before `ShellContext::new`, and includes `config.fingerprint_inputs.clone()` in `FingerprintInput`.

- [ ] **Step 1: Write startup-order and session-compatibility tests**

Add an E2E test that passes an unmatched pattern with `--session PATH`, then asserts exit 2, `stderr` contains `fingerprint.include.unmatched`, and `PATH` does not exist. Add a second test that runs with `--session --resume --fingerprint-include watched.toml`, marks the stored run incomplete, changes `watched.toml`, and asserts the next invocation creates a distinct run rather than reusing results.

Use this acceptance assertion for the early failure:

```rust
assert_eq!(run.exit_code, 2);
assert!(run.stderr.contains("fingerprint.include.unmatched"));
assert!(!database.exists());
```

- [ ] **Step 2: Run the new E2E tests to verify the failure mode**

Run: `uv run cargo test -p hoimin-cli --test run_e2e fingerprint_include -- --nocapture`

Expected: FAIL because no early resolver is invoked and the old schema-3 fingerprint is reused.

- [ ] **Step 3: Populate the config exactly once before run infrastructure exists**

Add this helper to `shell.rs` and call it at the beginning of both `run_loop` and the control variant's shared path, before `ShellContext::new`:

```rust
pub fn prepare_run_config(mut config: RunConfig) -> Result<RunConfig, crate::fingerprint_inputs::FingerprintInputError> {
    config.fingerprint_inputs = crate::fingerprint_inputs::resolve(
        &config.root,
        &config.fingerprint_includes,
    )?;
    Ok(config)
}
```

Avoid resolving twice: make `run_loop` delegate to a private `run_loop_prepared`, and have `run_loop_with_control` call `prepare_run_config` once before the private function. Tests that deliberately construct a `ShellContext` directly keep their existing no-project-I/O contract.

In `prepare_fingerprint`, add the field without recalculating hashes:

```rust
fingerprint_inputs: context.config.fingerprint_inputs.clone(),
```

This places validation before temporary spool creation, resource probing, workspace copy, baseline, and lazy SQLite opening.

- [ ] **Step 4: Run focused and regression tests**

Run: `uv run cargo test -p hoimin-cli --test run_e2e fingerprint_include && uv run cargo test -p hoimin-cli --test session_handler && uv run cargo test -p hoimin-core --test resume_policy`

Expected: PASS. The changed watched input creates a schema-4 fingerprint row, while an unlisted file change leaves the fingerprint unchanged.

- [ ] **Step 5: Commit startup integration**

```bash
git add crates/hoimin-cli/src/lib.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/run_e2e.rs crates/hoimin-cli/tests/session_handler.rs
git commit -m "feat: include resolved files in run fingerprints"
```

### Task 5: Expose the provenance without changing report schemas

**Files:**
- Modify: `crates/hoimin-cli/src/report/human.rs:6-21`
- Modify: `crates/hoimin-cli/tests/report_handler.rs:875-920`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:330-410`
- Modify: `README.md:48-105,148-165`

**Interfaces:**
- Consumes serialized `RunConfig.fingerprint_includes` and `RunConfig.fingerprint_inputs`.
- Produces JSON/JSONL values through the unchanged `normalized_config` extension point and one stable human provenance line containing patterns plus path/hash records.

- [ ] **Step 1: Add report expectations before rendering changes**

Add a JSON/JSONL E2E assertion and a human report-handler assertion:

```rust
assert_eq!(run.document["run"]["normalized_config"]["fingerprint_includes"], serde_json::json!(["pyproject.toml"]));
assert_eq!(run.document["run"]["normalized_config"]["fingerprint_inputs"][0]["path"], "pyproject.toml");
assert!(human.contains("fingerprint includes: [pyproject.toml]"));
assert!(human.contains("fingerprint inputs: [pyproject.toml="));
```

- [ ] **Step 2: Run them to verify the human output is incomplete**

Run: `uv run cargo test -p hoimin-cli --test report_handler fingerprint && uv run cargo test -p hoimin-cli --test run_e2e fingerprint_include_is_reported -- --exact`

Expected: the JSON assertion passes after Task 4 serialization; the human assertion fails until this task changes the renderer.

- [ ] **Step 3: Render only the count in human output and document exact semantics**

Keep the existing run-start line exactly, then emit a second provenance line when at least one pattern was supplied:

```rust
let patterns = config.fingerprint_includes.join(", ");
let inputs = config.fingerprint_inputs.iter()
    .map(|input| format!("{}={}", input.path, input.hash))
    .collect::<Vec<_>>()
    .join(", ");
writeln!(writer, "run started: {} (profile: {})", value.run_id, config.profile.as_str())?;
if !config.fingerprint_includes.is_empty() {
    writeln!(writer, "fingerprint includes: [{patterns}]")?;
    writeln!(writer, "fingerprint inputs: [{inputs}]")?;
}
```

This deliberately retains the existing `(profile: focused)` line while making both declared patterns and resolved path/hash records visible. Add a README option-table row for `--fingerprint-include GLOB`, an invocation example that includes both `--fingerprint-include` and `--include` when the test needs copied fixture data, and text stating that the former only invalidates reuse while the latter controls worker copy. State that no files are implicitly watched.

- [ ] **Step 4: Run report schema and documentation regressions**

Run: `uv run cargo test -p hoimin-cli --test report_handler && uv run cargo test -p hoimin-cli --test run_e2e readme_documents_focused_profile_selection_and_session_compatibility -- --exact`

Expected: PASS. Do not edit either JSON schema file or `REPORT_SCHEMA_VERSION`.

- [ ] **Step 5: Commit observable behavior**

```bash
git add crates/hoimin-cli/src/report/human.rs crates/hoimin-cli/tests/report_handler.rs crates/hoimin-cli/tests/run_e2e.rs README.md
git commit -m "docs: explain explicit fingerprint inputs"
```

### Task 6: Run the complete quality gate

**Files:**
- Verify: all files touched by Tasks 1-5

- [ ] **Step 1: Format and lint the workspace**

Run: `uv run cargo fmt --check && uv run cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS with the workspace's `clippy::all` and `clippy::pedantic` deny policy.

- [ ] **Step 2: Run the full test suite**

Run: `uv run cargo test --workspace`

Expected: PASS, including report-schema validators, progress parsing, machine contracts, resolver tests, session tests, and E2E runs.

- [ ] **Step 3: Inspect the final diff and commit only verified changes**

Run: `git diff --check HEAD^..HEAD && git status --short`

Expected: no whitespace errors; do not stage unrelated pre-existing files such as `.idea/`.
