use std::cell::Cell;
use std::fmt::Write as _;

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Init,
    Dependency,
    EdgeBuild,
    ForwardRoot,
    Forward,
    ReverseRoot,
    ReverseNode,
    ReverseEdge,
    DepthComponent,
    DepthNode,
    DepthEdge,
    Classify,
    Apply,
}
thread_local! { static WORK: Cell<[usize; 13]> = const { Cell::new([0; 13]) }; }
pub(super) fn record(phase: Phase) {
    WORK.with(|work| {
        let mut counts = work.get();
        counts[phase as usize] += 1;
        work.set(counts);
    });
}
fn reset_work() {
    WORK.with(|work| work.set([0; 13]));
}
const CLASSES: &str =
    "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n";

fn from_imports(prefix: &str, size: usize) -> String {
    let mut source = String::new();
    for to in 0..size {
        writeln!(source, "from {prefix}{to:03} import sentinel as s{to:03}").unwrap();
    }
    source
}

fn layered_sources(size: usize, missing: bool) -> Vec<(Utf8PathBuf, String)> {
    let middle = if missing { "x" } else { "m" };
    let leaf = if missing { "y" } else { "l" };
    let mut sources = vec![("target.py".into(), CLASSES.to_owned())];
    for node in 0..size {
        sources.push((format!("a{node:03}.py").into(), from_imports(middle, size)));
        sources.push((format!("m{node:03}.py").into(), from_imports(leaf, size)));
        sources.push((format!("l{node:03}.py").into(), "sentinel = 0\n".into()));
    }
    sources
}
macro_rules! layered_case {
    ($name:ident, $n:expr, $missing:expr, $bytes:expr) => {
        #[test]
        fn $name() {
            // Arrange: the issue's literal three-layer workload and local hierarchy.
            let sources = layered_sources($n, $missing);
            let vertices = 3 * $n + 1;
            let dependencies = 2 * $n * $n;
            let edges = if $missing { 0 } else { dependencies };
            assert_eq!(sources.len(), vertices);
            assert_eq!(sources.iter().map(|(_, s)| s.len()).sum::<usize>(), $bytes);
            reset_work();

            // Act: include the real module-name/source adapter.
            let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
            let work = WORK.with(Cell::get);
            let total: usize = work.iter().sum();
            eprintln!("N={} missing={} work={work:?} total={total}", $n, $missing);

            // Assert: fixed input-derived work ceiling and ordinary visibility.
            assert_eq!(index.visible[Utf8Path::new("target.py")].len(), 2);
            assert_eq!(work[Phase::Dependency as usize], dependencies);
            assert_eq!(work[Phase::EdgeBuild as usize], edges);
            assert_eq!(work[Phase::Forward as usize], vertices + edges);
            assert_eq!(work[Phase::ReverseEdge as usize], edges);
            assert_eq!(work[Phase::DepthEdge as usize], edges);
            assert!(
                total <= 16 * vertices + dependencies + 4 * edges + 1,
                "{total} exceeds graph work budget"
            );
        }
    };
}
layered_case!(layered_dag_16, 16, false, 17_695);
layered_case!(layered_dag_32, 32, false, 70_127);
layered_case!(layered_dag_64, 64, false, 279_439);
layered_case!(layered_dag_128, 128, false, 1_115_855);
layered_case!(layered_control_16, 16, true, 17_695);
layered_case!(layered_control_32, 32, true, 70_127);
layered_case!(layered_control_64, 64, true, 279_439);
layered_case!(layered_control_128, 128, true, 1_115_855);

fn joined_depth_sources(first: &str) -> Vec<(Utf8PathBuf, String)> {
    let mut sources = vec![
        (
            "r.py".into(),
            format!("import {first}\nimport x\n{CLASSES}"),
        ),
        ("x.py".into(), "import leaf\n".into()),
        ("leaf.py".into(), String::new()),
    ];
    for step in 1..=254 {
        let name = if step == 1 {
            first.to_owned()
        } else {
            format!("n{step:03}")
        };
        let next = if step == 254 {
            "x".to_owned()
        } else {
            format!("n{:03}", step + 1)
        };
        sources.push((format!("{name}.py").into(), format!("import {next}\n")));
    }
    sources
}
macro_rules! joined_depth_case {
    ($name:ident, $first:expr) => {
        #[test]
        fn $name() {
            // Arrange: length-256 path with a short join into its last two nodes.
            let sources = joined_depth_sources($first);
            // Act.
            let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
            // Assert: the bound is independent of import-name DFS order.
            assert!(index.modules[Utf8Path::new("r.py")].dynamic);
        }
    };
}
joined_depth_case!(joined_depth_short_branch_first_rejects, "n001");
joined_depth_case!(joined_depth_long_branch_first_rejects, "z001");

fn source_index(sources: &[(&str, &str)]) -> ExceptionIndex {
    let sources = sources
        .iter()
        .map(|(path, source)| (Utf8PathBuf::from(*path), (*source).to_owned()))
        .collect::<Vec<_>>();
    ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap()
}

