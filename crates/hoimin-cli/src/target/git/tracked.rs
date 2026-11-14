use std::collections::{BTreeMap, BTreeSet};

use super::{GitError, GitRunner};
use crate::process::ProcessCancellation;
use camino::Utf8Path;
use hoimin_core::{TargetError, normalize_changed};

use super::{GitChangedRanges, GitPathScope, parse_binary_numstat, parse_diff, run_git};

const MAX_PATHS: usize = 128;
const MAX_PATH_BYTES: usize = 8192;

#[derive(Debug, Eq, PartialEq)]
enum DiffPlan {
    Legacy,
    Scoped(Vec<Vec<String>>),
}

pub(super) async fn collect(
    root: &Utf8Path,
    patch_args: &[&str],
    numstat_args: &[&str],
    scope: &GitPathScope,
    scoped: bool,
    runner: &GitRunner,
    cancellation: &ProcessCancellation,
) -> Result<GitChangedRanges, GitError> {
    let plan = if scoped {
        let mut args: Vec<_> = patch_args
            .iter()
            .copied()
            .filter(|arg| !arg.starts_with("--unified="))
            .collect();
        args.splice(1..1, ["--name-status", "-z"]);
        inventory_plan(&run_git(root, &args, runner, cancellation).await?, scope)?
    } else {
        DiffPlan::Legacy
    };
    let mut changed = BTreeMap::new();
    let mut excluded = BTreeSet::new();
    match plan {
        DiffPlan::Legacy => {
            parse_diff(
                &run_git(root, patch_args, runner, cancellation).await?,
                &mut changed,
                &mut excluded,
                scope,
            )?;
            parse_binary_numstat(
                &run_git(root, numstat_args, runner, cancellation).await?,
                &mut excluded,
                scope,
            )?;
        }
        DiffPlan::Scoped(batches) => {
            for paths in batches {
                let patch = scoped_args(patch_args, &paths);
                parse_diff(
                    &run_git(root, &patch, runner, cancellation).await?,
                    &mut changed,
                    &mut excluded,
                    scope,
                )?;
                let numstat = scoped_args(numstat_args, &paths);
                parse_binary_numstat(
                    &run_git(root, &numstat, runner, cancellation).await?,
                    &mut excluded,
                    scope,
                )?;
            }
        }
    }
    changed.retain(|path, _| !excluded.contains(path));
    Ok(GitChangedRanges(normalize_changed(changed)))
}

fn scoped_args<'a>(args: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut scoped = vec!["--literal-pathspecs"];
    scoped.extend(args.iter().map(|arg| {
        if *arg == "--find-renames" {
            "--no-renames"
        } else {
            *arg
        }
    }));
    scoped.push("--");
    scoped.extend(paths.iter().map(String::as_str));
    scoped
}

fn invalid_inventory() -> TargetError {
    TargetError::GitFailed("invalid Git name-status inventory".into())
}

fn decode_path(path: &[u8]) -> Result<&str, TargetError> {
    if path.is_empty() {
        return Err(invalid_inventory());
    }
    std::str::from_utf8(path)
        .map_err(|_| TargetError::GitFailed("Git path is not valid UTF-8".into()))
}

fn inventory_plan(output: &[u8], scope: &GitPathScope) -> Result<DiffPlan, TargetError> {
    if output.is_empty() {
        return Ok(DiffPlan::Scoped(Vec::new()));
    }
    let Some(output) = output.strip_suffix(&[0]) else {
        return Err(invalid_inventory());
    };
    let mut fields = output.split(|byte| *byte == 0);
    let mut paths = BTreeSet::new();
    let mut legacy = false;
    while let Some(status) = fields.next() {
        let Some((&kind, similarity)) = status.split_first() else {
            return Err(invalid_inventory());
        };
        if !matches!(
            kind,
            b'A' | b'C' | b'D' | b'M' | b'R' | b'T' | b'U' | b'X' | b'B'
        ) || (!similarity.is_empty()
            && (!matches!(kind, b'R' | b'C' | b'M')
                || !similarity.iter().all(u8::is_ascii_digit)
                || std::str::from_utf8(similarity)
                    .ok()
                    .and_then(|similarity| similarity.parse::<u32>().ok())
                    .is_none_or(|similarity| similarity > 100)))
            || (matches!(kind, b'R' | b'C') && similarity.is_empty())
        {
            return Err(invalid_inventory());
        }
        let old = decode_path(fields.next().ok_or_else(invalid_inventory)?)?;
        let destination = if matches!(kind, b'R' | b'C') {
            decode_path(fields.next().ok_or_else(invalid_inventory)?)?
        } else {
            old
        };
        let Some(path) = scope.resolve(destination)? else {
            continue;
        };
        // Keep exact global rename pairing and unusual/unmerged patch behavior.
        legacy |= matches!(kind, b'R' | b'C' | b'U' | b'X' | b'B');
        legacy |= path.as_str().len() > MAX_PATH_BYTES;
        paths.insert(path.into_string());
    }
    if legacy {
        return Ok(DiffPlan::Legacy);
    }
    Ok(DiffPlan::Scoped(batches(paths)))
}

