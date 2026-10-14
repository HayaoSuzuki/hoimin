use std::ffi::OsString;

use hoimin_core::{ConfigError, EnvironmentRequest, EnvironmentSnapshot};

pub(crate) fn capture(request: &EnvironmentRequest) -> Result<EnvironmentSnapshot, ConfigError> {
    capture_with(request, |name| std::env::var_os(name))
}

fn capture_with(
    request: &EnvironmentRequest,
    mut read: impl FnMut(&str) -> Option<OsString>,
) -> Result<EnvironmentSnapshot, ConfigError> {
    let names = request.names();
    if names.is_empty() {
        return Ok(EnvironmentSnapshot::Untracked);
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
    EnvironmentSnapshot::captured(names.to_vec(), hash.finalize().to_hex().to_string())
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
        let request = EnvironmentRequest::new(&names).unwrap();
        capture_with(&request, |_| {
            let value = values[index].map(OsString::from);
            index += 1;
            value
        })
        .unwrap()
        .digest()
        .map(str::to_owned)
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
            capture_with(&EnvironmentRequest::new(&[]).unwrap(), |_| panic!(
                "unselected variables must not be read"
            ))
            .unwrap(),
            EnvironmentSnapshot::Untracked
        );
    }

    #[test]
    fn requests_normalize_names_before_environment_lookup() {
        let request = EnvironmentRequest::new(&["Z".into(), "A".into(), "A".into()]).unwrap();
        let mut seen = Vec::new();
        let snapshot = capture_with(&request, |name| {
            seen.push(name.to_owned());
            None
        })
        .unwrap();
        assert_eq!(seen, ["A", "Z"]);
        assert_eq!(snapshot.names(), seen);
        assert!(EnvironmentRequest::new(&["BAD\0NAME".into()]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unix_values_do_not_conflate_invalid_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let names = EnvironmentRequest::new(&["FLAG".into()]).unwrap();
        let first = capture_with(&names, |_| Some(OsString::from_vec(vec![0x80]))).unwrap();
        let second = capture_with(&names, |_| Some(OsString::from_vec(vec![0x81]))).unwrap();
        assert_ne!(first, second);
    }

    #[cfg(windows)]
    #[test]
    fn windows_values_preserve_unpaired_utf16_units() {
        use std::os::windows::ffi::OsStringExt;
        let names = EnvironmentRequest::new(&["FLAG".into()]).unwrap();
        let first = capture_with(&names, |_| Some(OsString::from_wide(&[0xd800]))).unwrap();
        let second = capture_with(&names, |_| Some(OsString::from_wide(&[0xd801]))).unwrap();
        assert_ne!(first, second);
    }
}
