use std::num::NonZeroUsize;
use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_core::{CommandArg, MutantTimeout, RawRunConfig, RunConfig, project_top_budget};

fn fixed_timeout(value: Duration) -> MutantTimeout {
    let mut raw = RawRunConfig {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    raw.limits.mutant_timeout = Some(value);
    RunConfig::try_from(raw).unwrap().limits.mutant_timeout
}

#[test]
fn serial_auto_timeout_projects_thirty_waves_and_a_shortfall() {
    let projection = project_top_budget(
        30,
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(300),
        Duration::from_secs(17),
        MutantTimeout::Auto,
        Duration::from_secs(281),
    );

    assert_eq!(projection.effective_mutant_timeout, Duration::from_secs(35));
    assert_eq!(projection.waves, 30);
    assert_eq!(projection.projected_capacity, Duration::from_secs(1_050));
    assert!(projection.is_shortfall());
}

#[test]
fn parallel_projection_uses_ceiling_wave_count() {
    let projection = project_top_budget(
        30,
        NonZeroUsize::new(4).unwrap(),
        Duration::from_secs(600),
        Duration::from_secs(17),
        MutantTimeout::Auto,
        Duration::from_secs(565),
    );

    assert_eq!(projection.waves, 8);
    assert_eq!(projection.projected_capacity, Duration::from_secs(280));
    assert!(!projection.is_shortfall());
}

#[test]
fn fixed_timeout_uses_the_exact_configured_duration() {
    let projection = project_top_budget(
        3,
        NonZeroUsize::new(2).unwrap(),
        Duration::from_secs(100),
        Duration::from_secs(1),
        fixed_timeout(Duration::from_millis(1_250)),
        Duration::from_secs(3),
    );

    assert_eq!(
        projection.effective_mutant_timeout,
        Duration::from_millis(1_250)
    );
    assert_eq!(projection.projected_capacity, Duration::from_millis(2_500));
}

#[test]
fn capacity_equal_to_remaining_is_not_a_shortfall() {
    let projection = project_top_budget(
        2,
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        fixed_timeout(Duration::from_secs(3)),
        Duration::from_secs(6),
    );

    assert_eq!(projection.projected_capacity, projection.remaining);
    assert!(!projection.is_shortfall());
}

#[test]
fn zero_selected_produces_zero_waves_and_capacity() {
    let projection = project_top_budget(
        0,
        NonZeroUsize::new(4).unwrap(),
        Duration::from_secs(10),
        Duration::from_secs(2),
        MutantTimeout::Auto,
        Duration::from_secs(10),
    );

    assert_eq!(projection.waves, 0);
    assert_eq!(projection.projected_capacity, Duration::ZERO);
}

#[test]
fn maximum_selected_count_and_duration_saturate_without_panic() {
    let projection = project_top_budget(
        usize::MAX,
        NonZeroUsize::new(1).unwrap(),
        Duration::MAX,
        Duration::MAX,
        MutantTimeout::Auto,
        Duration::MAX,
    );

    assert_eq!(projection.waves, usize::MAX);
    assert_eq!(projection.effective_mutant_timeout, Duration::MAX);
    assert_eq!(projection.projected_capacity, Duration::MAX);
}

#[test]
fn maximum_selected_parallel_projection_preserves_the_ceiling_wave() {
    let projection = project_top_budget(
        usize::MAX,
        NonZeroUsize::new(usize::MAX - 1).unwrap(),
        Duration::from_secs(10),
        Duration::from_secs(1),
        fixed_timeout(Duration::from_secs(2)),
        Duration::from_secs(3),
    );

    assert_eq!(projection.waves, 2);
    assert_eq!(projection.projected_capacity, Duration::from_secs(4));
    assert!(projection.is_shortfall());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn wave_counts_above_u32_max_multiply_exactly_when_duration_fits() {
    let selected = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    let projection = project_top_budget(
        selected,
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(10),
        Duration::from_nanos(1),
        fixed_timeout(Duration::from_nanos(1)),
        Duration::from_nanos(u64::from(u32::MAX)),
    );

    assert_eq!(projection.waves, selected);
    assert_eq!(
        projection.projected_capacity,
        Duration::from_nanos(u64::from(u32::MAX) + 1),
    );
    assert!(projection.is_shortfall());
}
