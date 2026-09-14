//! Python source decoding with offsets and writes tied to original bytes.
use std::borrow::Cow;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythonSourceEncoding {
    Utf8,
    Ascii,
    Latin1,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("source encoding '{encoding}': {reason}")]
pub struct SourceEncodingError {
    pub encoding: String,
    pub reason: String,
}

impl PythonSourceEncoding {
    /// Encodes mutation text without adding a BOM or changing line endings.
    ///
    /// # Errors
    /// Returns an error when a character cannot be represented by this codec.
    pub fn encode(self, text: &str) -> Result<Cow<'_, [u8]>, SourceEncodingError> {
        match self {
            Self::Utf8 => Ok(Cow::Borrowed(text.as_bytes())),
            Self::Ascii if text.is_ascii() => Ok(Cow::Borrowed(text.as_bytes())),
            Self::Ascii => Err(SourceEncodingError {
                encoding: "ascii".into(),
                reason: "mutation text contains a non-ASCII character".into(),
            }),
            Self::Latin1 => text
                .chars()
                .map(|character| u8::try_from(u32::from(character)))
                .collect::<Result<Vec<_>, _>>()
                .map(Cow::Owned)
                .map_err(|_| SourceEncodingError {
                    encoding: "latin-1".into(),
                    reason: "mutation text contains a character outside U+0000..U+00FF".into(),
                }),
        }
    }
}

#[derive(Debug)]
pub struct DecodedPythonSource<'source> {
    text: Cow<'source, str>,
    encoding: PythonSourceEncoding,
    raw_len: usize,
    // Latin-1 non-ASCII byte positions, in raw and decoded coordinates.
    expanded: Vec<(usize, usize)>,
}

impl DecodedPythonSource<'_> {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn encoding(&self) -> PythonSourceEncoding {
        self.encoding
    }

    /// Maps a decoded UTF-8 character boundary to an original byte boundary.
    #[must_use]
    pub fn utf8_to_raw(&self, offset: usize) -> Option<usize> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        let extra = self.expanded.partition_point(|(_, end)| *end <= offset);
        offset.checked_sub(extra)
    }

    /// Maps an original byte boundary to decoded UTF-8, rejecting split scalars.
    #[must_use]
    pub fn raw_to_utf8(&self, offset: usize) -> Option<usize> {
        if offset > self.raw_len {
            return None;
        }
        let extra = self.expanded.partition_point(|(start, _)| *start < offset);
        let decoded = offset.checked_add(extra)?;
        self.text.is_char_boundary(decoded).then_some(decoded)
    }
}

/// Decodes Python source without changing physical newlines or a leading UTF-8 BOM.
///
/// # Errors
/// Returns the declared encoding and a reason for unknown/unsupported codecs,
/// BOM conflicts, and bytes that do not match UTF-8 or ASCII.
pub fn decode_python_source(source: &[u8]) -> Result<DecodedPythonSource<'_>, SourceEncodingError> {
    let bom = source.starts_with(b"\xef\xbb\xbf");
    let cookie_bytes = if bom { &source[3..] } else { source };
    let declared = encoding_cookie(cookie_bytes);
    let name = declared.unwrap_or("utf-8 (default)");
    let normalized = declared
        .unwrap_or("utf-8")
        .to_ascii_lowercase()
        .replace('_', "-");
    let tokenizer_utf8 = normalized == "utf-8" || normalized.starts_with("utf-8-");
    let failure = |reason: String| SourceEncodingError {
        encoding: name.into(),
        reason,
    };
    if bom && !tokenizer_utf8 {
        return Err(failure("declaration conflicts with UTF-8 BOM".into()));
    }
    let encoding = if tokenizer_utf8 || normalized == "utf8" {
        PythonSourceEncoding::Utf8
    } else if matches!(normalized.as_str(), "ascii" | "us-ascii" | "646") {
        PythonSourceEncoding::Ascii
    } else if matches!(
        normalized.as_str(),
        "latin1"
            | "latin-1"
            | "iso-8859-1"
            | "iso-latin-1"
            | "iso8859-1"
            | "l1"
            | "latin"
            | "cp819"
            | "ibm819"
    ) || ["latin-1-", "iso-8859-1-", "iso-latin-1-"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        PythonSourceEncoding::Latin1
    } else {
        return Err(failure(
            "unsupported or unknown codec; supported codecs are UTF-8, ASCII and Latin-1".into(),
        ));
    };
    let mut expanded = Vec::new();
    let text = match encoding {
        PythonSourceEncoding::Utf8 => Cow::Borrowed(
            std::str::from_utf8(source)
                .map_err(|error| failure(format!("bytes are not valid UTF-8: {error}")))?,
        ),
        PythonSourceEncoding::Ascii => {
            if let Some(offset) = source.iter().position(|byte| !byte.is_ascii()) {
                return Err(failure(format!(
                    "non-ASCII byte at original offset {offset}"
                )));
            }
            Cow::Borrowed(std::str::from_utf8(source).map_err(|error| failure(error.to_string()))?)
        }
        PythonSourceEncoding::Latin1 => {
            let mut decoded = String::with_capacity(source.len());
            for (offset, byte) in source.iter().copied().enumerate() {
                decoded.push(char::from(byte));
                if !byte.is_ascii() {
                    expanded.push((offset, decoded.len()));
                }
            }
            Cow::Owned(decoded)
        }
    };
    Ok(DecodedPythonSource {
        text,
        encoding,
        raw_len: source.len(),
        expanded,
    })
}

fn encoding_cookie(mut source: &[u8]) -> Option<&str> {
    for _ in 0..2 {
        let end = source
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .unwrap_or(source.len());
        let line = &source[..end];
        let first = line
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\t' | 0x0c));
        if let Some(first) = first {
            if line[first] != b'#' {
                return None;
            }
            for offset in first..line.len().saturating_sub(6) {
                if &line[offset..offset + 6] != b"coding"
                    || !matches!(line[offset + 6], b':' | b'=')
                {
                    continue;
                }
                let rest = &line[offset + 7..];
                let start = rest
                    .iter()
                    .position(|byte| !matches!(byte, b' ' | b'\t'))
                    .unwrap_or(rest.len());
                let length = rest[start..]
                    .iter()
                    .take_while(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                    .count();
                if length != 0 {
                    return std::str::from_utf8(&rest[start..start + length]).ok();
                }
            }
        }
        let mut next = end;
        if source.get(next) == Some(&b'\r') {
            next += 1;
        }
        if source.get(next) == Some(&b'\n') {
            next += 1;
        }
        source = &source[next..];
    }
    None
}
