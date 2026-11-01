use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_budget_math::{
    MutantTimeout, NonZeroDuration, TopBudgetProjection, auto_mutant_timeout, project_top_budget,
};
use proptest::prelude::*;

const DMAX: u128 = u64::MAX as u128 * 1_000_000_000 + 999_999_999;

fn durations() -> impl Strategy<Value = Duration> {
    (any::<u64>(), 0..1_000_000_000u32).prop_map(|(secs, nanos)| Duration::new(secs, nanos))
}

fn project(selected: usize, jobs: usize, duration: Duration) -> TopBudgetProjection {
    project_top_budget(
        selected,
        NonZeroUsize::new(jobs).unwrap(),
        Duration::MAX,
        Duration::ZERO,
        MutantTimeout::Fixed(NonZeroDuration::new(duration).unwrap()),
        Duration::ZERO,
    )
}

#[test]
fn zero_duration_is_rejected() {
    let result = NonZeroDuration::new(Duration::ZERO);

    assert_eq!(result, None);
}

#[rstest::rstest]
#[case::one_nanosecond_round_trips(Duration::from_nanos(1))]
#[case::maximum_duration_round_trips(Duration::MAX)]
fn positive_duration_round_trips(#[case] duration: Duration) {
    let result = NonZeroDuration::new(duration).unwrap().get();

    assert_eq!(result, duration);
}

#[rstest::rstest]
#[case::zero_selected_has_no_waves(0, usize::MAX, 0)]
#[case::maximum_selected_serial(usize::MAX, 1, usize::MAX)]
#[case::maximum_selected_rounds_up(usize::MAX, usize::MAX - 1, 2)]
#[case::exact_division_has_no_extra_wave(30, 5, 6)]
fn projection_wave_boundaries(
    #[case] selected: usize,
    #[case] jobs: usize,
    #[case] expected: usize,
) {
    let projection = project(selected, jobs, Duration::from_nanos(1));

    assert_eq!(projection.waves, expected);
}

#[rstest::rstest]
#[case::zero_baseline_uses_floor(Duration::ZERO, Duration::from_secs(5))]
#[case::auto_floor_equality(Duration::from_secs(2), Duration::from_secs(5))]
#[case::auto_preserves_nanoseconds(Duration::new(2, 1), Duration::new(5, 2))]
#[case::auto_multiplication_saturates(Duration::MAX, Duration::MAX)]
#[case::auto_addition_saturates(Duration::new(u64::MAX / 2, 500_000_000), Duration::MAX)]
fn automatic_timeout_boundaries(#[case] baseline: Duration, #[case] expected: Duration) {
    let result = auto_mutant_timeout(baseline);

    assert_eq!(result, expected);
}

