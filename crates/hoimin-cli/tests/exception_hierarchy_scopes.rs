#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
fn config(root: &std::path::Path) -> hoimin_core::RunConfig<hoimin_core::EnvironmentRequest> {
    hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--root",
        root.to_str().unwrap(),
        "--file",
        "service.py",
        "--operators",
        "exception_hierarchy",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--",
        "unused-test-command",
    ])
    .unwrap()
}

#[rstest::rstest]
#[case::parameter_1(0)]
#[case::renamed_parameter_2(1)]
#[case::local_import_3(2)]
#[case::local_errors_import_4(3)]
#[case::free_5(4)]
#[case::global_6(5)]
#[case::closure_parameter_7(6)]
#[case::closure_import_8(7)]
#[case::nonlocal_9(8)]
#[case::late_nonlocal_10(9)]
#[case::late_local_shadow_11(10)]
#[case::method_skips_class_12(11)]
#[case::method_closure_13(12)]
#[case::class_read_fallback_14(13)]
#[case::private_parameter_15(14)]
#[case::default_in_outer_16(15)]
#[case::lambda_parameter_17(16)]
#[case::lambda_free_18(17)]
#[case::comprehension_target_19(18)]
#[case::comprehension_free_20(19)]
#[case::comprehension_first_iterable_21(20)]
#[case::comprehension_walrus_22(21)]
#[tokio::test]
#[allow(clippy::too_many_lines)] // Paired syntax fixtures form one acceptance matrix.
async fn hierarchy_scoped_writes_distinguish_local_bindings_and_escaping_aliases(
    #[case] scenario_index: usize,
) {
    let cases = [
        (
            "parameter",
            "import errors as e\ndef patch(e):\n    other = e\n    other.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "renamed-parameter",
            "import errors as e\ndef patch(value):\n    other = value\n    other.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "local-import",
            "import errors as e\ndef patch():\n    import unrelated as e\n    e.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "local-errors-import",
            "import unrelated as e\ndef patch():\n    import errors as e\n    e.Root = object\n",
            vec![],
        ),
        (
            "free",
            "import errors as e\ndef patch():\n    other = e\n    other.Root = object\n",
            vec![],
        ),
        (
            "global",
            "import errors as e\ndef patch():\n    global e\n    other = e\n    other.Root = object\n",
            vec![],
        ),
        (
            "closure-parameter",
            "import errors as e\ndef outer(e):\n    def patch():\n        e.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "closure-import",
            "def outer():\n    import errors as e\n    def patch():\n        e.Root = object\n",
            vec![],
        ),
        (
            "nonlocal",
            "def outer():\n    import errors as e\n    def patch():\n        nonlocal e\n        e.Root = object\n",
            vec![],
        ),
        (
            "late-nonlocal",
            "def outer():\n    def patch():\n        nonlocal e\n        e.Root = object\n    import errors as e\n",
            vec![],
        ),
        (
            "late-local-shadow",
            "import errors as e\ndef patch():\n    e.Root = object\n    e = object()\n",
            vec![("Child", "Root")],
        ),
        (
            "method-skips-class",
            "import errors as e\nclass C:\n    e = object()\n    def patch(self):\n        e.Root = object\n",
            vec![],
        ),
        (
            "method-closure",
            "import errors as e\ndef outer(e):\n    class C:\n        e = object()\n        def patch(self):\n            e.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "class-read-fallback",
            "import errors as e\nclass C:\n    other = e\n    e = object()\n    other.Root = object\n",
            vec![],
        ),
        (
            "private-parameter",
            "import errors as _P__e\nclass P:\n    def patch(self, __e):\n        other = __e\n        other.Root = object\n",
            vec![("Child", "Root")],
        ),
        (
            "default-in-outer",
            "import errors as e\ndef patch(e=(other := e)):\n    other.Root = object\n",
            vec![],
        ),
        (
            "lambda-parameter",
            "import errors as e\nf = lambda e: setattr(e, 'Root', object)\n",
            vec![("Child", "Root")],
        ),
        (
            "lambda-free",
            "import errors as e\nf = lambda: setattr(e, 'Root', object)\n",
            vec![],
        ),
        (
            "comprehension-target",
            "import errors as e\nvalues = [setattr(e, 'Root', object) for e in ()]\n",
            vec![("Child", "Root")],
        ),
        (
            "comprehension-free",
            "import errors as e\nvalues = [setattr(e, 'Root', object) for value in ()]\n",
            vec![],
        ),
        (
            "comprehension-first-iterable",
            "import errors as e\nvalues = [e for e in [setattr(e, 'Root', object)]]\n",
            vec![],
        ),
        (
            "comprehension-walrus",
            "import errors as e\nvalues = [(other := e) for value in (0,)]\nother.Root = object\n",
            vec![],
        ),
    ];

    let (name, patch, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let dir = tempfile::tempdir().unwrap();
    for (path, source) in [
        (
            "errors.py",
            "class Root(Exception): pass\nclass Child(Root): pass\n",
        ),
        ("unrelated.py", "class Root(Exception): pass\n"),
        (
            "service.py",
            "from errors import Root, Child\ndef target():\n    raise Child()\n",
        ),
        ("patcher.py", patch),
    ] {
        ruff_python_parser::parse_module(source).unwrap();
        std::fs::write(dir.path().join(path), source).unwrap();
    }
    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    let pairs: Vec<_> = plan
        .manifest
        .candidates
        .iter()
        .map(|c| {
            (
                c.candidate.original.as_str(),
                c.candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(pairs, expected, "{name}");
}

#[rstest::rstest]
#[case::root_1("Root")]
#[case::value_2("value")]
#[tokio::test]
async fn hierarchy_local_parameter_renaming_preserves_module_candidates(#[case] parameter: &str) {
    let dir = tempfile::tempdir().unwrap();
    let source = format!(
        "class Root(Exception): pass\nclass Child(Root): pass\ndef unrelated({parameter}):\n    other = {parameter}\n    other.tag = 1\ndef target():\n    raise Child()\n"
    );
    std::fs::write(dir.path().join("service.py"), source).unwrap();

    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), 1, "{parameter}");
    assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
}

#[rstest::rstest]
#[case::for_e_in_pass_1("for e in (): pass")]
#[case::with_object_as_e_pass_2("with object() as e: pass")]
#[case::try_pass_except_exception_as_e_pass_3("try: pass\n    except Exception as e: pass")]
#[case::match_object_case_e_pass_4("match object():\n        case e: pass")]
#[case::del_e_5("del e")]
#[case::e_object_6("e: object")]
#[case::def_e_pass_7("def e(): pass")]
#[case::class_e_pass_8("class e: pass")]
#[tokio::test]
async fn hierarchy_local_declaration_preserves_candidates(#[case] declaration: &str) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("errors.py"),
        "class Root(Exception): pass\nclass Child(Root): pass\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("service.py"),
        "from errors import Root, Child\ndef target():\n    raise Child()\n",
    )
    .unwrap();

    let source =
        format!("import errors as e\ndef patch():\n    {declaration}\n    e.Root = object\n");
    std::fs::write(dir.path().join("patcher.py"), source).unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), 1, "{declaration}");
}

