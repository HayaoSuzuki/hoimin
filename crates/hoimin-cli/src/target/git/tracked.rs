use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{LineRange, TargetError};

use super::{GitPathScope, parse_binary_numstat, parse_diff, run_git};

const MAX_PATHS: usize = 128;
const MAX_PATH_BYTES: usize = 8192;

type Changed = BTreeMap<Utf8PathBuf, Vec<LineRange>>;

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
) -> Result<(Changed, BTreeSet<Utf8PathBuf>), TargetError> {
    let plan = if scoped {
        let mut args: Vec<_> = patch_args
            .iter()
            .copied()
            .filter(|arg| !arg.starts_with("--unified="))
            .collect();
        args.splice(1..1, ["--name-status", "-z"]);
        inventory_plan(&run_git(root, &args).await?, scope)?
    } else {
        DiffPlan::Legacy
    };
    let mut changed = BTreeMap::new();
    let mut excluded = BTreeSet::new();
    match plan {
        DiffPlan::Legacy => {
            parse_diff(
                &run_git(root, patch_args).await?,
                &mut changed,
                &mut excluded,
                scope,
            )?;
            parse_binary_numstat(&run_git(root, numstat_args).await?, &mut excluded, scope)?;
        }
        DiffPlan::Scoped(batches) => {
            for paths in batches {
                let patch = scoped_args(patch_args, &paths);
                parse_diff(
                    &run_git(root, &patch).await?,
                    &mut changed,
                    &mut excluded,
                    scope,
                )?;
                let numstat = scoped_args(numstat_args, &paths);
                parse_binary_numstat(&run_git(root, &numstat).await?, &mut excluded, scope)?;
            }
        }
    }
    Ok((changed, excluded))
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

    #[test]
    fn inventory_preserves_global_rename_membership_and_strict_framing() {
        let scope = scope(&["selected.py"]);
        assert_eq!(
            inventory_plan(b"M\0selected.py\0R099\0old.py\0other.py\0", &scope).unwrap(),
            DiffPlan::Scoped(vec![vec!["selected.py".into()]])
        );
        for output in [
            b"R099\0old.py\0selected.py\0".as_slice(),
            b"C100\0old.py\0selected.py\0",
            b"U\0selected.py\0",
        ] {
            assert_eq!(inventory_plan(output, &scope).unwrap(), DiffPlan::Legacy);
        }
        for output in [
            b"M\0selected.py".as_slice(),
            b"R099\0old.py\0",
            b"R101\0old.py\0selected.py\0",
            b"R\0old.py\0selected.py\0",
            b"Q\0other.py\0",
            b"M\0\0",
            b"M\0bad\xff.py\0",
        ] {
            assert!(inventory_plan(output, &scope).is_err(), "{output:?}");
        }
        assert_eq!(
            inventory_plan(b"M\0literal\\bad.py\0", &scope).unwrap(),
            DiffPlan::Scoped(vec![])
        );
    }

    #[test]
    fn pathspec_batches_bound_count_and_total_bytes_without_dropping_paths() {
        for names in [
            (0..300)
                .map(|index| format!("{index:03}.py"))
                .collect::<BTreeSet<_>>(),
            (0..40)
                .map(|index| format!("{index:03}{}.py", "x".repeat(500)))
                .collect(),
        ] {
            let grouped = batches(names.clone());
            assert!(grouped.len() > 1);
            for batch in &grouped {
                assert!(!batch.is_empty());
                assert!(batch.len() <= 128);
                assert!(batch.iter().map(String::len).sum::<usize>() <= 8192);
            }
            assert_eq!(
                grouped.into_iter().flatten().collect::<BTreeSet<_>>(),
                names
            );
        }
        let long = "x".repeat(MAX_PATH_BYTES + 1);
        let output = format!("M\0{long}\0");
        assert_eq!(
            inventory_plan(output.as_bytes(), &scope(&[&long])).unwrap(),
            DiffPlan::Legacy
        );
    }
}
