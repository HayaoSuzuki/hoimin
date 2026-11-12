//! Explicit release-only measurements using the pinned, read-only audit fixtures.
use super::*;

const INPUT_ROOT: &str = "/tmp/hoimin-design-audit-5d3aee95/analysis-inputs";
const PROOFREADTOOLS: (&str, &str) = (
    "proofreadtools",
    "baaca907fb57acb1aee3a9e32f37958e59daeec7a39cc4a37a07096f06550f75",
);
const DJANGO: (&str, &str) = (
    "django",
    "924cb2668bbe7b7f55bfe204528984bd484398f0e0911d1fbfa9967438f809ea",
);
const GENERATED: (&str, &str) = (
    "generated",
    "6e0ed0bde15d87b67a13fbbe24e81f91c5c0a143abf0a2d567c172f351750495",
);

// The fixtures and original operator results remain fixed; defaults added
// since the original probe commit (5dd992af) account for the larger totals.
const ADDED_DEFAULTS: &[&str] = &[
    "async_iterator_stop_insert",
    "base_order_shuffle",
    "binary_div_floor",
    "bisect_boundary_swap",
    "comparison_not_implemented",
    "continue_remove",
    "dataclass_factory_share",
    "deepcopy_to_copy",
    "deque_popleft_to_pop",
    "encoding_errors_ignore",
    "exception_cause_remove",
    "fstring_conversion_swap",
    "gather_return_exceptions",
    "generator_empty",
    "generator_return_insert",
    "generator_skip_first",
    "iterator_stop_insert",
    "itertools_take_drop_swap",
    "lambda_capture_late",
    "list_comprehension_to_generator",
    "loop_break_insert",
    "mapping_setdefault_to_get",
    "mutable_default_share",
    "none_truthiness",
    "nonlocal_remove",
    "override_method_delete",
    "sort_key_swap",
    "structure_slice_bound_move",
    "structure_slice_bound_omit",
    "super_call_move",
    "yield_from_to_yield",
];

fn check_fixture((name, expected): (&str, &str)) {
    let source = Utf8Path::new(INPUT_ROOT).join(name).join("source.py");
    assert!(source.is_file(), "missing pinned payload fixture: {source}");
    let output = std::process::Command::new("shasum")
        .args(["-a", "256", source.as_str()])
        .output()
        .expect("payload probe requires shasum for pinned SHA-256 verification");
    assert!(
        output.status.success(),
        "fixture SHA-256 failed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .next(),
        Some(expected)
    );
}

#[expect(
    clippy::assertions_on_constants,
    reason = "an explicitly requested debug probe must fail with a useful release-only message"
)]
fn observe(
    name: &str,
    root: &Utf8Path,
    paths: &[&str],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    expected_count: usize,
) {
    assert!(
        !cfg!(debug_assertions),
        "payload probes require cargo test --release (Django has a pre-existing debug-only fact-index assertion)"
    );
    let targets = paths
        .iter()
        .map(|path| TargetSlice {
            path: (*path).into(),
            lines: vec![],
            symbols: vec![],
        })
        .collect::<Vec<_>>();
    let budget = PayloadBudget::default();
    let mut work = discovery_work(
        root,
        &targets,
        operators,
        profile,
        20_000,
        ProcessCancellation::new(),
        None,
    );
    work.payload = budget.clone();
    let started = std::time::Instant::now();

    let output = discover_targets_blocking(work).unwrap();
    let elapsed = started.elapsed();
    let stats = budget.snapshot();

    let mut operator_counts = std::collections::BTreeMap::<&str, usize>::new();
    for candidate in &output.candidates {
        *operator_counts.entry(&candidate.operator).or_default() += 1;
    }
    assert_eq!(
        output.candidates.len(),
        expected_count,
        "operators={operator_counts:?}"
    );
    let original_count = match name {
        "proofreadtools_full" | "proofreadtools_focused" => Some(31),
        "django_full" => Some(1309),
        "django_focused" => Some(1289),
        "normal_three_file_plan" => Some(10_340),
        _ => None,
    };
    if let Some(expected) = original_count {
        let retained = output
            .candidates
            .iter()
            .filter(|candidate| !ADDED_DEFAULTS.contains(&candidate.operator.as_str()))
            .count();
        assert_eq!(
            retained, expected,
            "original catalog results changed: {operator_counts:?}"
        );
    }
    assert!(!output.truncated);
    assert_eq!(
        stats.used, 0,
        "accounting ends only at terminal plain Discovery handoff"
    );
    eprintln!(
        "PAYLOAD_PROBE {}",
        serde_json::json!({"name":name,"operators":operators.names(),"operator_counts":operator_counts,
        "profile":format!("{profile:?}"),"candidates":output.candidates.len(),"charged_peak":stats.peak,
        "terminal_used":stats.used,"reservations":stats.reservations,"releases":stats.releases,
        "admissions":stats.admissions,"generated":stats.generated,"analysis_seconds":elapsed.as_secs_f64(),
        "scope":"whole plan discovery: all producers, conversion scratch, all files; plain terminal handoff"})
    );
}

macro_rules! normal_probe {
    ($name:ident, $fixture:ident, $operators:expr, $profile:ident, $count:expr) => {
        #[test]
        #[ignore = "explicit release probe; requires SHA-256 verified /tmp audit fixtures"]
        fn $name() {
            check_fixture($fixture);
            let root = Utf8Path::new(INPUT_ROOT).join($fixture.0);
            let operators = $operators;

            observe(
                stringify!($name),
                &root,
                &["source.py"],
                &operators,
                MutationProfile::$profile,
                $count,
            );
        }
    };
}
normal_probe!(
    proofreadtools_full,
    PROOFREADTOOLS,
    MutationOperatorSelection::default(),
    Full,
    43
);
normal_probe!(
    proofreadtools_focused,
    PROOFREADTOOLS,
    MutationOperatorSelection::default(),
    Focused,
    43
);
normal_probe!(
    proofreadtools_boolean,
    PROOFREADTOOLS,
    serde_json::from_str("[\"boolean_literal\"]").unwrap(),
    Full,
    0
);
normal_probe!(
    django_full,
    DJANGO,
    MutationOperatorSelection::default(),
    Full,
    1549
);
normal_probe!(
    django_focused,
    DJANGO,
    MutationOperatorSelection::default(),
    Focused,
    1528
);
normal_probe!(
    django_boolean,
    DJANGO,
    serde_json::from_str("[\"boolean_literal\"]").unwrap(),
    Full,
    55
);
normal_probe!(
    generated_full,
    GENERATED,
    MutationOperatorSelection::default(),
    Full,
    9000
);
normal_probe!(
    generated_focused,
    GENERATED,
    MutationOperatorSelection::default(),
    Focused,
    9000
);
normal_probe!(
    generated_boolean,
    GENERATED,
    serde_json::from_str("[\"boolean_literal\"]").unwrap(),
    Full,
    0
);

#[test]
#[ignore = "explicit release probe; requires SHA-256 verified /tmp audit fixtures"]
fn normal_three_file_plan() {
    check_fixture(PROOFREADTOOLS);
    check_fixture(DJANGO);
    check_fixture(GENERATED);

    observe(
        "normal_three_file_plan",
        Utf8Path::new(INPUT_ROOT),
        &[
            "proofreadtools/source.py",
            "django/source.py",
            "generated/source.py",
        ],
        &MutationOperatorSelection::default(),
        MutationProfile::Full,
        10_592,
    );
}
