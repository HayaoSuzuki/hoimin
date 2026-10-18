use std::fmt;

use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer as _};

use super::{PLAN_SCHEMA_VERSION, PlanError, PlanManifest};

/// Route obsolete versions before closed shapes obscure regeneration guidance.
/// Routing never accepts a document: current plans are decoded from the original
/// bytes; unsupported plans check JSON structure and end-of-input before rejection.
pub(super) fn decode_manifest(bytes: &[u8]) -> Result<PlanManifest, PlanError> {
    let invalid = |error: serde_json::Error| PlanError::ManifestInvalid(error.to_string());
    let version = probe_deserializer(&mut serde_json::Deserializer::from_slice(bytes));
    if let Some(version) = version
        && version != u64::from(PLAN_SCHEMA_VERSION)
    {
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        IgnoredAny::deserialize(&mut deserializer).map_err(invalid)?;
        deserializer.end().map_err(invalid)?;
        return Err(PlanError::ManifestInvalid(format!(
            "unsupported schema version {version}; regenerate the plan with this hoimin version"
        )));
    }
    // Retain typed duplicate/unknown-field checks and validation of all trailing bytes.
    serde_json::from_slice(bytes).map_err(invalid)
}

pub(super) fn probe_deserializer<'de, R: serde_json::de::Read<'de>>(
    deserializer: &mut serde_json::Deserializer<R>,
) -> Option<u64> {
    let mut version = None;
    // A successful discriminator deliberately interrupts the map. This is only
    // routing evidence, so incomplete/invalid evidence falls back to typed decoding.
    let _ = deserializer.deserialize_map(SchemaProbe {
        version: &mut version,
    });
    version
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum Field {
    #[serde(rename = "schema_version")]
    SchemaVersion,
    #[serde(other)]
    Other,
}

struct SchemaProbe<'a> {
    version: &'a mut Option<u64>,
}

impl<'de> Visitor<'de> for SchemaProbe<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a plan object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        while let Some(field) = map.next_key::<Field>()? {
            match field {
                Field::SchemaVersion => {
                    *self.version = Some(map.next_value()?);
                    return Err(serde::de::Error::custom("plan schema identified"));
                }
                Field::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}
