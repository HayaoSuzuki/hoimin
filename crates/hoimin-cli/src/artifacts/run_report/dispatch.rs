use std::fmt;

use serde::Deserializer as _;
use serde::de::{IgnoredAny, MapAccess, Visitor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Format {
    Document,
    Jsonl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Header {
    pub(super) format: Format,
    pub(super) schema_version: u32,
}

pub(super) fn probe(bytes: &[u8]) -> Option<Header> {
    probe_deserializer(&mut serde_json::Deserializer::from_slice(bytes))
}

fn probe_deserializer<'de, R: serde_json::de::Read<'de>>(
    deserializer: &mut serde_json::Deserializer<R>,
) -> Option<Header> {
    let mut header = None;
    // The visitor deliberately stops before the rest of the map. This is only
    // routing evidence: the selected full parser must still validate all bytes.
    let _ = deserializer.deserialize_map(HeaderProbe {
        header: &mut header,
    });
    header
}

struct HeaderProbe<'a> {
    header: &'a mut Option<Header>,
}

impl<'de> Visitor<'de> for HeaderProbe<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a report or event object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let mut schema_version = None;
        let mut format = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "schema_version" => schema_version = Some(map.next_value()?),
                "kind" => {
                    let _: String = map.next_value()?;
                    format = Some(Format::Jsonl);
                }
                "run" | "baseline" | "mutants" | "summary" => {
                    format.get_or_insert(Format::Document);
                    if schema_version.is_none() {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            if let (Some(schema_version), Some(format)) = (schema_version, format) {
                *self.header = Some(Header {
                    format,
                    schema_version,
                });
                return Err(serde::de::Error::custom("progress dispatch identified"));
            }
        }
        *self.header = schema_version.map(|schema_version| Header {
            format: format.unwrap_or(Format::Document),
            schema_version,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};

    struct Counted<'a> {
        input: &'a [u8],
        read: usize,
    }
    impl Read for Counted<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = self.input.read(output)?;
            self.read += count;
            Ok(count)
        }
    }

    #[test]
    fn compact_dispatch_does_not_scan_candidate_bodies() {
        for bytes in [32, 256 * 1024] {
            let input = format!(
                "{{\"schema_version\":3,\"mutants\":[\"{}\"],\"run\":null}}",
                "x".repeat(bytes)
            );
            let mut reader = Counted {
                input: input.as_bytes(),
                read: 0,
            };
            let header =
                probe_deserializer(&mut serde_json::Deserializer::from_reader(&mut reader));
            assert_eq!(
                header,
                Some(Header {
                    format: Format::Document,
                    schema_version: 3
                })
            );
            eprintln!("dispatch body_bytes={bytes} consumed_bytes={}", reader.read);
            assert!(
                reader.read < 128,
                "dispatch scanned {} bytes for a {bytes}-byte body",
                reader.read
            );
        }
    }

    #[test]
    fn dispatch_handles_reordered_nested_and_escaped_keys() {
        for (bytes, expected) in [
            (
                r#"{"kind":"run_started","schema_version":3,"run_id":"x"}"#,
                Format::Jsonl,
            ),
            (
                r#"{"run_id":"x","schema_version":3,"kind":"run_started"}"#,
                Format::Jsonl,
            ),
            (
                r#"{"mutants":[{"kind":"nested","schema_version":2}],"run":{},"schema_version":3}"#,
                Format::Document,
            ),
            (r#"{"sch\u0065ma_version":3,"\u0072un": "#, Format::Document),
        ] {
            assert_eq!(
                probe(bytes.as_bytes()),
                Some(Header {
                    format: expected,
                    schema_version: 3
                }),
                "{bytes}"
            );
        }
        assert_eq!(
            probe(br#"{"schema_version":2,"run":{}}"#)
                .unwrap()
                .schema_version,
            2
        );
    }

    #[test]
    fn incomplete_or_invalid_discriminators_have_no_dispatch_evidence() {
        for bytes in [
            b"[]".as_slice(),
            b"{",
            br#"{"schema_version":"3","run":{}}"#,
            br#"{"other":{"schema_version":3,"kind":"nested"}}"#,
        ] {
            assert_eq!(probe(bytes), None);
        }
    }
}
