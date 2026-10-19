use std::time::Duration;

/// A strictly positive duration, checked during construction and deserialization.
/// Timeout ceilings belong to configuration validation, not this invariant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NonZeroDuration(Duration);

impl NonZeroDuration {
    #[must_use]
    pub fn new(value: Duration) -> Option<Self> {
        if value.is_zero() {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NonZeroDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Duration::deserialize(deserializer)?;
        Self::new(value)
            .ok_or_else(|| serde::de::Error::custom("invalid zero or overflowing limit: duration"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MutantTimeout {
    Auto,
    Fixed(NonZeroDuration),
}

#[must_use]
pub fn auto_mutant_timeout(baseline: Duration) -> Duration {
    baseline
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .unwrap_or(Duration::MAX)
        .max(Duration::from_secs(5))
}
