use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{EffectFailed, EffectId, LineRange, TargetError, normalize_changed};
use tokio::process::Command;

use crate::portable_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveGitChanges {
    pub id: EffectId,
    pub root: Utf8PathBuf,
    pub diff_base: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitChangesResolved {
    pub id: EffectId,
    pub changed: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
}

/// Resolves changed Python lines from Git into target-change data.
///
/// # Errors
///
/// Returns an effect failure when Git inspection cannot resolve changed files.
pub async fn handle_git(request: ResolveGitChanges) -> Result<GitChangesResolved, EffectFailed> {
    let id = request.id;
    resolve_changed(&request.root, request.diff_base.as_deref())
        .await
        .map(|changed| GitChangesResolved { id, changed })
        .map_err(|error| EffectFailed::other(id, "target.git", error.to_string()))
}

pub(crate) async fn resolve_changed(
    root: &Utf8Path,
    diff_base: Option<&str>,
) -> Result<BTreeMap<Utf8PathBuf, Vec<LineRange>>, TargetError> {
    ensure_git_worktree(root).await?;
    let mut changed = BTreeMap::<Utf8PathBuf, Vec<LineRange>>::new();
    let mut excluded = BTreeSet::new();
    let mut diff_args = vec![
        "diff",
        "--unified=0",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--find-renames",
        "--inter-hunk-context=0",
        "--diff-algorithm=myers",
        "--no-indent-heuristic",
        "-l0",
        "--relative",
    ];
    let mut numstat_args = vec![
        "diff",
        "--numstat",
        "-z",
        "--no-ext-diff",
        "--no-textconv",
        "--find-renames",
        "-l0",
        "--relative",
    ];
    if let Some(base) = diff_base {
        let base = resolve_commit(root, base).await?;
        diff_args.push("--merge-base");
        diff_args.push(&base);
        numstat_args.push("--merge-base");
        numstat_args.push(&base);
        let output = run_git(root, &diff_args).await?;
        parse_diff(&output, &mut changed, &mut excluded)?;
        let output = run_git(root, &numstat_args).await?;
        parse_binary_numstat(&output, &mut excluded)?;
    } else if head_exists(root).await? {
        diff_args.push("HEAD");
        numstat_args.push("HEAD");
        let output = run_git(root, &diff_args).await?;
        parse_diff(&output, &mut changed, &mut excluded)?;
        let output = run_git(root, &numstat_args).await?;
        parse_binary_numstat(&output, &mut excluded)?;
    } else {
        let indexed = run_git(root, &["ls-files", "-z"]).await?;
        collect_current_worktree_paths(root, &indexed, &mut changed).await?;
    }
    changed.retain(|path, _| !excluded.contains(path));

    let untracked = run_git(root, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    collect_current_worktree_paths(root, &untracked, &mut changed).await?;
    Ok(normalize_changed(changed))
}

async fn resolve_commit(root: &Utf8Path, revision: &str) -> Result<String, TargetError> {
    let revision = format!("{revision}^{{commit}}");
    let output = run_git(
        root,
        &["rev-parse", "--verify", "--end-of-options", &revision],
    )
    .await?;
    let oid = std::str::from_utf8(output.trim_ascii())
        .map_err(|_| TargetError::GitFailed("Git commit ID is not valid UTF-8".into()))?;
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(oid.to_owned())
    } else {
        Err(TargetError::GitFailed(
            "git rev-parse returned an invalid commit ID".into(),
        ))
    }
}

async fn head_exists(root: &Utf8Path) -> Result<bool, TargetError> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .current_dir(root)
        .output()
        .await
        .map_err(|error| TargetError::GitFailed(error.to_string()))?;
    Ok(output.status.success())
}

async fn ensure_git_worktree(root: &Utf8Path) -> Result<(), TargetError> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .await
        .map_err(|error| TargetError::GitFailed(error.to_string()))?;
    if output.status.success() && output.stdout.trim_ascii() == b"true" {
        Ok(())
    } else {
        Err(TargetError::GitRepositoryRequired)
    }
}

async fn run_git(root: &Utf8Path, args: &[&str]) -> Result<Vec<u8>, TargetError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await
        .map_err(|error| TargetError::GitFailed(error.to_string()))?;
    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(TargetError::GitFailed(format!(
        "git {} exited with {}: {stderr}",
        args.join(" "),
        output.status
    )))
}

