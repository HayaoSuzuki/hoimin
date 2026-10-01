/// Lexical rules for slash-separated, normalized paths beneath a project root.
///
/// These rules do not establish filesystem containment or follow symlinks; those
/// checks belong to the filesystem access layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelativePathPolicy {
    /// Logical paths in mutation candidates and fingerprint records. Also used
    /// for Windows workspaces to reject drive prefixes and alternate streams.
    /// This is not a complete check of Windows filename compatibility.
    Portable,
    /// Unix workspace entries, where a colon is an ordinary filename character.
    UnixWorkspace,
}

impl RelativePathPolicy {
    #[must_use]
    pub fn allows(self, path: &str) -> bool {
        normalized_components(path)
            && match self {
                Self::Portable => !path.contains(':'),
                Self::UnixWorkspace => true,
            }
    }
}

fn normalized_components(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::RelativePathPolicy::{Portable, UnixWorkspace};

    #[test]
    fn both_policies_reject_non_normal_paths_and_nul() {
        for path in [
            "",
            ".",
            "..",
            "../outside",
            "/absolute",
            "a//b",
            "a/./b",
            "a/../b",
            "a/",
            "a\\b",
            "a\0b",
            "//server/share",
            "C:\\file",
        ] {
            for policy in [Portable, UnixWorkspace] {
                assert!(!policy.allows(path), "{policy:?}: {path:?}");
            }
        }
    }

    #[test]
    fn both_policies_accept_normal_relative_paths() {
        for path in [
            "pkg/file.txt",
            ".dockerfiles/appconfig/sample",
            "日本語/設定.txt",
        ] {
            for policy in [Portable, UnixWorkspace] {
                assert!(policy.allows(path), "{policy:?}: {path}");
            }
        }
    }

    #[test]
    fn only_unix_workspaces_accept_colons_in_any_component() {
        for path in [
            ".dockerfiles/appconfig/app:env:conf-sample",
            "fixtures:local/file.txt",
            "file:stream",
            "C:/file.txt",
        ] {
            assert!(!Portable.allows(path), "{path}");
            assert!(UnixWorkspace.allows(path), "{path}");
        }
    }
}
