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

    #[rstest::rstest]
    #[case::value("")]
    #[case::value_2(".")]
    #[case::value_3("..")]
    #[case::outside("../outside")]
    #[case::absolute("/absolute")]
    #[case::a_b("a//b")]
    #[case::a_b_7("a/./b")]
    #[case::a_b_8("a/../b")]
    #[case::a("a/")]
    #[case::a_b_10("a\\b")]
    #[case::a_b_11("a\0b")]
    #[case::server_share("//server/share")]
    #[case::c_file("C:\\file")]
    fn both_policies_reject_non_normal_paths_and_nul(
        #[case] path: &str,
        #[values(0, 1)] scenario_index: usize,
    ) {
        let policy = [Portable, UnixWorkspace]
            .into_iter()
            .nth(scenario_index)
            .unwrap();

        assert!(!policy.allows(path), "{policy:?}: {path:?}");
    }

    #[rstest::rstest]
    #[case::pkg_file_txt("pkg/file.txt")]
    #[case::dockerfiles_appconfig_sample(".dockerfiles/appconfig/sample")]
    #[case::txt("日本語/設定.txt")]
    fn both_policies_accept_normal_relative_paths(
        #[case] path: &str,
        #[values(0, 1)] scenario_index: usize,
    ) {
        let policy = [Portable, UnixWorkspace]
            .into_iter()
            .nth(scenario_index)
            .unwrap();

        assert!(policy.allows(path), "{policy:?}: {path}");
    }

    #[rstest::rstest]
    #[case::dockerfiles_appconfig_app_env_conf_sample(".dockerfiles/appconfig/app:env:conf-sample")]
    #[case::fixtures_local_file_txt("fixtures:local/file.txt")]
    #[case::file_stream("file:stream")]
    #[case::c_file_txt("C:/file.txt")]
    fn only_unix_workspaces_accept_colons_in_any_component(#[case] path: &str) {
        assert!(!Portable.allows(path), "{path}");
        assert!(UnixWorkspace.allows(path), "{path}");
    }
}
