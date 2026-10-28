use super::{analyze, analyze_selected, analyze_selected_with, apply_candidate_and_reparse};

#[test]
fn lambda_capture_late_is_default_for_matching_loop_binding() {
    let source = "def build(values):\n callbacks=[]\n for value in values:\n  callbacks.append(lambda value=value: value)\n return callbacks\n";
    let output = analyze(source);
    let rows: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "lambda_capture_late")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].original, "lambda value=value: value");
    apply_candidate_and_reparse(source, rows[0]);
}

#[test]
fn lambda_capture_preserves_grouping_utf8_comments_and_hygiene() {
    for name in ["value", "_hoimin_get", "_hoimin_missing", "値"] {
        let source = format!(
            "def build(values):\n for {name} in values:\n  callback = (lambda {name}=( # captured\n   {name} # default\n  ): ( # body\n   {name}\n  ))\n"
        );
        let output = analyze_selected(&["lambda_capture_late"], &source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        let changed = apply_candidate_and_reparse(&source, &output.candidates[0]);
        for comment in ["# captured", "# default", "# body"] {
            assert!(changed.contains(comment), "{changed}");
        }
        if name == "_hoimin_get" {
            assert!(changed.contains("lambda _hoimin_get_, _hoimin_missing:"));
        }
        if name == "_hoimin_missing" {
            assert!(changed.contains("lambda _hoimin_get, _hoimin_missing_:"));
        }
    }
}

#[test]
fn lambda_capture_rejects_nonmatching_scopes_and_shapes() {
    for source in [
        "callback=lambda value=value:value\n",
        "for other in values:\n callback=lambda value=value:value\n",
        "for value in values:\n callback=lambda value=other:value\n",
        "for value in values:\n callback=lambda value=value:consume(value)\n",
        "for value in values:\n callback=lambda value=value,other=1:value\n",
        "for value in values:\n callback=lambda value=value,/:value\n",
        "for value in values:\n callback=lambda *,value=value:value\n",
        "for value in values:\n callback=lambda value=value,*args:value\n",
        "for value in values:\n def nested():\n  return lambda value=value:value\n",
        "for value in values:\n callback=lambda:lambda value=value:value\n",
        "for value in values:\n callbacks=[lambda value=value:value for value in values]\n",
        "for value in values:\n pass\nelse:\n callback=lambda value=value:value\n",
        "async def build(values):\n async for value in values:\n  callback=lambda value=value:value\n",
        "for value in values:\n for value,other in pairs:\n  callback=lambda value=value:value\n",
        "for value in values:\n annotation: (lambda value=value:value)\n",
        "for value in values:\n type Alias = lambda value=value:value\n",
    ] {
        let output = analyze_selected(&["lambda_capture_late"], source);
        assert!(
            output.candidates.is_empty(),
            "{source}: {:?}",
            output.candidates
        );
    }
}

#[test]
fn lambda_capture_supports_outer_loop_and_respects_selection_caps_and_cancellation() {
    let source = "def build(values):\n for value in values:\n  first=lambda value=value:value\n  for other in values:\n   second=lambda value=value:value\n";
    let output = analyze_selected(&["lambda_capture_late"], source);
    assert_eq!(output.candidates.len(), 2);
    let limited = analyze_selected_with(
        &["lambda_capture_late"],
        camino::Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(limited.candidates.len(), 1);
    assert!(limited.truncated);
    let selected = analyze_selected_with(
        &["lambda_capture_late"],
        camino::Utf8Path::new("a.py"),
        &[hoimin_core::LineRange { start: 5, end: 5 }],
        &["build".to_owned()],
        10,
        source,
    );
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].line, 5);
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let mut count = 0;
    assert!(
        super::super::lambda_capture::emit(
            parsed.syntax(),
            source,
            &|| true,
            |_| true,
            |_, _| {
                count += 1;
                true
            }
        )
        .is_err()
    );
    assert_eq!(count, 0);
    super::super::lambda_capture::emit(
        parsed.syntax(),
        source,
        &|| false,
        |_| true,
        |_, _| {
            count += 1;
            false
        },
    )
    .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn lambda_capture_excludes_class_namespace_loops_but_allows_method_locals() {
    let source = "class Example:\n callbacks=[]\n for value in [4,9]:\n  callbacks.append(lambda value=value: value)\n def build(self,values):\n  for value in values:\n   callback=lambda value=value: value\n";
    let output = analyze_selected(&["lambda_capture_late"], source);
    assert_eq!(
        output.candidates.len(),
        1,
        "class-local names are not closure cells"
    );
    assert_eq!(output.candidates[0].line, 7);
}
