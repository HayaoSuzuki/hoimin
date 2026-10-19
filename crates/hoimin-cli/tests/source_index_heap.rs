//! One test owns the allocator windows; discovery reads its real source inside them.
use camino::Utf8PathBuf;
use hoimin_core::{
    CandidateValidationContext, MutationOperatorSelection, MutationProfile, PythonSourceIndex,
    TargetSlice,
};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

#[tokio::test(flavor = "current_thread")]
async fn unicode_context_and_cli_discovery_avoid_overlapping_location_indexes() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let target = TargetSlice {
        path: "source.py".into(),
        lines: vec![],
        symbols: vec![],
    };
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(operator);
        }
    }
    for operator in MutationOperatorSelection::parse_selector("boolean_literal").unwrap() {
        operators.include(operator);
    }
    let mut observations = Vec::new();
    for count in [262_144, 524_288, 1_048_576] {
        let source = format!("#{}\nflag = True\n", "é".repeat(count));
        std::fs::write(root.join("source.py"), &source).unwrap();

        heap_tracking::begin();
        let context = CandidateValidationContext::new(source.as_bytes()).unwrap();
        let context_peak = heap_tracking::finish();
        assert_eq!(
            context.file_hash(),
            blake3::hash(source.as_bytes()).to_hex().as_str()
        );
        drop(context);

        heap_tracking::begin();
        let discovery = hoimin_cli::analyzer::discover_targets(
            &root,
            std::slice::from_ref(&target),
            &operators,
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap();
        let discovery_peak = heap_tracking::finish();
        assert_eq!(discovery.candidates.len(), 1);
        let candidate = &discovery.candidates[0];
        assert_eq!((candidate.line, candidate.column), (2, 7));
        assert_eq!(candidate.original, "True");
        assert_eq!(
            usize::try_from(candidate.span.start).unwrap(),
            2 * count + 9
        );
        let id = candidate.id.clone();
        drop(discovery);

        // Sensitivity control: deliberately retain an additional real Unicode
        // index during the same discovery. This is an eager-retention proxy,
        // while the recorded RED run executes the original eager context itself.
        heap_tracking::begin();
        let retained = PythonSourceIndex::new(&source).unwrap();
        let eager = hoimin_cli::analyzer::discover_targets(
            &root,
            std::slice::from_ref(&target),
            &operators,
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap();
        std::hint::black_box(&retained);
        let eager_peak = heap_tracking::finish();
        assert_eq!(eager.candidates[0].id, id);
        drop((retained, eager));
        eprintln!(
            "unicode-index count={count} context={context_peak} discovery={discovery_peak} eager_control={eager_peak}"
        );
        observations.push((count, context_peak, discovery_peak, eager_peak));
    }
    for (count, context_peak, discovery_peak, eager_peak) in observations {
        // Raw file (2N) plus one 8N Unicode index, allowing parser/file-capacity
        // headroom. A second 8N index exceeds this bound at every fixture size.
        let bound = 12 * count + 262_144;
        assert!(
            eager_peak > bound,
            "insensitive eager control: {eager_peak} <= {bound}"
        );
        assert!(
            discovery_peak <= bound,
            "overlapping CLI indexes: N={count} peak={discovery_peak} bound={bound}"
        );
        assert!(
            context_peak < 1024,
            "unused context built locations: N={count} peak={context_peak}"
        );
    }
}
