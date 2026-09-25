use std::ffi::OsString;

use hoimin_core::{ConfigError, normalize_fingerprint_env_names};

pub(crate) fn capture(names: &[String]) -> Result<Option<String>, ConfigError> {
    capture_with(names, |name| std::env::var_os(name))
}

fn capture_with(
    names: &[String],
    mut read: impl FnMut(&str) -> Option<OsString>,
) -> Result<Option<String>, ConfigError> {
    if normalize_fingerprint_env_names(names)? != names {
        return Err(ConfigError::NonNormalizedFingerprintEnv);
    }
    if names.is_empty() {
        return Ok(None);
    }
    let mut hash = blake3::Hasher::new();
    hash.update(b"hoimin.fingerprint.env\0\x01");
    hash.update(&[u8::from(cfg!(windows))]);
    hash.update(&(names.len() as u64).to_le_bytes());
    for name in names {
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        if let Some(value) = read(name) {
            hash.update(&[1]);
            hash_native(&mut hash, &value);
        } else {
            hash.update(&[0]);
        }
    }
    Ok(Some(hash.finalize().to_hex().to_string()))
}

#[cfg(unix)]
fn hash_native(hash: &mut blake3::Hasher, value: &std::ffi::OsStr) {
    use std::os::unix::ffi::OsStrExt;
    hash.update(&(value.as_bytes().len() as u64).to_le_bytes());
    hash.update(value.as_bytes());
}

#[cfg(windows)]
fn hash_native(hash: &mut blake3::Hasher, value: &std::ffi::OsStr) {
    use std::os::windows::ffi::OsStrExt;
    hash.update(&(value.encode_wide().count() as u64).to_le_bytes());
    for unit in value.encode_wide() {
        hash.update(&unit.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(names: &[&str], values: &[Option<&str>]) -> Option<String> {
        let names = names
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        let mut index = 0;
        capture_with(&names, |_| {
            let value = values[index].map(OsString::from);
            index += 1;
            value
        })
        .unwrap()
    }

    #[test]
    fn capture_is_framed_and_distinguishes_absent_empty_and_values() {
        let variants = [None, Some(""), Some("1"), Some("0")];
        for (left_index, left) in variants.iter().enumerate() {
            for (right_index, right) in variants.iter().enumerate() {
                assert_eq!(
                    digest(&["FLAG"], &[*left]) == digest(&["FLAG"], &[*right]),
                    left_index == right_index
                );
            }
        }
        assert_ne!(
            digest(&["A", "B"], &[Some("ab"), Some("c")]),
            digest(&["A", "B"], &[Some("a"), Some("bc")])
        );
        assert_ne!(digest(&["A"], &[Some("BC")]), digest(&["AB"], &[Some("C")]));
        assert_ne!(digest(&["A"], &[None]), digest(&["B"], &[None]));
        assert_eq!(
            capture_with(&[], |_| panic!("unselected variables must not be read")).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_names_are_rejected_before_environment_lookup() {
        for names in [
            vec!["BAD\0NAME".into()],
            vec!["Z".into(), "A".into()],
            vec!["A".into(), "A".into()],
        ] {
            assert!(
                capture_with(&names, |_| panic!("invalid names must not reach OS lookup")).is_err()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_values_do_not_conflate_invalid_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let names = vec!["FLAG".into()];
        let first = capture_with(&names, |_| Some(OsString::from_vec(vec![0x80]))).unwrap();
        let second = capture_with(&names, |_| Some(OsString::from_vec(vec![0x81]))).unwrap();
        assert_ne!(first, second);
    }

    #[cfg(windows)]
    #[test]
    fn windows_values_preserve_unpaired_utf16_units() {
        use std::os::windows::ffi::OsStringExt;
        let names = vec!["FLAG".into()];
        let first = capture_with(&names, |_| Some(OsString::from_wide(&[0xd800]))).unwrap();
        let second = capture_with(&names, |_| Some(OsString::from_wide(&[0xd801]))).unwrap();
        assert_ne!(first, second);
    }
}
