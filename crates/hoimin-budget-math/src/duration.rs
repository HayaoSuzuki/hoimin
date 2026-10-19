use std::time::Duration;

#[cfg(creusot)]
use creusot_std::prelude::{DeepModel, Int, Invariant, View, ensures, logic};

/// A strictly positive duration, checked during construction and deserialization.
/// Timeout ceilings belong to configuration validation, not this invariant.
#[derive(Clone, Copy, Debug, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NonZeroDuration(Duration);

impl PartialEq for NonZeroDuration {
    #[cfg_attr(creusot, ensures(result == (self.deep_model() == other.deep_model())))]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(creusot)]
impl View for NonZeroDuration {
    type ViewTy = Int;

    #[logic(open(self))]
    fn view(self) -> Int {
        pearlite! { self.0@ }
    }
}

#[cfg(creusot)]
impl DeepModel for NonZeroDuration {
    type DeepModelTy = Int;

    #[logic(open)]
    fn deep_model(self) -> Int {
        pearlite! { self@ }
    }
}

#[cfg(creusot)]
impl Invariant for NonZeroDuration {
    #[logic(open)]
    fn invariant(self) -> bool {
        pearlite! { self@ > 0 }
    }
}

impl NonZeroDuration {
    #[must_use]
    #[cfg_attr(creusot, ensures(match result {
        Some(value_out) => value_out.0@ == value@ && value@ > 0,
        None => value@ == 0,
    }))]
    pub fn new(value: Duration) -> Option<Self> {
        if value.is_zero() {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    #[cfg_attr(creusot, ensures(result@ == self@ && result@ > 0))]
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

#[derive(Clone, Copy, Debug, Eq)]
#[cfg_attr(creusot, derive(creusot_std::prelude::DeepModel))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MutantTimeout {
    Auto,
    Fixed(NonZeroDuration),
}

impl PartialEq for MutantTimeout {
    #[cfg_attr(creusot, ensures(result == (self.deep_model() == other.deep_model())))]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Auto, Self::Auto) => true,
            (Self::Fixed(left), Self::Fixed(right)) => left == right,
            _ => false,
        }
    }
}

#[cfg(creusot)]
#[logic(open(crate))]
pub(crate) fn duration_max_nanos() -> Int {
    pearlite! { u64::MAX@ * 1_000_000_000 + 999_999_999 }
}

#[cfg(creusot)]
#[logic(open(crate))]
pub(crate) fn auto_timeout_nanos(baseline: Int) -> Int {
    pearlite! { (2 * baseline + 1_000_000_000).min(duration_max_nanos()).max(5_000_000_000) }
}

#[must_use]
#[cfg_attr(creusot, ensures(result@ == auto_timeout_nanos(baseline@)))]
pub fn auto_mutant_timeout(baseline: Duration) -> Duration {
    baseline
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .unwrap_or(Duration::MAX)
        .max(Duration::from_secs(5))
}
