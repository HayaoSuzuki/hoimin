//! Observe actual component iteration on collected entries in test builds.
use std::ops::Deref;
use std::path::{Component, PathBuf};

#[repr(transparent)]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct WorkerNativePath(PathBuf);
impl From<PathBuf> for WorkerNativePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
impl Deref for WorkerNativePath {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}
impl WorkerNativePath {
    pub(super) fn components(&self) -> impl Iterator<Item = Component<'_>> {
        self.0.components().inspect(|_| {
            super::tests::SORT_COMPONENT_VISITS.update(|n| n + 1);
        })
    }
}
#[test]
fn component_observer_records_real_iteration() {
    let path = WorkerNativePath::from(PathBuf::from("a/b/c"));
    super::tests::SORT_COMPONENT_VISITS.set(0);
    assert_eq!(path.components().count(), 3);
    assert_eq!(super::tests::SORT_COMPONENT_VISITS.get(), 3);
}
