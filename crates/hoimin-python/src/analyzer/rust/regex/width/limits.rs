//! Reusable growth limits for several fixed-width assertions.
use std::rc::Rc;

/// A sparse, persistent map from assertion IDs to contributions. Applies
/// repeat multipliers lazily, and shares identical or disjoint subtrees when
/// adding paths. Cached subtree allowances make the tightest limit a scalar
/// lookup after all paths to the same assertion have been combined.
#[derive(Clone, Default)]
pub(super) struct WidthLimits(Option<Scaled>);

#[derive(Debug)]
pub(super) enum Error {
    Cancelled,
    AllocationLimit,
}

#[derive(Clone)]
struct Scaled {
    node: Rc<Node>,
    multiplier: u64,
}

struct Node {
    allowance: u64,
    children: Option<[WidthLimits; 2]>,
}

impl WidthLimits {
    pub(super) fn assertion(
        key: usize,
        bits: u32,
        remaining: u64,
        allocations: &mut usize,
    ) -> Result<Self, Error> {
        if bits == 0 {
            return Self::new(remaining, None, allocations);
        }
        let child = Self::assertion(key, bits - 1, remaining, allocations)?;
        let mut children = [Self::default(), Self::default()];
        children[(key >> (bits - 1)) & 1] = child;
        Self::branch(children, allocations)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub(super) fn allowance(&self) -> u64 {
        self.0
            .as_ref()
            .map_or(u64::MAX, |limit| limit.node.allowance / limit.multiplier)
    }

    pub(super) fn scaled(&self, multiplier: u64) -> Self {
        if multiplier == 0 {
            return Self::default();
        }
        Self(self.0.as_ref().map(|limit| Scaled {
            node: Rc::clone(&limit.node),
            multiplier: limit.multiplier.saturating_mul(multiplier),
        }))
    }

    /// Merge maps covering the same assertion ID range. Positional trie
    /// branches distinguish different assertions even at identical headroom.
    pub(super) fn merge(
        self,
        other: Self,
        allocations: &mut usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        let (a, b) = match (self.0, other.0) {
            (None, subtree) | (subtree, None) => return Ok(Self(subtree)),
            (Some(a), Some(b)) => (a, b),
        };
        if Rc::ptr_eq(&a.node, &b.node) {
            return Ok(Self(Some(a.add_multiplier(b.multiplier))));
        }
        match (&a.node.children, &b.node.children) {
            (Some([a_left, a_right]), Some([b_left, b_right])) => {
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                let left = a_left.scaled(a.multiplier).merge(
                    b_left.scaled(b.multiplier),
                    allocations,
                    cancelled,
                )?;
                let right = a_right.scaled(a.multiplier).merge(
                    b_right.scaled(b.multiplier),
                    allocations,
                    cancelled,
                )?;
                Self::branch([left, right], allocations)
            }
            (None, None) => {
                debug_assert_eq!(a.node.allowance, b.node.allowance);
                Ok(Self(Some(a.add_multiplier(b.multiplier))))
            }
            _ => unreachable!("maps cover the same assertion ID range"),
        }
    }

    fn branch(children: [Self; 2], allocations: &mut usize) -> Result<Self, Error> {
        let allowance = children[0].allowance().min(children[1].allowance());
        Self::new(allowance, Some(children), allocations)
    }

    fn new(
        allowance: u64,
        children: Option<[Self; 2]>,
        allocations: &mut usize,
    ) -> Result<Self, Error> {
        *allocations = allocations.checked_sub(1).ok_or(Error::AllocationLimit)?;
        Ok(Self(Some(Scaled {
            node: Rc::new(Node {
                allowance,
                children,
            }),
            multiplier: 1,
        })))
    }
}

impl Scaled {
    fn add_multiplier(mut self, other: u64) -> Self {
        self.multiplier = self.multiplier.saturating_add(other);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]

        #[test]
        fn merged_scaled_paths_preserve_each_assertions_growth_allowance(
            left in proptest::collection::vec((0..8usize, 1..7u64), 0..20),
            right in proptest::collection::vec((0..8usize, 1..7u64), 0..20),
            scale in 1..5u64,
        ) {
            // Arrange
            let remaining = [91, 37, 59, 19, 87, 62, 98, 73];
            let build = |paths: &[(usize, u64)]| {
                let mut allocations = usize::MAX;
                paths.iter().fold(WidthLimits::default(), |limits, &(key, width)| {
                    let path = WidthLimits::assertion(key, 3, remaining[key], &mut allocations)
                        .unwrap().scaled(width);
                    limits.merge(path, &mut allocations, &|| false).unwrap()
                })
            };
            let mut total = [0; 8];
            for &(key, width) in &left {
                total[key] += width;
            }
            for &(key, width) in &right {
                total[key] += width * scale;
            }
            let expected = remaining.iter().zip(total).filter_map(|(slack, multiplicity)| {
                slack.checked_div(multiplicity)
            }).min().unwrap_or(u64::MAX);

            // Act
            let mut allocations = usize::MAX;
            let actual = build(&left)
                .merge(build(&right).scaled(scale), &mut allocations, &|| false)
                .unwrap();

            // Assert
            proptest::prop_assert_eq!(actual.allowance(), expected);
        }
    }
}