fn parse_diff(
    output: &[u8],
    changed: &mut BTreeMap<Utf8PathBuf, Vec<LineRange>>,
    excluded: &mut BTreeSet<Utf8PathBuf>,
) -> Result<(), TargetError> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum PatchState {
        OutsideSection,
        AwaitingOldHeader,
        AwaitingNewHeader,
        Body,
    }

    let mut old_path = None;
    let mut path = None;
    let mut state = PatchState::OutsideSection;
    for line in output.split(|byte| *byte == b'\n') {
        if line.starts_with(b"diff --git ") {
            old_path = None;
            path = None;
            state = PatchState::AwaitingOldHeader;
        } else if state == PatchState::AwaitingOldHeader
            && let Some(raw_path) = line.strip_prefix(b"--- ")
        {
            let raw_path = std::str::from_utf8(raw_path)
                .map_err(|_| TargetError::GitFailed("Git diff path is not valid UTF-8".into()))?;
            old_path = parse_patch_path(raw_path)?;
            state = PatchState::AwaitingNewHeader;
        } else if state == PatchState::AwaitingNewHeader
            && let Some(raw_path) = line.strip_prefix(b"+++ ")
        {
            let raw_path = std::str::from_utf8(raw_path)
                .map_err(|_| TargetError::GitFailed("Git diff path is not valid UTF-8".into()))?;
            path = parse_patch_path(raw_path)?;
            if path.is_none()
                && let Some(old_path) = &old_path
                && is_python(old_path)
            {
                excluded.insert(old_path.clone());
            }
            state = PatchState::Body;
        } else if state == PatchState::Body && line.starts_with(b"@@") {
            let line = std::str::from_utf8(line)
                .map_err(|_| TargetError::GitFailed("invalid Git hunk header".into()))?;
            let range = parse_hunk_range(line)?;
            if let (Some(path), Some(range)) = (&path, range)
                && is_python(path)
            {
                changed.entry(path.clone()).or_default().push(range);
            }
        }
    }
    Ok(())
}

fn parse_binary_numstat(
    output: &[u8],
    excluded: &mut BTreeSet<Utf8PathBuf>,
) -> Result<(), TargetError> {
    let mut records = output.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let added = fields.next();
        let deleted = fields.next();
        let path = fields
            .next()
            .ok_or_else(|| TargetError::GitFailed("invalid Git numstat binary record".into()))?;
        let binary = added == Some(b"-".as_slice()) && deleted == Some(b"-".as_slice());
        if path.is_empty() {
            let old_path = records.next().ok_or_else(|| {
                TargetError::GitFailed("invalid Git numstat rename record".into())
            })?;
            let new_path = records.next().ok_or_else(|| {
                TargetError::GitFailed("invalid Git numstat rename record".into())
            })?;
            if old_path.is_empty() || new_path.is_empty() {
                return Err(TargetError::GitFailed(
                    "invalid Git numstat rename record".into(),
                ));
            }
            if binary {
                insert_binary_numstat_path(old_path, excluded)?;
                insert_binary_numstat_path(new_path, excluded)?;
            }
        } else if binary {
            insert_binary_numstat_path(path, excluded)?;
        }
    }
    Ok(())
}

fn insert_binary_numstat_path(
    raw_path: &[u8],
    excluded: &mut BTreeSet<Utf8PathBuf>,
) -> Result<(), TargetError> {
    let path = std::str::from_utf8(raw_path)
        .map_err(|_| TargetError::GitFailed("Git numstat path is not valid UTF-8".into()))?;
    let path = portable_path::from_git(path)
        .map_err(|error| TargetError::GitFailed(error.to_string()))?;
    let path = Utf8PathBuf::from(path);
    if is_python(&path) {
        excluded.insert(path);
    }
    Ok(())
}

fn parse_patch_path(value: &str) -> Result<Option<Utf8PathBuf>, TargetError> {
    let value = value.split_once('\t').map_or(value, |(path, _)| path);
    let decoded = if value.starts_with('"') {
        decode_git_quoted(value)?
    } else {
        value.to_owned()
    };
    if decoded == "/dev/null" {
        return Ok(None);
    }
    let relative = decoded
        .strip_prefix("a/")
        .or_else(|| decoded.strip_prefix("b/"))
        .unwrap_or(&decoded);
    let relative = portable_path::from_git(relative)
        .map_err(|error| TargetError::GitFailed(error.to_string()))?;
    Ok(Some(Utf8PathBuf::from(relative)))
}

