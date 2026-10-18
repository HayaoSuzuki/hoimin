use std::cell::Cell;

use super::*;

thread_local! {
    static MODULE_VISITS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_module_visit() {
    MODULE_VISITS.with(|visits| visits.set(visits.get() + 1));
}

#[test]
fn external_import_visibility_work_tracks_imports_not_project_mappings() {
    // A whole-project scan for every external import must exceed this bound.
    for (form, ceilings) in [
        ("module", vec![1024, 2048, 4096, 8192]),
        ("member", vec![0, 0, 0, 0]),
    ] {
        let mut measured = Vec::new();
        for size in [32, 64, 128, 256] {
            // Arrange: a real hierarchy prevents an empty-class shortcut.
            let mut sources = vec![(
                Utf8PathBuf::from("target.py"),
                "class Root(Exception): pass\nclass Child(Root): pass\ndef target():\n    raise Child()\n".to_owned(),
            )];
            let imports = (0..32)
                .map(|alias| match form {
                    "module" => format!("import os as ext{alias}\n"),
                    _ => format!("from os import sep as ext{alias}\n"),
                })
                .collect::<String>();
            sources.extend(
                (0..size)
                    .map(|file| (Utf8PathBuf::from(format!("m{file:04}.py")), imports.clone())),
            );
            MODULE_VISITS.with(|visits| visits.set(0));

            // Act: use the normal source-to-index construction path.
            let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
            measured.push(MODULE_VISITS.with(Cell::get));

            // Assert: unrelated imports retain the target's visible classes.
            let visible = &index.visible[Utf8Path::new("target.py")];
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[&(Utf8PathBuf::from("target.py"), "Root".to_owned())][0].0,
                "Root"
            );
        }
        eprintln!("{form} visibility module visits at N=32/64/128/256: {measured:?}");
        for (visits, ceiling) in measured.into_iter().zip(ceilings) {
            assert!(
                visits <= ceiling,
                "{form}: {visits} visits exceed {ceiling}"
            );
        }
    }
}

#[test]
fn loaded_descendant_classes_keep_package_and_exact_alias_spellings_in_order() {
    // Arrange: `zz` loads the child, while `p` exposes that loaded descendant.
    let sources = [
        ("pkg/__init__.py".into(), "class Root(Exception): pass\n".into()),
        (
            "pkg/errors.py".into(),
            "from pkg import Root\nclass Child(Root): pass\n".into(),
        ),
        (
            "service.py".into(),
            "import pkg as p\nimport pkg.errors as zz\nfrom pkg import Root\ndef target():\n    raise Root()\n".into(),
        ),
    ];

    // Act.
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
    let service = Utf8Path::new("service.py");
    let parsed = depth::parse_checked(&sources[2].1, &|| false).unwrap();
    let mut replacements = Vec::new();
    let report = index
        .collect(service, parsed.syntax(), 100, &|| false, |_, spelling| {
            replacements.push(spelling);
            Ok(())
        })
        .unwrap();

    // Assert: lexical spelling order still picks the package descendant.
    let aliases = &index.visible[service][&(Utf8PathBuf::from("pkg/errors.py"), "Child".into())];
    assert_eq!(
        aliases
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["p.errors.Child", "zz.Child"]
    );
    assert_eq!(replacements, ["p.errors.Child"]);
    assert!(report.diagnostic().is_none());
}

#[test]
fn visibility_cancels_while_visiting_loaded_modules_with_no_members() {
    // Arrange: isolate this stage so empty targets cannot defer cancellation
    // until the next binding or class member.
    let mut index = ExceptionIndex::default();
    let mut service = Module::default();
    service.insert(
        "p".to_owned(),
        BindingKind::Import {
            module: "pkg".to_owned(),
            member: None,
            level: 0,
        },
        0,
    );
    for child in 0..128 {
        let name = format!("pkg.child{child:03}");
        let path = Utf8PathBuf::from(format!("pkg/child{child:03}.py"));
        service.loaded.insert(name.clone(), 0);
        index.module_paths.insert(name, path.clone());
        index.modules.insert(path, Module::default());
    }
    index.modules.insert("service.py".into(), service);
    MODULE_VISITS.with(|visits| visits.set(0));

    // Act.
    let result = index.build_visible(&|| MODULE_VISITS.with(|visits| visits.get() >= 16));

    // Assert.
    assert!(matches!(result, Err(AnalysisError::Cancelled)));
    assert_eq!(MODULE_VISITS.with(Cell::get), 16);
}
