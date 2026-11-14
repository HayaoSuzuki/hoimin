use super::{analyze, apply_candidate_and_reparse};

#[test]
fn dataclass_factory_share_is_default_for_plain_dataclass() {
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n    values: list = field(default_factory=list)\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "dataclass_factory_share")
        .collect();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].original, "list");
    assert_eq!(candidates[0].replacement, "lambda shared=[]: shared");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[rstest::rstest]
#[case::from_dataclasses_import_dataclass_field(
    "from dataclasses import dataclass, field",
    "dataclass",
    "field"
)]
#[case::from_dataclasses_import_dataclass_as_dc_field_as_item(
    "from dataclasses import dataclass as dc, field as item",
    "dc(frozen=True)",
    "item"
)]
#[case::import_dataclasses_as_dc(
    "import dataclasses as dc",
    "dc.dataclass(slots=True, kw_only=True)",
    "dc.field"
)]
fn dataclass_factory_share_preserves_factory_options_aliases_and_grouping(
    #[case] imports: &str,
    #[case] decorator: &str,
    #[case] field: &str,
    #[values(0, 1, 2)] scenario_index: usize,
    #[values(0, 1, 2, 3)] nested_scenario_index: usize,
) {
    let (factory, replacement) = [
        ("list", "lambda shared=[]: shared"),
        ("dict", "lambda shared={}: shared"),
        ("set", "lambda shared={*()}: shared"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!(
        "{imports}\n@{decorator}\nclass Example:\n    結果: object = {field}(default_factory=( # factory\n        {factory} # preserve\n    ), repr=False, compare=False)\n"
    );

    let output = analyze(&source);
    let rows: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "dataclass_factory_share")
        .collect();

    assert_eq!(rows.len(), 1, "{source}");
    assert_eq!(rows[0].original, factory);
    assert_eq!(rows[0].replacement, replacement);
    let changed = apply_candidate_and_reparse(&source, rows[0]);

    let preserved = [
        "# factory",
        "# preserve",
        "repr=False, compare=False",
        "結果: object",
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    assert!(changed.contains(preserved), "{changed}");
}

#[rstest::rstest]
#[case::class_example_values_list_field_default_factory_list(
    "class Example:\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_options_class_example_values_list_field_default_factory(
    "@dataclass(**options)\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_de(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n def method(self):\n  return self.values\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_va(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n values: dict = field(default_factory=dict)\n"
)]
#[case::other_dataclass_class_example_values_list_field_default_factory_l(
    "@other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_base_values_list_field_default_factory_li(
    "@dataclass\nclass Example(Base):\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_metaclass_meta_values_list_field_default(
    "@dataclass\nclass Example(metaclass=Meta):\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_t_values_list_field_default_factory_list(
    "@dataclass\nclass Example[T]:\n values: list = field(default_factory=list)\n"
)]
#[case::def_outer_dataclass_class_example_values_list_field_default_facto(
    "def outer():\n @dataclass\n class Example:\n  values: list = field(default_factory=list)\n"
)]
#[case::if_flag_dataclass_class_example_values_list_field_default_factory(
    "if flag:\n @dataclass\n class Example:\n  values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_values_field_default_factory_list(
    "@dataclass\nclass Example:\n values = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_custom(
    "@dataclass\nclass Example:\n values: list = field(default_factory=custom)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_lambda(
    "@dataclass\nclass Example:\n values: list = field(default_factory=lambda: [])\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list())\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_op(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list, **options)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_16(
    "@dataclass\nclass Example:\n values: list = field([], default_factory=list)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_fi(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n field: object = other\n"
)]
#[case::dataclass_class_example_list_object_custom_values_list_field_defa(
    "@dataclass\nclass Example:\n list: object = custom\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_values_list_field_default_factory_list_li(
    "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n list: object = custom\n"
)]
#[case::dataclass_class_example_locals_field_other_values_list_field_defa(
    "@dataclass\nclass Example:\n locals()['field'] = other\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_class_example_annotations_object_mapping_values_list_fi(
    "@dataclass\nclass Example:\n __annotations__: object = mapping\n values: list = field(default_factory=list)\n"
)]
#[case::dataclass_other_dataclass_class_example_values_list_field_default(
    "dataclass = other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::field_other_dataclass_class_example_values_list_field_default_fac(
    "field = other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::list_custom_dataclass_class_example_values_list_field_default_fac(
    "list = custom\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::from_elsewhere_import_dataclass_class_example_values_list_field_d(
    "from elsewhere import *\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
