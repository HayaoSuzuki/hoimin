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

    assert_eq!(output.candidates.len(), expected_count);
    assert!(!output.truncated);
    assert_eq!(
        stats.used, 0,
        "accounting ends only at terminal plain Discovery handoff"
    );
    eprintln!(
        "PAYLOAD_PROBE {}",
        serde_json::json!({"name":name,"operators":operators.names(),
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
    31
);
normal_probe!(
    proofreadtools_focused,
    PROOFREADTOOLS,
    MutationOperatorSelection::default(),
    Focused,
    31
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
    1309
);
normal_probe!(
    django_focused,
    DJANGO,
    MutationOperatorSelection::default(),
    Focused,
    1289
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
        10_340,
    );
}
