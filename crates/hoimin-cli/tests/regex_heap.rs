//! Compare real planning allocations on identical inputs and candidate budgets.
#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;
static WINDOW: std::sync::Mutex<()> = std::sync::Mutex::new(());

use camino::Utf8PathBuf;
use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};

#[rstest::rstest]
#[case::typical_regex("typical", "regex")]
#[case::typical_string_literal_empty("typical", "string_literal_empty")]
#[case::typical_integer_literal_neighbor("typical", "integer_literal_neighbor")]
#[case::long_reference_regex("long_reference", "regex")]
#[case::long_reference_string_literal_empty("long_reference", "string_literal_empty")]
#[case::long_reference_integer_literal_neighbor("long_reference", "integer_literal_neighbor")]
#[case::deep_groups_regex("deep_groups", "regex")]
#[case::deep_groups_string_literal_empty("deep_groups", "string_literal_empty")]
#[case::deep_groups_integer_literal_neighbor("deep_groups", "integer_literal_neighbor")]
#[case::without_import_regex("without_import", "regex")]
#[case::without_import_string_literal_empty("without_import", "string_literal_empty")]
#[case::without_import_integer_literal_neighbor("without_import", "integer_literal_neighbor")]
fn measured_discovery(#[case] shape: &str, #[case] selector: &str) {
    let _guard = WINDOW
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let body = match shape {
        "typical" => r"import re; p=r'\d+\s\w+'; result=re.fullmatch(p,'42 x')".to_owned(),
        "long_reference" => format!(
            "import re\np=r'{}'\nresult=re.search(p,'123')",
            r"\d".repeat(2048)
        ),
        "deep_groups" => format!(
            "import re; p=re.compile(r'{}\\d{}')",
            "(?:".repeat(128),
            ")".repeat(128)
        ),
        _ => format!("p=r'{}'\nresult=len(p)+17", r"\d".repeat(2048)),
    };
    let project = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let source = format!("{body}\nvalue=17\n");
    std::fs::write(root.join("subject.py"), &source).unwrap();
    let target = TargetSlice {
        path: "subject.py".into(),
        lines: vec![],
        symbols: vec![],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut ops: MutationOperatorSelection = serde_json::from_value(serde_json::json!([])).unwrap();
    for op in MutationOperatorSelection::parse_selector(selector).unwrap() {
        ops.include(op);
    }
    heap_tracking::begin();
    let at = std::time::Instant::now();
    let result = runtime
        .block_on(hoimin_cli::analyzer::discover_targets(
            &root,
            std::slice::from_ref(&target),
            &ops,
            MutationProfile::Full,
            3,
        ))
        .unwrap();
    let elapsed = at.elapsed();
    let peak = heap_tracking::finish();

    eprintln!(
        "{shape} {selector} candidates={} elapsed_us={} peak_heap_bytes={peak}",
        result.candidates.len(),
        elapsed.as_micros()
    );
    assert!(result.candidates.len() <= 3);
    if shape == "long_reference" && selector == "regex" {
        assert_eq!(result.candidates.len(), 3);
        assert!(peak < 4 * 1024 * 1024);
    }
    if shape == "without_import" && selector == "regex" {
        assert_eq!(result.candidates.len(), 0);
    }
}