macro_rules! source_case {
    ($name:ident, $sources:expr, $target:expr, $expected:expr, $reason:expr) => {
        #[test]
        fn $name() {
            // Arrange: real source parsing, normalization and visibility precede collection.
            let sources: &[(&str, &str)] = $sources;
            let index = source_index(sources);
            let source = sources.iter().find(|(path, _)| *path == $target).unwrap().1;
            let parsed = depth::parse_checked(source, &|| false).unwrap();
            let mut replacements = Vec::new();
            // Act.
            let report = index
                .collect(
                    Utf8Path::new($target),
                    parsed.syntax(),
                    100,
                    &|| false,
                    |_, spelling| {
                        replacements.push(spelling);
                        Ok(())
                    },
                )
                .unwrap();
            // Assert: the exact candidate spelling and existing diagnostic contract.
            let expected: &[&str] = $expected;
            assert_eq!(replacements, expected);
            let expected_reason: Option<&str> = $reason;
            if let Some(reason) = expected_reason {
                assert!(report.diagnostic().unwrap().1.contains(reason));
            } else {
                assert!(report.diagnostic().is_none());
                assert_eq!(index.visible[Utf8Path::new($target)].len(), 2);
            }
        }
    };
}
source_case!(
    source_dag_keeps_local_candidate,
    &[
        (
            "target.py",
            concat!(
                "import leaf\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("leaf.py", "sentinel = 0\n")
    ],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_diamond_keeps_local_candidate,
    &[
        (
            "target.py",
            concat!(
                "import a\nimport b\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("a.py", "import leaf\n"),
        ("b.py", "import leaf\n"),
        ("leaf.py", "")
    ],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_cycle_prefix_keeps_local_candidate,
    &[
        (
            "target.py",
            concat!(
                "import a\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("a.py", "import b\n"),
        ("b.py", "import a\n")
    ],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_self_cycle_skips_untrusted_module,
    &[(
        "target.py",
        concat!(
            "import target\n",
            "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
        )
    )],
    "target.py",
    &[],
    Some("untrusted_module=1")
);
source_case!(
    source_mutual_cycle_skips_untrusted_module,
    &[
        (
            "target.py",
            concat!(
                "import other\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("other.py", "import target\n")
    ],
    "target.py",
    &[],
    Some("untrusted_module=1")
);
source_case!(
    source_lazy_back_edge_is_not_cycle,
    &[
        (
            "target.py",
            concat!(
                "import other\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("other.py", "def load():\n    import target\n")
    ],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_dynamic_module_stays_untrusted,
    &[(
        "target.py",
        concat!(
            "exec('pass')\n",
            "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
        )
    )],
    "target.py",
    &[],
    Some("untrusted_module=1")
);
source_case!(
    source_dynamic_vertex_still_participates_in_cycle,
    &[
        (
            "target.py",
            concat!(
                "import other\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("other.py", "exec('pass')\nimport target\n")
    ],
    "target.py",
    &[],
    Some("untrusted_module=1")
);
source_case!(
    source_missing_import_has_no_graph_edge,
    &[(
        "target.py",
        concat!(
            "import absent\n",
            "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
        )
    )],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_unparsed_destination_has_no_graph_edge,
    &[
        (
            "target.py",
            concat!(
                "import broken\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("broken.py", "def invalid ???\n")
    ],
    "target.py",
    &["Root"],
    None
);
source_case!(
    source_imported_cycle_class_remains_unresolved,
    &[
        (
            "target.py",
            "from other import Root, Child\ndef f():\n    raise Child()\n"
        ),
        (
            "other.py",
            concat!(
                "import other\n",
                "class Root(Exception): pass\nclass Child(Root): pass\n"
            )
        )
    ],
    "target.py",
    &[],
    Some("unresolved_binding=1")
);
source_case!(
    source_relative_cycle_prefix_keeps_local_candidate,
    &[
        ("pkg/__init__.py", ""),
        (
            "pkg/target.py",
            concat!(
                "from .a import sentinel\n",
                "class Root(Exception): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
            )
        ),
        ("pkg/a.py", "from .b import sentinel\n"),
        ("pkg/b.py", "from .a import sentinel\n")
    ],
    "pkg/target.py",
    &["Root"],
    None
);

#[test]
fn aliases_to_same_path_share_vertex_identity() {
    // Arrange: distinct normalized names resolve to the same parsed destination.
    let mut index = ExceptionIndex::default();
    let mut target = Module::default();
    target.dependencies.insert(
        ("one".into(), 0),
        ImportUse {
            eager: true,
            mutated: false,
        },
    );
    target.dependencies.insert(
        ("two".into(), 0),
        ImportUse {
            eager: true,
            mutated: false,
        },
    );
    index.modules.insert("target.py".into(), target);
    index.modules.insert("same.py".into(), Module::default());
    index.module_paths.insert("one".into(), "same.py".into());
    index.module_paths.insert("two".into(), "same.py".into());
    reset_work();
    // Act.
    index.exclude_import_cycles(&|| false).unwrap();
    // Assert: parallel edges are counted but neither creates a cycle.
    assert!(index.modules.values().all(|module| !module.dynamic));
    assert_eq!(WORK.with(Cell::get)[Phase::EdgeBuild as usize], 2);
}

macro_rules! cancellation_case {
    ($name:ident, $phase:ident, $sources:expr) => {
        #[test]
        fn $name() {
            // Arrange: count actual stage work; no time/thread scheduling assumption.
            let sources = $sources;
            reset_work();
            // Act.
            let result = ExceptionIndex::from_sources(&sources, &[".".into()], &|| {
                WORK.with(Cell::get)[Phase::$phase as usize] >= 16
            });
            // Assert: no successful partial index escapes, at the exact work boundary.
            assert!(matches!(result, Err(AnalysisError::Cancelled)));
            assert_eq!(WORK.with(Cell::get)[Phase::$phase as usize], 16);
        }
    };
}
cancellation_case!(
    cancel_during_node_initialization,
    Init,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_dependency_scan,
    Dependency,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_edge_construction,
    EdgeBuild,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_forward_fanout,
    Forward,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_reverse_fanin,
    ReverseEdge,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_depth_fanout,
    DepthEdge,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_during_final_application,
    Apply,
    layered_sources(16, false)
);
cancellation_case!(
    cancel_zero_edge_components,
    DepthComponent,
    (0..64)
        .map(|n| (Utf8PathBuf::from(format!("m{n:03}.py")), String::new()))
        .collect::<Vec<_>>()
);
cancellation_case!(
    cancel_all_lazy_dependency_scan,
    Dependency,
    vec![(
        Utf8PathBuf::from("target.py"),
        format!(
            "def load():\n{}",
            (0..64).fold(String::new(), |mut source, n| {
                writeln!(source, "    import absent{n:03}").unwrap();
                source
            })
        )
    )]
);

#[test]
fn already_cancelled_empty_graph_returns_error() {
    // Arrange.
    let mut index = ExceptionIndex::default();
    // Act.
    let result = index.exclude_import_cycles(&|| true);
    // Assert.
    assert!(matches!(result, Err(AnalysisCancelled)));
}

fn source_boundary(size: usize, cyclic: bool) -> Vec<(Utf8PathBuf, String)> {
    let mut sources = vec![("target.py".into(), format!("import n001\n{CLASSES}"))];
    for vertex in 1..=size {
        let next = if vertex < size { vertex + 1 } else { 1 };
        let source = if vertex < size || cyclic {
            format!("import n{next:03}\n")
        } else {
            String::new()
        };
        sources.push((format!("n{vertex:03}.py").into(), source));
    }
    sources
}
macro_rules! source_boundary_case {
    ($name:ident, $size:expr, $cyclic:expr, $excluded:expr) => {
        #[test]
        fn $name() {
            // Arrange: the target has local classes unrelated to its imports.
            let sources = source_boundary($size, $cyclic);
            let source = &sources[0].1;
            let parsed = depth::parse_checked(source, &|| false).unwrap();
            let mut replacements = Vec::new();
            // Act: traverse actual source acquisition into collection.
            let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
            let report = index
                .collect(
                    Utf8Path::new("target.py"),
                    parsed.syntax(),
                    100,
                    &|| false,
                    |_, spelling| {
                        replacements.push(spelling);
                        Ok(())
                    },
                )
                .unwrap();
            // Assert: real depth boundaries affect candidates and the existing reason.
            if $excluded {
                assert!(replacements.is_empty());
                assert!(
                    report
                        .diagnostic()
                        .unwrap()
                        .1
                        .contains("untrusted_module=1")
                );
            } else {
                assert_eq!(replacements, ["Root"]);
                assert!(report.diagnostic().is_none());
            }
        }
    };
}
source_boundary_case!(source_depth_255_keeps_candidate, 255, false, false);
source_boundary_case!(source_depth_256_excludes_candidate, 256, false, true);
source_boundary_case!(
    source_prefix_into_254_cycle_keeps_candidate,
    254,
    true,
    false
);
source_boundary_case!(
    source_prefix_into_255_cycle_excludes_candidate,
    255,
    true,
    true
);

#[test]
fn normalized_relative_cycle_excludes_only_its_members() {
    // Arrange: package-qualified aliases must converge on the same two vertices.
    let sources = [
        ("pkg/__init__.py", ""),
        ("pkg/target.py", "from .a import sentinel\n"),
        ("pkg/a.py", "from .b import sentinel\n"),
        ("pkg/b.py", "from .a import sentinel\n"),
    ];
    // Act.
    let index = source_index(&sources);
    // Assert.
    let excluded = index
        .modules
        .iter()
        .filter(|(_, module)| module.dynamic)
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(excluded, ["pkg/a.py", "pkg/b.py"]);
}
