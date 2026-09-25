use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{TargetError, TargetSlice, logical_path_equality_key};

use crate::portable_path;

/// Membership is checked against already discovered, portable target paths.
#[derive(Default)]
pub(super) struct GitPathScope {
    keys: Option<BTreeSet<String>>,
}

impl GitPathScope {
    pub(super) fn new(targets: Option<&[TargetSlice]>) -> Self {
        Self {
            keys: targets.map(|targets| {
                targets
                    .iter()
                    .map(|target| logical_path_equality_key(&target.path).into_owned())
                    .collect()
            }),
        }
    }

    pub(super) fn resolve(&self, value: &str) -> Result<Option<Utf8PathBuf>, TargetError> {
        if let Some(keys) = &self.keys {
            // Git always emits slash separators. Do not let Windows comparison
            // normalize a literal backslash into an unrelated eligible path.
            if value.contains('\\')
                || !keys.contains(logical_path_equality_key(Utf8Path::new(value)).as_ref())
            {
                return Ok(None);
            }
        }
        let path = portable_path::from_git(value)
            .map_err(|error| TargetError::GitFailed(error.to_string()))?;
        Ok(Some(Utf8PathBuf::from(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::GitPathScope;
    use hoimin_core::TargetSlice;

    #[test]
    fn absent_scope_stays_strict_while_empty_scope_admits_no_path() {
        assert!(GitPathScope::default().resolve(r"bad\name.py").is_err());
        for name in ["good.py", r"bad\name.py", "data.txt"] {
            assert_eq!(GitPathScope::new(Some(&[])).resolve(name).unwrap(), None);
        }
    }

    #[test]
    fn scoped_membership_preserves_platform_case_without_separator_aliases() {
        let targets = [TargetSlice {
            path: "selected/app.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let scope = GitPathScope::new(Some(&targets));
        assert_eq!(
            scope.resolve("selected/app.py").unwrap(),
            Some(targets[0].path.clone())
        );
        assert_eq!(scope.resolve(r"selected\app.py").unwrap(), None);
        assert_eq!(scope.resolve("other/app.py").unwrap(), None);
        assert_eq!(
            scope.resolve("SELECTED/APP.PY").unwrap().is_some(),
            cfg!(windows)
        );
    }
}