fn decode_git_quoted(value: &str) -> Result<String, TargetError> {
    let Some(value) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(TargetError::GitFailed("invalid quoted Git path".into()));
    };
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        let Some(&escaped) = bytes.get(index) else {
            return Err(TargetError::GitFailed("invalid Git path escape".into()));
        };
        match escaped {
            b'\\' | b'"' => decoded.push(escaped),
            b'a' => decoded.push(0x07),
            b'b' => decoded.push(0x08),
            b't' => decoded.push(b'\t'),
            b'n' => decoded.push(b'\n'),
            b'v' => decoded.push(0x0b),
            b'f' => decoded.push(0x0c),
            b'r' => decoded.push(b'\r'),
            b'0'..=b'7' => {
                let end = (index + 3).min(bytes.len());
                let digits = &bytes[index..end];
                if digits.len() != 3 || !digits.iter().all(|digit| matches!(digit, b'0'..=b'7')) {
                    return Err(TargetError::GitFailed(
                        "invalid octal Git path escape".into(),
                    ));
                }
                let value = digits
                    .iter()
                    .fold(0_u16, |value, digit| value * 8 + u16::from(digit - b'0'));
                let value = u8::try_from(value)
                    .map_err(|_| TargetError::GitFailed("invalid octal Git path escape".into()))?;
                decoded.push(value);
                index += 2;
            }
            _ => return Err(TargetError::GitFailed("invalid Git path escape".into())),
        }
        index += 1;
    }
    String::from_utf8(decoded)
        .map_err(|_| TargetError::GitFailed("Git diff path is not valid UTF-8".into()))
}

fn parse_hunk_range(line: &str) -> Result<Option<LineRange>, TargetError> {
    let Some(value) = line.split_whitespace().find(|value| value.starts_with('+')) else {
        return Err(TargetError::GitFailed("invalid Git hunk header".into()));
    };
    let value = &value[1..];
    let (start, count) = value
        .split_once(',')
        .map_or((value, "1"), |(start, count)| (start, count));
    let start = start
        .parse::<u32>()
        .map_err(|_| TargetError::GitFailed("invalid Git hunk start".into()))?;
    let count = count
        .parse::<u32>()
        .map_err(|_| TargetError::GitFailed("invalid Git hunk count".into()))?;
    if count == 0 {
        return Ok(None);
    }
    let end = start
        .checked_add(count - 1)
        .ok_or_else(|| TargetError::GitFailed("Git hunk line range overflow".into()))?;
    Ok(Some(LineRange { start, end }))
}

async fn collect_current_worktree_paths(
    root: &Utf8Path,
    output: &[u8],
    changed: &mut BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> Result<(), TargetError> {
    for raw_path in output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(raw_path)
            .map_err(|_| TargetError::GitFailed("Git path is not valid UTF-8".into()))?;
        let path = portable_path::from_git(path)
            .map_err(|error| TargetError::GitFailed(error.to_string()))?;
        let path = Utf8PathBuf::from(path);
        if !is_python(&path) {
            continue;
        }
        let contents = match tokio::fs::read(root.join(&path)).await {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(TargetError::GitFailed(error.to_string())),
        };
        if contents.contains(&0) || contents.is_empty() {
            continue;
        }
        let mut line_count = usize::from(!contents.ends_with(b"\n"));
        for byte in &contents {
            line_count += usize::from(*byte == b'\n');
        }
        let end = u32::try_from(line_count)
            .map_err(|_| TargetError::GitFailed("Python file has too many lines".into()))?;
        changed
            .entry(path)
            .or_default()
            .push(LineRange { start: 1, end });
    }
    Ok(())
}

