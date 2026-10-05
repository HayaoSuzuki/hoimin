fn config(root: &std::path::Path) -> hoimin_core::RunConfig {
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

#[tokio::test]
#[allow(clippy::too_many_lines)] // Paired syntax fixtures form one acceptance matrix.
async fn hierarchy_scoped_writes_distinguish_local_bindings_and_escaping_aliases() {
    let cases = [
        (
            "parameter",
            "import errors as e\ndef patch(e):\n    other = e\n    other.Root = object\n",
            true,
        ),
        (
            "renamed-parameter",
            "import errors as e\ndef patch(value):\n    other = value\n    other.Root = object\n",
            true,
        ),
        (
            "local-import",
            "import errors as e\ndef patch():\n    import unrelated as e\n    e.Root = object\n",
            true,
        ),
        (
            "local-errors-import",
            "import unrelated as e\ndef patch():\n    import errors as e\n    e.Root = object\n",
            false,
        ),
        (
            "free",
            "import errors as e\ndef patch():\n    other = e\n    other.Root = object\n",
            false,
        ),
        (
            "global",
            "import errors as e\ndef patch():\n    global e\n    other = e\n    other.Root = object\n",
            false,
        ),
        (
            "closure-parameter",
            "import errors as e\ndef outer(e):\n    def patch():\n        e.Root = object\n",
            true,
        ),
        (
            "closure-import",
            "def outer():\n    import errors as e\n    def patch():\n        e.Root = object\n",
            false,
        ),
        (
            "nonlocal",
            "def outer():\n    import errors as e\n    def patch():\n        nonlocal e\n        e.Root = object\n",
            false,
        ),
        (
            "late-nonlocal",
            "def outer():\n    def patch():\n        nonlocal e\n        e.Root = object\n    import errors as e\n",
            false,
        ),
        (
            "late-local-shadow",
            "import errors as e\ndef patch():\n    e.Root = object\n    e = object()\n",
            true,
        ),
        (
            "method-skips-class",
            "import errors as e\nclass C:\n    e = object()\n    def patch(self):\n        e.Root = object\n",
            false,
        ),
        (
            "method-closure",
            "import errors as e\ndef outer(e):\n    class C:\n        e = object()\n        def patch(self):\n            e.Root = object\n",
            true,
        ),
        (
            "class-read-fallback",
            "import errors as e\nclass C:\n    other = e\n    e = object()\n    other.Root = object\n",
            false,
        ),
        (
            "private-parameter",
            "import errors as _P__e\nclass P:\n    def patch(self, __e):\n        other = __e\n        other.Root = object\n",
            true,
        ),
        (
            "default-in-outer",
            "import errors as e\ndef patch(e=(other := e)):\n    other.Root = object\n",
            false,
        ),
        (
            "lambda-parameter",
            "import errors as e\nf = lambda e: setattr(e, 'Root', object)\n",
            true,
        ),
        (
            "lambda-free",
            "import errors as e\nf = lambda: setattr(e, 'Root', object)\n",
            false,
        ),
        (
            "comprehension-target",
            "import errors as e\nvalues = [setattr(e, 'Root', object) for e in ()]\n",
            true,
        ),
        (
            "comprehension-free",
            "import errors as e\nvalues = [setattr(e, 'Root', object) for value in ()]\n",
            false,
        ),
        (
            "comprehension-first-iterable",
            "import errors as e\nvalues = [e for e in [setattr(e, 'Root', object)]]\n",
            false,
        ),
        (
            "comprehension-walrus",
            "import errors as e\nvalues = [(other := e) for value in (0,)]\nother.Root = object\n",
            false,
        ),
    ];
    let mut failures = Vec::new();
    for (name, patch, eligible) in cases {
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
        let plan = hoimin_cli::plan::create(config(dir.path())).await.unwrap();
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
        let expected = if eligible {
            vec![("Child", "Root")]
        } else {
            vec![]
        };
        if pairs != expected {
            failures.push(format!("{name}: expected {expected:?}, actual {pairs:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn hierarchy_local_parameter_renaming_preserves_module_candidates() {
    for parameter in ["Root", "value"] {
        let dir = tempfile::tempdir().unwrap();
        let source = format!(
            "class Root(Exception): pass\nclass Child(Root): pass\ndef unrelated({parameter}):\n    other = {parameter}\n    other.tag = 1\ndef target():\n    raise Child()\n"
        );
        std::fs::write(dir.path().join("service.py"), source).unwrap();
        let plan = hoimin_cli::plan::create(config(dir.path())).await.unwrap();
        assert_eq!(plan.manifest.candidates.len(), 1, "{parameter}");
        assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
    }
}

#[tokio::test]
async fn hierarchy_other_local_declarations_and_comprehensions_preserve_candidates() {
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
    let declarations = [
        "for e in (): pass",
        "with object() as e: pass",
        "try: pass\n    except Exception as e: pass",
        "match object():\n        case e: pass",
        "del e",
        "e: object",
        "def e(): pass",
        "class e: pass",
    ];
    for declaration in declarations {
        let source =
            format!("import errors as e\ndef patch():\n    {declaration}\n    e.Root = object\n");
        std::fs::write(dir.path().join("patcher.py"), source).unwrap();
        let plan = hoimin_cli::plan::create(config(dir.path())).await.unwrap();
        assert_eq!(plan.manifest.candidates.len(), 1, "{declaration}");
    }
    for expression in [
        "{setattr(e, 'Root', object) for e in ()}",
        "{e: setattr(e, 'Root', object) for e in ()}",
        "(setattr(e, 'Root', object) for e in ())",
        "[setattr(e, 'Root', object) for e in () for value in ()]",
    ] {
        std::fs::write(
            dir.path().join("patcher.py"),
            format!("import errors as e\nvalues = {expression}\n"),
        )
        .unwrap();
        let plan = hoimin_cli::plan::create(config(dir.path())).await.unwrap();
        assert_eq!(plan.manifest.candidates.len(), 1, "{expression}");
    }
}

#[tokio::test]
async fn hierarchy_implicit_scope_bindings_do_not_hide_module_classes() {
    for body in [
        "values = [Root for Root in ()]\ndef target():\n    raise Child()\n",
        "def target():\n    values = [Root for Root in ()]\n    raise Child()\n",
        "value = lambda: (Root := object())\ndef target():\n    raise Child()\n",
        "def target():\n    value = lambda: (Root := object())\n    raise Child()\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("service.py"),
            format!("class Root(Exception): pass\nclass Child(Root): pass\n{body}"),
        )
        .unwrap();
        let plan = hoimin_cli::plan::create(config(dir.path())).await.unwrap();
        assert_eq!(plan.manifest.candidates.len(), 1, "{body}");
        assert_eq!(plan.manifest.candidates[0].candidate.replacement, "Root");
    }
}
