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

#[test]
fn dataclass_factory_share_preserves_factory_options_aliases_and_grouping() {
    for (imports, decorator, field) in [
        (
            "from dataclasses import dataclass, field",
            "dataclass",
            "field",
        ),
        (
            "from dataclasses import dataclass as dc, field as item",
            "dc(frozen=True)",
            "item",
        ),
        (
            "import dataclasses as dc",
            "dc.dataclass(slots=True, kw_only=True)",
            "dc.field",
        ),
    ] {
        for (factory, replacement) in [
            ("list", "lambda shared=[]: shared"),
            ("dict", "lambda shared={}: shared"),
            ("set", "lambda shared={*()}: shared"),
        ] {
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
            for preserved in [
                "# factory",
                "# preserve",
                "repr=False, compare=False",
                "結果: object",
            ] {
                assert!(changed.contains(preserved), "{changed}");
            }
        }
    }
}

#[test]
fn dataclass_factory_share_rejects_uncertain_class_or_factory_identity() {
    let prefix = "from dataclasses import dataclass, field\n";
    for body in [
        "class Example:\n values: list = field(default_factory=list)\n",
        "@dataclass(**options)\nclass Example:\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n def method(self):\n  return self.values\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n values: dict = field(default_factory=dict)\n",
        "@other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example(Base):\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example(metaclass=Meta):\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example[T]:\n values: list = field(default_factory=list)\n",
        "def outer():\n @dataclass\n class Example:\n  values: list = field(default_factory=list)\n",
        "if flag:\n @dataclass\n class Example:\n  values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example:\n values = field(default_factory=list)\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=custom)\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=lambda: [])\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list())\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list, **options)\n",
        "@dataclass\nclass Example:\n values: list = field([], default_factory=list)\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n field: object = other\n",
        "@dataclass\nclass Example:\n list: object = custom\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example:\n values: list = field(default_factory=list)\n list: object = custom\n",
        "@dataclass\nclass Example:\n locals()['field'] = other\n values: list = field(default_factory=list)\n",
        "@dataclass\nclass Example:\n __annotations__: object = mapping\n values: list = field(default_factory=list)\n",
        "dataclass = other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
        "field = other\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
        "list = custom\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
        "from elsewhere import *\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
        "exec(code)\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n",
    ] {
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
    for source in [
        "import dataclasses as dc\ndc.field = other\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n",
        "import dataclasses as dc\nexpose(dc)\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n",
        "import dataclasses as dc\n@dc.dataclass\nclass A:\n x: list = dc.field(default_factory=list)\n dc: object = other\n",
        "from elsewhere import dataclass, field\n@dataclass\nclass A:\n x: list = field(default_factory=list)\n",
    ] {
        let output = analyze(source);
        assert!(
            output
                .candidates
                .iter()
                .all(|c| c.operator.as_str() != "dataclass_factory_share"),
            "{source}"
        );
    }
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
    use ruff_python_ast::{Expr, Stmt};
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let imports = CallableImports::build(parsed.syntax(), &|| false, &["dataclasses"]).unwrap();
    let Stmt::ClassDef(class) = &parsed.syntax().body[1] else {
        panic!("class");
    };
    let Stmt::AnnAssign(field) = &class.body[0] else {
        panic!("field");
    };
    let Expr::Call(call) = field.value.as_deref().unwrap() else {
        panic!("call");
    };
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
fn dataclass_factory_share_build_and_emission_observe_cancellation() {
    use super::super::{NameResolutionIndex, dataclass_factory::DataclassFactories};
    use hoimin_core::MutationOperatorSelection;
    use ruff_python_ast::Stmt;
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Example:\n values: list = field(default_factory=list)\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let operators = MutationOperatorSelection::default();
    assert!(DataclassFactories::build(parsed.syntax(), &operators, &|| true).is_err());
    let factories = DataclassFactories::build(parsed.syntax(), &operators, &|| false).unwrap();
    let resolution = NameResolutionIndex::from_module(parsed.syntax());
    let Stmt::ClassDef(class) = &parsed.syntax().body[1] else {
        panic!("class");
    };
    assert!(
        factories
            .emit_class(class, &resolution, &|| true, |_, _, _| panic!(
                "cancelled emission"
            ))
            .is_err()
    );
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
