use std::cell::Cell;
use std::fmt::Write as _;

use proptest::prelude::*;

use super::*;

thread_local! {
    static MODULE_VISITS: Cell<usize> = const { Cell::new(0) };
    static MEMBER_SCANS: Cell<usize> = const { Cell::new(0) };
    static MEMBER_RESOLUTIONS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_module_visit() {
    MODULE_VISITS.with(|visits| visits.set(visits.get() + 1));
}

pub(super) fn record_member_scan() {
    MEMBER_SCANS.with(|visits| visits.set(visits.get() + 1));
}

pub(super) fn record_member_resolution() {
    MEMBER_RESOLUTIONS.with(|visits| visits.set(visits.get() + 1));
}

fn alias_sources(ordinary: usize, aliases: usize, classes: usize) -> Vec<(Utf8PathBuf, String)> {
    let mut target = String::new();
    for member in 0..ordinary {
        writeln!(target, "ordinary{member:04} = 0").unwrap();
    }
    for class in 0..classes {
        writeln!(target, "class Error{class:04}(Exception): pass").unwrap();
    }
    let mut service = String::new();
    for alias in 0..aliases {
        writeln!(service, "import errors as alias{alias:04}").unwrap();
    }
    vec![("errors.py".into(), target), ("service.py".into(), service)]
}

// Each generated test has one independent (ordinary bindings, aliases) case.
macro_rules! member_work_case {
    ($name:ident, $ordinary:expr, $aliases:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn scans_do_not_multiply_ordinary_bindings_by_aliases() {
                // Arrange: no eligible members means no required alias output.
                // A target.bindings scan per alias breaks this operation bound.
                let sources = alias_sources($ordinary, $aliases, 0);
                MEMBER_SCANS.with(|count| count.set(0));

                // Act.
                let index =
                    ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
                let scans = MEMBER_SCANS.with(Cell::get);

                // Assert.
                assert!(index.visible.values().all(BTreeMap::is_empty));
                eprintln!("ordinary={} aliases={} scans={scans}", $ordinary, $aliases);
                assert!(scans <= $ordinary, "{scans} member scans");
            }

            #[test]
            fn unrelated_members_do_not_trigger_alias_resolution() {
                // Arrange: retaining two classes rules out an empty-class shortcut.
                // Filtering after spelling construction/resolution breaks this bound.
                let sources = alias_sources($ordinary, $aliases, 2);
                MEMBER_SCANS.with(|count| count.set(0));
                MEMBER_RESOLUTIONS.with(|count| count.set(0));

                // Act.
                let index =
                    ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
                let resolutions = MEMBER_RESOLUTIONS.with(Cell::get);

                // Assert: required alias × class output remains fully represented.
                let service = &index.visible[Utf8Path::new("service.py")];
                assert_eq!(service.values().map(Vec::len).sum::<usize>(), 2 * $aliases);
                assert!(MEMBER_SCANS.with(Cell::get) <= $ordinary + 2);
                eprintln!(
                    "ordinary={} aliases={} resolutions={resolutions}",
                    $ordinary, $aliases
                );
                assert_eq!(resolutions, 2 * $aliases);
            }
        }
    };
}

member_work_case!(ordinary_32_aliases_1, 32, 1);
member_work_case!(ordinary_32_aliases_8, 32, 8);
member_work_case!(ordinary_32_aliases_32, 32, 32);
member_work_case!(ordinary_128_aliases_1, 128, 1);
member_work_case!(ordinary_128_aliases_8, 128, 8);
member_work_case!(ordinary_128_aliases_32, 128, 32);
member_work_case!(ordinary_512_aliases_1, 512, 1);
member_work_case!(ordinary_512_aliases_8, 512, 8);
member_work_case!(ordinary_512_aliases_32, 512, 32);

type Visibility = BTreeMap<Utf8PathBuf, BTreeMap<ClassId, Vec<(String, usize)>>>;