fn is_python(path: &Utf8Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fmt::Write as _;

    use camino::Utf8PathBuf;
    use hoimin_core::LineRange;
    use proptest::prelude::*;

    use super::{
        collect_current_worktree_paths, decode_git_quoted, parse_binary_numstat, parse_diff,
    };

    #[derive(Clone, Debug)]
    struct GeneratedHunk {
        old_start: u32,
        new_start: u32,
        extra_lines: Vec<(bool, String)>,
    }

    impl GeneratedHunk {
        fn old_len(&self) -> u32 {
            let generated = self.extra_lines.iter().filter(|(added, _)| !added).count();
            1 + u32::try_from(generated).expect("the generator emits at most five extra lines")
        }

        fn new_len(&self) -> u32 {
            let generated = self.extra_lines.iter().filter(|(added, _)| *added).count();
            3 + u32::try_from(generated).expect("the generator emits at most five extra lines")
        }
    }

    #[derive(Clone, Debug)]
    struct GeneratedDiffSection {
        destination: Utf8PathBuf,
        hunks: Vec<GeneratedHunk>,
    }

    fn generated_diff_sections() -> impl Strategy<Value = Vec<GeneratedDiffSection>> {
        let ordinary_line = prop::string::string_regex("[ -~]{0,24}").unwrap();
        let hunk = (
            1_u32..10_000,
            1_u32..10_000,
            prop::collection::vec((any::<bool>(), ordinary_line), 0..6),
        )
            .prop_map(|(old_start, new_start, extra_lines)| GeneratedHunk {
                old_start,
                new_start,
                extra_lines,
            });

        prop::collection::vec(prop::collection::vec(hunk, 2..6), 1..5).prop_map(|sections| {
            sections
                .into_iter()
                .enumerate()
                .map(|(index, hunks)| GeneratedDiffSection {
                    destination: Utf8PathBuf::from(format!("generated/case-{index}.py")),
                    hunks,
                })
                .collect()
        })
    }

    fn render_unified0_diff(sections: &[GeneratedDiffSection]) -> String {
        let mut rendered = String::new();
        for section in sections {
            let path = section.destination.as_str();
            let _ = writeln!(rendered, "diff --git a/{path} b/{path}");
            let _ = writeln!(rendered, "--- a/{path}");
            let _ = writeln!(rendered, "+++ b/{path}");
            for hunk in &section.hunks {
                let _ = writeln!(
                    rendered,
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start,
                    hunk.old_len(),
                    hunk.new_start,
                    hunk.new_len()
                );
                // These source lines become patch lines that resemble every structural
                // header which previously confused the parser.
                rendered.push_str("+++ b/evil.py\n");
                rendered.push_str("--- a/evil.py\n");
                rendered.push_str("+@@ -1 +1 @@\n");
                rendered.push_str("+Binary files a/x.py and b/y.py differ\n");
                for (added, line) in &hunk.extra_lines {
                    rendered.push(if *added { '+' } else { '-' });
                    rendered.push_str(line);
                    rendered.push('\n');
                }
            }
        }
        rendered
    }

    fn expected_ranges(sections: &[GeneratedDiffSection]) -> BTreeMap<Utf8PathBuf, Vec<LineRange>> {
        sections
            .iter()
            .map(|section| {
                let ranges = section
                    .hunks
                    .iter()
                    .map(|hunk| LineRange {
                        start: hunk.new_start,
                        end: hunk.new_start + hunk.new_len() - 1,
                    })
                    .collect();
                (section.destination.clone(), ranges)
            })
            .collect()
    }

    fn arbitrary_representable_utf8_git_path() -> impl Strategy<Value = String> {
        prop::collection::vec(
            any::<char>().prop_filter("Git paths cannot contain NUL", |ch| *ch != '\0'),
            0..24,
        )
        .prop_map(|prefix| {
            let mut path = prefix.into_iter().collect::<String>();
            path.push_str("/space tab\tquote\"slash\\snowman-雪.py");
            path
        })
    }

    fn git_c_quote(path: &str) -> String {
        let mut quoted = String::from("\"");
        for byte in path.bytes() {
            match byte {
                b'\\' => quoted.push_str("\\\\"),
                b'"' => quoted.push_str("\\\""),
                b'\t' => quoted.push_str("\\t"),
                b'\n' => quoted.push_str("\\n"),
                b'\r' => quoted.push_str("\\r"),
                0x20..=0x7e => quoted.push(char::from(byte)),
                _ => {
                    let _ = write!(quoted, "\\{byte:03o}");
                }
            }
        }
        quoted.push('"');
        quoted
    }

    #[test]
    fn quoted_path_accepts_standard_control_escapes() {
        assert_eq!(
            decode_git_quoted(r#""a/\a\b\v\f.py""#).unwrap(),
            "a/\x07\x08\x0b\x0c.py"
        );
    }

    #[test]
    fn quoted_path_rejects_out_of_range_octal_escapes() {
        for path in [r#""a/\400.py""#, r#""a/\777.py""#] {
            let error = decode_git_quoted(path).unwrap_err();

            assert!(error.to_string().contains("invalid octal Git path escape"));
        }
    }

    #[test]
    fn quoted_path_accepts_maximum_octal_byte_before_utf8_validation() {
        let error = decode_git_quoted(r#""a/\377.py""#).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Git diff path is not valid UTF-8")
        );
    }

    #[test]
    fn lean_changed_target_parser_isolation_fixture() {
        let diff = b"diff --git a/bad.txt b/bad.txt\n--- a/bad.txt\n+++ b/bad.txt\n@@ -1 +1 @@\n+++ b/decoy.py\n@@ -9 +9 @@\ndiff --git a/pkg/good.py b/pkg/good.py\n--- a/pkg/good.py\n+++ b/pkg/good.py\n@@ -2,0 +3 @@\n+value = 1 + 2\n";
        let mut changed = BTreeMap::new();
        let mut excluded = BTreeSet::new();

        parse_diff(diff, &mut changed, &mut excluded).unwrap();

        assert_eq!(
            changed,
            BTreeMap::from([(
                Utf8PathBuf::from("pkg/good.py"),
                vec![LineRange { start: 3, end: 3 }],
            )])
        );
        assert!(excluded.is_empty());
    }

    proptest! {
        #[test]
        fn generated_hostile_zero_context_diffs_match_ranges(
            sections in generated_diff_sections(),
        ) {
            let rendered = render_unified0_diff(&sections);
            let expected = expected_ranges(&sections);
            let mut changed = BTreeMap::new();
            let mut excluded = BTreeSet::new();

            parse_diff(rendered.as_bytes(), &mut changed, &mut excluded)?;

            // pins: issue #101
            prop_assert_eq!(changed, expected);
            prop_assert!(excluded.is_empty());
        }

        #[test]
        fn representable_utf8_git_c_quote_round_trips(
            path in arbitrary_representable_utf8_git_path(),
        ) {
            prop_assert_eq!(decode_git_quoted(&git_c_quote(&path))?, path);
        }

        #[test]
        fn quoted_path_decoding_is_total(path in any::<String>()) {
            let quoted = format!("\"{path}\"");

            let _ = decode_git_quoted(&quoted);
        }
    }

    #[test]
    fn binary_numstat_collects_both_rename_paths() {
        let mut excluded = BTreeSet::new();

        parse_binary_numstat(b"-\t-\t\0old and name.py\0new and name.py\0", &mut excluded).unwrap();

        assert_eq!(
            excluded,
            BTreeSet::from([
                Utf8PathBuf::from("new and name.py"),
                Utf8PathBuf::from("old and name.py"),
            ])
        );
    }

    #[test]
    fn binary_numstat_rejects_an_incomplete_rename_record() {
        let error = parse_binary_numstat(b"-\t-\t\0old.py\0", &mut BTreeSet::new()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("invalid Git numstat rename record")
        );
    }

    #[test]
    fn binary_numstat_rejects_a_literal_backslash_path() {
        let error = parse_binary_numstat(
            b"-\t-\tliteral\\binary.py\0",
            &mut BTreeSet::new(),
        )
        .unwrap_err();

        assert!(error.to_string().contains(r"literal\binary.py"), "{error}");
    }

    #[test]
    fn quoted_patch_header_rejects_a_decoded_literal_backslash_path() {
        let diff = b"diff --git ignored ignored\n--- \"a/literal\\\\calc.py\"\n+++ \"b/literal\\\\calc.py\"\n@@ -1 +1 @@\n";

        let error = parse_diff(diff, &mut BTreeMap::new(), &mut BTreeSet::new()).unwrap_err();

        assert!(error.to_string().contains(r"literal\calc.py"), "{error}");
    }

    #[tokio::test]
    async fn current_worktree_record_rejects_a_literal_backslash_path() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(r"literal\work.py"), "x = 1\n").unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();

        let error = collect_current_worktree_paths(
            &root,
            b"literal\\work.py\0",
            &mut BTreeMap::new(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains(r"literal\work.py"), "{error}");
    }
}