#[rstest::rstest]
#[case::setattr_e_root_object_for_e_in_1("{setattr(e, 'Root', object) for e in ()}")]
#[case::e_setattr_e_root_object_for_e_in_2("{e: setattr(e, 'Root', object) for e in ()}")]
#[case::setattr_e_root_object_for_e_in_3("(setattr(e, 'Root', object) for e in ())")]
#[case::setattr_e_root_object_for_e_in_for_value_in_4(
    "[setattr(e, 'Root', object) for e in () for value in ()]"
)]
#[tokio::test]
async fn hierarchy_comprehension_preserves_candidates(#[case] expression: &str) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("errors.py"),
        "class Root(Exception): pass\nclass Child(Root): pass\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("service.py"),
        "from errors import Root, Child\ndef target():\n    raise Child()\n",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("patcher.py"),
        format!("import errors as e\nvalues = {expression}\n"),
    )
    .unwrap();
    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), 1, "{expression}");
}

#[rstest::rstest]
#[case::values_root_for_root_in_def_target_raise_child_1(
    "values = [Root for Root in ()]\ndef target():\n    raise Child()\n"
)]
#[case::def_target_values_root_for_root_in_raise_child_2(
    "def target():\n    values = [Root for Root in ()]\n    raise Child()\n"
)]
#[case::value_lambda_root_object_def_target_raise_child_3(
    "value = lambda: (Root := object())\ndef target():\n    raise Child()\n"
)]
#[case::def_target_value_lambda_root_object_raise_child_4(
    "def target():\n    value = lambda: (Root := object())\n    raise Child()\n"
)]
#[tokio::test]
async fn hierarchy_implicit_scope_bindings_do_not_hide_module_classes(#[case] body: &str) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("service.py"),
        format!("class Root(Exception): pass\nclass Child(Root): pass\n{body}"),
    )
    .unwrap();

    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), 1, "{body}");
    assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
}

#[rstest::rstest]
#[case::def_patch_self_class_init_lambda_self_required_none_1(
    "def patch(self):\n        __class__.__init__ = lambda self, required: None",
    0
)]
#[case::def_patch_self_alias_class_alias_init_lambda_self_requi_2(
    "def patch(self):\n        alias = __class__\n        alias.__init__ = lambda self, required: None",
    0
)]
#[case::def_patch_self_def_nested_class_init_lambda_self_requir_3(
    "def patch(self):\n        def nested():\n            __class__.__init__ = lambda self, required: None",
    0
)]
#[case::def_patch_self_class_class_init_lambda_self_required_no_4(
    "def patch(self, __class__):\n        __class__.__init__ = lambda self, required: None",
    1
)]
#[case::def_patch_self_global_class_class_init_lambda_self_requ_5(
    "def patch(self):\n        global __class__\n        __class__.__init__ = lambda self, required: None",
    1
)]
#[case::def_patch_self_value_alias_class_alias_init_lambda_self_6(
    "def patch(self, value=(alias := __class__)):\n        alias.__init__ = lambda self, required: None",
    1
)]
#[tokio::test]
async fn hierarchy_implicit_class_cell_writes_invalidate_owning_class(
    #[case] method: &str,
    #[case] expected: usize,
) {
    let dir = tempfile::tempdir().unwrap();
    let source = format!(
        "__class__ = object()\nclass Root(Exception): pass\nclass Child(Root):\n    {method}\ndef target():\n    raise Root()\n"
    );
    std::fs::write(dir.path().join("service.py"), source).unwrap();

    let plan = hoimin_cli::plan::create(
        config(dir.path()),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(plan.manifest.candidates.len(), expected, "{method}");
    for candidate in &plan.manifest.candidates {
        assert_eq!(candidate.candidate.replacement, "Child");
    }
}