/// Exhaustive reference: enumerate every mapped module and every target binding.
/// It intentionally has no resolved-member index or loaded-name range traversal.
fn exhaustive_visibility(index: &ExceptionIndex) -> Visibility {
    index
        .modules
        .iter()
        .map(|(path, module)| {
            let mut visible = BTreeMap::<ClassId, Vec<(String, usize)>>::new();
            let mut add = |spelling: String, end| {
                if let Some(id) = index.binding_id(path, &spelling, usize::MAX)
                    && index.classes.contains_key(&id)
                {
                    visible.entry(id).or_default().push((spelling, end));
                }
            };
            for (name, binding) in &module.bindings {
                add(name.clone(), binding.end);
                let BindingKind::Import {
                    module: imported,
                    member: None,
                    ..
                } = &binding.kind
                else {
                    continue;
                };
                for (module_name, module_path) in &index.module_paths {
                    let Some(suffix) = module_name.strip_prefix(imported) else {
                        continue;
                    };
                    if !suffix.is_empty() && !suffix.starts_with('.') {
                        continue;
                    }
                    let Some(loaded_end) = module.loaded.get(module_name) else {
                        continue;
                    };
                    if let Some(target) = index.modules.get(module_path) {
                        for member in target.bindings.keys() {
                            add(
                                format!("{name}{suffix}.{member}"),
                                binding.end.max(*loaded_end),
                            );
                        }
                    }
                }
            }
            for spellings in visible.values_mut() {
                spellings.sort();
            }
            (path.clone(), visible)
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn member_index_preserves_exhaustive_visibility(
        ordinary in 0usize..32,
        aliases in 1usize..8,
        flags in 0u8..64,
    ) {
        // Arrange: independent axes cover loaded descendants, class/alias
        // shadowing, reexports, untrusted importers and ambiguous identities.
        let mut errors = "from pkg import Root\nclass Child(Root): pass\nclass Unrelated(object): pass\n".to_owned();
        for n in 0..ordinary {
            writeln!(errors, "value{n} = 0").unwrap();
        }
        if flags & 2 != 0 {
            errors.push_str("Child = 0\n");
        }
        let mut service = "import pkg as p\nfrom pkg import Root\nclass Local(Exception): pass\n".to_owned();
        if flags & 1 != 0 {
            for alias in 0..aliases {
                writeln!(service, "import pkg.errors as e{alias}").unwrap();
            }
            service.push_str("from pkg.errors import Child as direct\n");
        }
        if flags & 4 != 0 {
            service.push_str("e0 = 0\n");
        }
        if flags & 8 != 0 {
            service.push_str("import facade as exported\n");
        }
        if flags & 16 != 0 {
            service.push_str("exec('unknown = 0')\n");
        }
        if flags & 32 != 0 {
            service.push_str("import errors as ambiguous\n");
        }
        let sources = vec![
            ("pkg/__init__.py".into(), "class Root(Exception): pass\n".into()),
            ("pkg/errors.py".into(), errors),
            ("facade.py".into(), "from pkg.errors import Child\n".into()),
            ("service.py".into(), service),
        ];
        let roots = if flags & 32 != 0 { vec![".".into(), "pkg".into()] } else { vec![".".into()] };

        // Act.
        let index = ExceptionIndex::from_sources(&sources, &roots, &|| false).unwrap();

        // Assert: compare complete class identities, spellings and visibility offsets.
        prop_assert_eq!(&index.visible, &exhaustive_visibility(&index));
    }
}

#[test]
fn qualified_reexports_stay_ineligible_while_direct_imports_remain_visible() {
    // Arrange: caching all locally resolvable names would wrongly admit f.Child.
    let sources = [
        ("errors.py".into(), "class Child(Exception): pass\n".into()),
        ("facade.py".into(), "from errors import Child\n".into()),
        (
            "service.py".into(),
            "import errors as e\nimport facade as f\nfrom errors import Child as direct\n".into(),
        ),
    ];

    // Act.
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();

    // Assert.
    let spellings = &index.visible[Utf8Path::new("service.py")]
        [&(Utf8PathBuf::from("errors.py"), "Child".into())];
    assert_eq!(
        spellings
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["direct", "e.Child"]
    );
}

#[test]
fn visibility_cancels_while_indexing_class_members() {
    // Arrange: cancel during member indexing, before any imported module visit.
    let sources = alias_sources(0, 1, 32);
    let mut index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
    index.visible.clear();
    MEMBER_SCANS.with(|count| count.set(0));
    MODULE_VISITS.with(|count| count.set(0));

    // Act.
    let result = index.build_visible(&|| MEMBER_SCANS.with(|count| count.get() >= 16));

    // Assert.
    assert!(matches!(result, Err(AnalysisError::Cancelled)));
    assert_eq!(MEMBER_SCANS.with(Cell::get), 16);
    assert_eq!(MODULE_VISITS.with(Cell::get), 0);
}

#[test]
fn alias_output_accepts_exact_summary_limit() {
    // Arrange: 256 local spellings plus 255 × 256 alias spellings = 65,536.
    let sources = alias_sources(0, 255, 256);

    // Act.
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();

    // Assert.
    assert_eq!(
        index
            .visible
            .values()
            .flat_map(BTreeMap::values)
            .map(Vec::len)
            .sum::<usize>(),
        65_536
    );
}

#[test]
fn alias_output_rejects_excess_summary_entries() {
    // Arrange: one more alias exceeds the shared local/qualified spelling limit.
    let sources = alias_sources(0, 256, 256);

    // Act.
    let result = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false);

    // Assert.
    assert!(matches!(result, Err(AnalysisError::HierarchyLimit)));
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
