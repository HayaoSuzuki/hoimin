use camino::Utf8Path;
use hoimin_core::{MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use proptest::prelude::*;
use std::fmt::Write as _;
use std::ops::Range;

fn edits(source: &str, id: &str, cap: usize) -> Vec<(Range<usize>, String)> {
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([id])).unwrap();
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: cap,
    };
    analyze_source_cancellable(&request, source, || false)
        .unwrap()
        .candidates
        .into_iter()
        .map(|candidate| {
            let candidate = candidate.value;
            let start = usize::try_from(candidate.span.start).unwrap();
            let length = usize::try_from(candidate.span.length).unwrap();
            (start..start + length, candidate.replacement)
        })
        .collect()
}

#[rstest::rstest]
#[case::module_protocol_name("context_exit_result_negate", "def __exit__(*exc): return False\n")]
#[case::nested_helper(
    "context_exit_result_negate",
    "class CM:\n def __exit__(self,*exc):\n  def helper(): return False\n  raise ValueError\n"
)]
#[case::wrong_async_protocol(
    "context_exit_result_negate",
    "class CM:\n async def __exit__(self,*exc): return False\n"
)]
#[case::shadowed_type("context_exit_omit", "type = factory\nwith manager: pass\n")]
#[case::shadowed_property("context_exit_omit", "property = factory\nwith manager: pass\n")]
#[case::shadowed_next("context_exit_omit", "next = factory\nwith manager: pass\n")]
#[case::shadowed_type_error("context_exit_omit", "TypeError = factory\nwith manager: pass\n")]
#[case::temporarily_shadowed_helper_with_tracked_operand_root(
    "context_exit_omit",
    "try:\n raise ValueError\nexcept ValueError as property:\n with next(managers): pass\n"
)]
#[case::sync_generator_exit(
    "context_exit_result_negate",
    "class CM:\n def __exit__(self,*exc):\n  if False: yield 1\n  return False\n"
)]
#[case::async_generator_exit(
    "context_exit_result_negate",
    "class CM:\n async def __aexit__(self,*exc):\n  if False: yield 1\n  return\n"
)]
#[case::yield_result_used("yield_statement_remove", "def f():\n result = yield value\n")]
#[case::delegating_yield("yield_statement_remove", "def f():\n yield from values\n")]
fn unsupported_lifecycle_site_has_no_candidates(#[case] id: &str, #[case] source: &str) {
    // Arrange / Act
    let actual = edits(source, id, 100);

    // Assert
    assert!(actual.is_empty(), "{actual:?}");
}

#[rstest::rstest]
#[case::at_limit(193, true)]
#[case::past_limit(194, false)]
fn context_omission_admission_accounts_for_proxy_depth(
    #[case] depth: usize,
    #[case] admitted: bool,
) {
    // Arrange
    let open = "(".repeat(depth);
    let close = ")".repeat(depth);
    let source = format!("with {open}manager{close}: pass\n");

    // Act
    let actual = edits(&source, "context_exit_omit", 100);

    // Assert
    assert_eq!(actual.len(), usize::from(admitted));
    if let [(range, replacement)] = actual.as_slice() {
        let mut mutant = source;
        mutant.replace_range(range.clone(), replacement);
        assert!(ruff_python_parser::parse_module(&mutant).is_ok());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn lifecycle_candidates_keep_the_same_prefix_and_parse(
        yields in 1usize..12,
        cap in 0usize..16,
        value in -1000i32..1000,
    ) {
        // Arrange
        let mut source = String::from("def f():\n");
        for i in 0..yields {
            writeln!(source, "    yield {value} + {i}").unwrap();
        }
        let complete = edits(&source, "yield_statement_remove", 100);

        // Act
        let bounded = edits(&source, "yield_statement_remove", cap);

        // Assert
        prop_assert_eq!(complete.len(), yields);
        prop_assert_eq!(&bounded, &complete[..cap.min(yields)]);
        for (range, replacement) in bounded {
            let mut mutant = source.clone();
            mutant.replace_range(range, &replacement);
            prop_assert!(ruff_python_parser::parse_module(&mutant).is_ok(), "{}", mutant);
        }
    }
}