#[rstest::rstest]
#[case::zero_waves_times_maximum_is_zero(0, Duration::MAX, Duration::ZERO)]
#[case::maximum_duration_times_one_is_exact(1, Duration::MAX, Duration::MAX)]
#[case::duration_product_saturates(2, Duration::MAX, Duration::MAX)]
#[case::intermediate_product_saturates(usize::MAX, Duration::MAX, Duration::MAX)]
#[case::nanoseconds_carry_into_seconds(
    3,
    Duration::new(0, 500_000_001),
    Duration::new(1, 500_000_003)
)]
fn projection_capacity_boundaries(
    #[case] selected: usize,
    #[case] duration: Duration,
    #[case] expected: Duration,
) {
    let projection = project(selected, 1, duration);

    assert_eq!(projection.projected_capacity, expected);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn waves_above_u32_are_not_truncated() {
    let projection = project(4_294_967_296, 1, Duration::from_nanos(1));

    assert_eq!(projection.projected_capacity, Duration::new(4, 294_967_296));
}

#[rstest::rstest]
#[case::manual_equal_is_not_shortfall(Duration::MAX, Duration::MAX, false)]
#[case::manual_greater_is_shortfall(Duration::from_nanos(1), Duration::ZERO, true)]
#[case::manual_less_is_not_shortfall(Duration::ZERO, Duration::from_nanos(1), false)]
fn manual_projection_shortfall_boundaries(
    #[case] capacity: Duration,
    #[case] remaining: Duration,
    #[case] expected: bool,
) {
    // Deliberately inconsistent fields: public structs are valid input.
    let projection = TopBudgetProjection {
        selected: usize::MAX,
        jobs: 0,
        planned_total_timeout: Duration::ZERO,
        baseline: Duration::MAX,
        effective_mutant_timeout: Duration::MAX,
        remaining,
        waves: 0,
        projected_capacity: capacity,
    };

    let result = projection.is_shortfall();

    assert_eq!(result, expected);
}

proptest! {
    #[test]
    fn positive_durations_round_trip(duration in durations().prop_filter("positive", |d| !d.is_zero())) {
        let result = NonZeroDuration::new(duration).unwrap().get();

        prop_assert_eq!(result, duration);
    }

    #[test]
    fn waves_are_the_smallest_covering_count(selected in any::<usize>(), jobs in 1..=usize::MAX) {
        let selected_natural = selected as u128;
        let jobs_natural = jobs as u128;

        let projection = project(selected, jobs, Duration::from_nanos(1));
        let waves = projection.waves as u128;

        // These products fit u128 for every 32-bit or 64-bit usize input.
        prop_assert!(selected_natural <= waves * jobs_natural
            && (waves == 0 || (waves - 1) * jobs_natural < selected_natural));
    }

    #[test]
    fn auto_matches_bounded_natural_arithmetic(baseline in durations()) {
        // 2 * DMAX + 1 second fits u128; no saturating Duration operations in this oracle.
        let expected = (2 * baseline.as_nanos() + 1_000_000_000).clamp(5_000_000_000, DMAX);

        let result = auto_mutant_timeout(baseline);

        prop_assert_eq!(result.as_nanos(), expected);
    }

    #[test]
    fn capacity_matches_division_guarded_product(
        selected in any::<usize>(), jobs in 1..=usize::MAX,
        duration in durations().prop_filter("positive", |d| !d.is_zero()),
    ) {
        // Keep this widened additive oracle independent of production arithmetic.
        #[allow(clippy::manual_div_ceil)]
        let waves = (selected as u128 + jobs as u128 - 1) / jobs as u128;
        let nanos = duration.as_nanos();
        // Guard before multiplying: even a full-width usize product cannot overflow the oracle.
        let expected = if waves > DMAX / nanos { DMAX } else { nanos * waves };

        let projection = project(selected, jobs, duration);

        prop_assert_eq!(projection.projected_capacity.as_nanos(), expected);
    }

    #[test]
    fn fixed_timeout_is_preserved(duration in durations().prop_filter("positive", |d| !d.is_zero())) {
        let projection = project(1, 1, duration);

        prop_assert_eq!(projection.effective_mutant_timeout, duration);
    }

    #[test]
    fn auto_projection_uses_auto_arithmetic(baseline in durations()) {
        let expected = (2 * baseline.as_nanos() + 1_000_000_000).clamp(5_000_000_000, DMAX);

        let projection = project_top_budget(1, NonZeroUsize::new(1).unwrap(), Duration::ZERO, baseline, MutantTimeout::Auto, Duration::ZERO);

        prop_assert_eq!(projection.effective_mutant_timeout.as_nanos(), expected);
    }

    #[test]
    fn input_fields_are_preserved(selected in any::<usize>(), jobs in 1..=usize::MAX, planned in durations(), baseline in durations(), remaining in durations()) {
        let projection = project_top_budget(selected, NonZeroUsize::new(jobs).unwrap(), planned, baseline, MutantTimeout::Auto, remaining);

        prop_assert_eq!((projection.selected, projection.jobs, projection.planned_total_timeout, projection.baseline, projection.remaining), (selected, jobs, planned, baseline, remaining));
    }

    #[test]
    fn shortfall_matches_strict_comparison(capacity in durations(), remaining in durations()) {
        let projection = TopBudgetProjection { selected: 0, jobs: 0, planned_total_timeout: Duration::MAX, baseline: Duration::MAX, effective_mutant_timeout: Duration::ZERO, remaining, waves: usize::MAX, projected_capacity: capacity };

        let result = projection.is_shortfall();

        prop_assert_eq!(result, capacity.as_nanos() > remaining.as_nanos());
    }
}