fn batches(paths: BTreeSet<String>) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut batch = Vec::new();
    let mut bytes = 0;
    for path in paths {
        if batch.len() == MAX_PATHS || bytes + path.len() > MAX_PATH_BYTES {
            result.push(std::mem::take(&mut batch));
            bytes = 0;
        }
        bytes += path.len();
        batch.push(path);
    }
    if !batch.is_empty() {
        result.push(batch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hoimin_core::TargetSlice;

    fn scope(paths: &[&str]) -> GitPathScope {
        GitPathScope::new(Some(
            &paths
                .iter()
                .map(|path| TargetSlice {
                    path: (*path).into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    #[rstest::rstest]
    #[case::unselected_rename(b"M\0selected.py\0R099\0old.py\0other.py\0".as_slice(), &DiffPlan::Scoped(vec![vec!["selected.py".into()]]))]
    #[case::unselected_literal_backslash(b"M\0literal\\bad.py\0".as_slice(), &DiffPlan::Scoped(vec![]))]
    #[case::legacy_0(b"R099\0old.py\0selected.py\0".as_slice(), &DiffPlan::Legacy)]
    #[case::legacy_1(b"C100\0old.py\0selected.py\0", &DiffPlan::Legacy)]
    #[case::legacy_2(b"U\0selected.py\0", &DiffPlan::Legacy)]
    fn inventory_preserves_global_rename_membership(
        #[case] output: &[u8],
        #[case] expected: &DiffPlan,
    ) {
        let selected = scope(&["selected.py"]);

        let actual = inventory_plan(output, &selected).unwrap();

        assert_eq!(&actual, expected);
    }

    #[rstest::rstest]
    #[case::missing_terminator(b"M\0selected.py".as_slice())]
    #[case::incomplete_rename(b"R099\0old.py\0")]
    #[case::oversized_similarity(b"R101\0old.py\0selected.py\0")]
    #[case::missing_similarity(b"R\0old.py\0selected.py\0")]
    #[case::unknown_status(b"Q\0other.py\0")]
    #[case::empty_path(b"M\0\0")]
    #[case::non_utf8_path(b"M\0bad\xff.py\0")]
    fn inventory_rejects_malformed_frames(#[case] output: &[u8]) {
        let selected = scope(&["selected.py"]);

        let result = inventory_plan(output, &selected);

        assert!(result.is_err());
    }

    #[rstest::rstest]
    #[case::count_bound(300, 0)]
    #[case::byte_bound(40, 500)]
    fn pathspec_batches_preserve_original_boundaries(#[case] count: usize, #[case] padding: usize) {
        let names = (0..count)
            .map(|index| format!("{index:03}{}.py", "x".repeat(padding)))
            .collect::<BTreeSet<_>>();

        let grouped = batches(names.clone());

        assert!(grouped.len() > 1);
        assert!(grouped.iter().all(|batch| !batch.is_empty()
            && batch.len() <= 128
            && batch.iter().map(String::len).sum::<usize>() <= 8192));
        assert_eq!(
            grouped.into_iter().flatten().collect::<BTreeSet<_>>(),
            names
        );
    }

    proptest::proptest! {
        #[test]
        fn generated_pathspec_batches_bound_count_and_bytes_without_dropping_paths(count in 0usize..512, padding in 0usize..512) {
            let names = (0..count).map(|index| format!("{index:03}{}.py", "x".repeat(padding))).collect::<BTreeSet<_>>();

            let grouped = batches(names.clone());

            proptest::prop_assert!(grouped.iter().all(|batch| !batch.is_empty() && batch.len() <= 128 && batch.iter().map(String::len).sum::<usize>() <= 8192));
            proptest::prop_assert_eq!(grouped.into_iter().flatten().collect::<BTreeSet<_>>(), names);
        }
    }

    #[test]
    fn oversized_pathspec_uses_legacy_inventory() {
        let long = "x".repeat(MAX_PATH_BYTES + 1);

        let output = format!("M\0{long}\0");
        let selected = scope(&[&long]);

        let actual = inventory_plan(output.as_bytes(), &selected).unwrap();

        assert_eq!(actual, DiffPlan::Legacy);
    }
}

#[cfg(test)]
mod rejection_coverage {
    use super::*;
    use hoimin_core::TargetSlice;

    #[rstest::rstest]
    #[case::empty_status(b"\0".as_slice(), "invalid Git name-status inventory")]
    #[case::missing_old(b"M\0".as_slice(), "invalid Git name-status inventory")]
    #[case::missing_new(b"R100\0old.py\0".as_slice(), "invalid Git name-status inventory")]
    #[case::invalid_old(b"M\0\xff.py\0".as_slice(), "Git path is not valid UTF-8")]
    #[case::invalid_new(b"R100\0old.py\0\xff.py\0".as_slice(), "Git path is not valid UTF-8")]
    fn malformed_inventory_records_preserve_the_typed_diagnostic(
        #[case] input: &[u8],
        #[case] expected: &str,
    ) {
        let error = inventory_plan(input, &GitPathScope::default()).unwrap_err();

        assert!(matches!(error, TargetError::GitFailed(message) if message == expected));
    }

    #[rstest::rstest]
    #[case::outside(b"M\0outside.py\0".as_slice())]
    #[case::separator_alias(b"M\0selected\\app.py\0".as_slice())]
    fn scoped_inventory_omits_records_outside_the_selected_portable_paths(#[case] input: &[u8]) {
        let targets = [TargetSlice {
            path: "selected/app.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let scope = GitPathScope::new(Some(&targets));

        let plan = inventory_plan(input, &scope).unwrap();

        assert_eq!(plan, DiffPlan::Scoped(Vec::new()));
    }
}
