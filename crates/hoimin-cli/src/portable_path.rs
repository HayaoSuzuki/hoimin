use std::borrow::Cow;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[error("path contains an unsupported literal backslash: {value}")]
pub(crate) struct PortablePathError {
    value: String,
}

impl PortablePathError {
    fn new(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    #[cfg(test)]
    fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn into_value(self) -> String {
        self.value
    }
}

pub(crate) fn from_git(value: &str) -> Result<&str, PortablePathError> {
    if value.contains('\\') {
        Err(PortablePathError::new(value))
    } else {
        Ok(value)
    }
}

#[cfg_attr(
    windows,
    allow(
        clippy::unnecessary_wraps,
        reason = "the shared API rejects backslashes on Unix"
    )
)]
pub(crate) fn from_native(value: &str) -> Result<Cow<'_, str>, PortablePathError> {
    #[cfg(windows)]
    {
        Ok(Cow::Owned(value.replace('\\', "/")))
    }
    #[cfg(not(windows))]
    {
        from_git(value).map(Cow::Borrowed)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{from_git, from_native};

    #[test]
    fn git_paths_are_already_slash_separated() {
        let path = from_git("pkg/file.py").unwrap();

        assert_eq!(path, "pkg/file.py");
    }

    #[test]
    fn git_paths_reject_literal_backslashes() {
        let error = from_git(r"pkg\file.py").unwrap_err();

        assert_eq!(error.value(), r"pkg\file.py");
        assert!(error.to_string().contains(r"pkg\file.py"));
    }

    #[cfg(not(windows))]
    #[test]
    fn native_paths_are_borrowed_without_rewriting_on_non_windows() {
        let path = from_native("pkg/file.py").unwrap();

        assert!(matches!(path, Cow::Borrowed("pkg/file.py")));
    }

    #[cfg(not(windows))]
    #[test]
    fn native_paths_reject_literal_backslashes_on_non_windows() {
        let error = from_native(r"pkg\file.py").unwrap_err();

        assert_eq!(error.value(), r"pkg\file.py");
    }

    #[cfg(windows)]
    #[test]
    fn native_paths_convert_windows_separators() {
        let path = from_native(r"pkg\file.py").unwrap();

        assert!(matches!(path, Cow::Owned(ref value) if value == "pkg/file.py"));
    }
}