#[case::exec_code_dataclass_class_example_values_list_field_default_facto(
    "exec(code)\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n"
)]
fn dataclass_factory_share_rejects_uncertain_class_or_factory_identity_scenario_1(
    #[case] body: &str,
) {
    let prefix = "from dataclasses import dataclass, field\n";

    let source = format!("{prefix}{body}");

    let output = analyze(&source);

    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "dataclass_factory_share"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::import_dataclasses_as_dc_dc_field_other_dc_dataclass_class_a_x_li(
    "import dataclasses as dc\ndc.field = other\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n"
)]
#[case::import_dataclasses_as_dc_expose_dc_dc_dataclass_class_a_x_list_dc(
    "import dataclasses as dc\nexpose(dc)\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n"
)]
#[case::import_dataclasses_as_dc_dc_dataclass_class_a_x_list_dc_field_def(
    "import dataclasses as dc\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n dc: object = other\n"
)]
#[case::from_elsewhere_import_dataclass_field_dataclass_class_a_x_list_fi(
    "from elsewhere import dataclass, field\n@dataclass\nclass A:\n x: list = field(default_factory=list)\n"
)]
fn dataclass_factory_share_rejects_uncertain_class_or_factory_identity_scenario_2(
    #[case] source: &str,
) {
    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "dataclass_factory_share"),
        "{source}"
    );
}

#[test]
fn dataclass_factory_share_uses_shared_caps_and_line_selection() {
    use super::analyze_selected_with;
    use camino::Utf8Path;
    use hoimin_core::LineRange;
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n first: list = field(default_factory=list)\n second: dict = field(default_factory=dict)\n";
    let limited = analyze_selected_with(
        &["dataclass_factory_share"],
        Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        source,
    );

    assert_eq!(limited.candidates.len(), 1);
    assert!(limited.truncated);
    let selected = analyze_selected_with(
        &["dataclass_factory_share"],
        Utf8Path::new("a.py"),
        &[LineRange { start: 5, end: 5 }],
        &[],
        10,
        source,
    );
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].original, "dict");
    assert!(!selected.truncated);
}

#[test]
fn dataclass_guard_does_not_relax_ordinary_callable_resolution() {
    use super::super::callable_imports::CallableImports;
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let imports = CallableImports::build(parsed.syntax(), &|| false, &["dataclasses"]).unwrap();
    let class = parsed.syntax().body[1].as_class_def_stmt().unwrap();
    let field = class.body[0].as_ann_assign_stmt().unwrap();
    let call = field.value.as_deref().unwrap().as_call_expr().unwrap();

    assert!(imports.resolve(&call.func).is_none());
    let guard = imports
        .ordinary_dataclass_namespace(class, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        imports
            .resolve_in_ordinary_dataclass(&call.func, &guard)
            .unwrap()
            .member,
        "field"
    );
    assert!(imports.resolve(&call.func).is_none());
}

#[test]
fn dataclass_factory_share_build_observes_cancellation() {
    use super::super::dataclass_factory::DataclassFactories;
    use hoimin_core::MutationOperatorSelection;
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();

    let actual = DataclassFactories::build(
        parsed.syntax(),
        &MutationOperatorSelection::default(),
        &|| true,
    )
    .is_err();

    assert!(actual);
}

#[rstest::rstest]
#[case(true, 0)]
#[case(false, 1)]
fn dataclass_factory_share_emission_observes_cancellation(
    #[case] cancelled: bool,
    #[case] expected_count: usize,
) {
    use super::super::{NameResolutionIndex, dataclass_factory::DataclassFactories};
    use hoimin_core::MutationOperatorSelection;
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let factories = DataclassFactories::build(
        parsed.syntax(),
        &MutationOperatorSelection::default(),
        &|| false,
    )
    .unwrap();
    let resolution = NameResolutionIndex::from_module(parsed.syntax());
    let class = parsed.syntax().body[1].as_class_def_stmt().unwrap();
    let mut replacements = Vec::new();
    let result = factories.emit_class(class, &resolution, &|| cancelled, |_, replacement, _| {
        replacements.push(replacement);
        true
    });

    let actual = result.is_err();
    let actual_2 = replacements.len();

    assert_eq!(actual, cancelled);
    assert_eq!(actual_2, expected_count);
    assert_eq!(replacements, ["lambda shared=[]: shared"][..expected_count]);
}

#[test]
fn dataclass_factory_share_allows_ordinary_declarations_and_local_helper_collision() {
    let source = "from dataclasses import dataclass, field\n@dataclass()\nclass Example():\n 'description'\n count: int\n shared: list = field(default_factory=list, init=False)\n pass\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "dataclass_factory_share")
        .collect();

    assert_eq!(candidates.len(), 1);
    apply_candidate_and_reparse(source, candidates[0]);
}

#[test]
fn dataclass_factory_share_allows_module_rebinding_after_class_evaluation() {
    // The field captures the builtin eagerly while the class suite runs. A later
    // module binding cannot change that captured factory's identity.
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\nlist = custom\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "dataclass_factory_share")
        .collect();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].original, "list");
    assert_eq!(candidates[0].replacement, "lambda shared=[]: shared");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[rstest::rstest]
#[case("default_factory=list, default_factory=dict")]
#[case("default_factory=list, default_factory=list")]
fn repeated_factory_keywords_never_produce_a_shared_default(#[case] keywords: &str) {
    let source = format!(
        "from dataclasses import dataclass, field\n@dataclass\nclass C:\n values: list = field({keywords})\n"
    );
    ruff_python_parser::parse_module(&source).unwrap();
    let output = super::analyze_selected(&["dataclass_factory_share"], &source);

    assert_eq!(output.diagnostics, Vec::new());
    assert_eq!(output.candidates, Vec::new());
}
