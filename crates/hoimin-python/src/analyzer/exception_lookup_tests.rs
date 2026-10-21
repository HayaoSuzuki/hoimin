use super::*;
use std::cell::Cell;
use std::fmt::Write as _;

thread_local! {
    pub(super) static PRIVATE_ALIAS_VISITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static RELATED_VISITS: Cell<usize> = const { Cell::new(0) };
}

#[test]
fn private_alias_scan_is_independent_of_class_count() {
    for aliases in [1, 32, 128] {
        for classes in [0, 1, 8, 32] {
            let mut source = String::from("from errors import Base\n");
            for i in 0..aliases {
                writeln!(source, "from errors import Child as a{i}").unwrap();
            }
            // A private spelling exercises the cached subset; public aliases are
            // still usable in methods and function-local exclusion is restored.
            source.push_str("from errors import Child as __hidden\n");
            for i in 0..classes {
                writeln!(source, "class C{i}:\n def f(self):\n  raise a0()").unwrap();
            }
            let sources = vec![
                (
                    "errors.py".into(),
                    "class Base(Exception): pass\nclass Child(Base): pass\n".into(),
                ),
                ("service.py".into(), source.clone()),
            ];
            let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
            let parsed = ruff_python_parser::parse_module(&source).unwrap();
            PRIVATE_ALIAS_VISITS.set(0);
            let mut candidates = Vec::new();
            index
                .collect(
                    Utf8Path::new("service.py"),
                    parsed.syntax(),
                    1000,
                    &|| false,
                    |_, replacement| {
                        candidates.push(replacement);
                        Ok(())
                    },
                )
                .unwrap();
            let visible_aliases: usize = index.visible[Utf8Path::new("service.py")]
                .values()
                .map(Vec::len)
                .sum();
            assert!(
                PRIVATE_ALIAS_VISITS.get() <= visible_aliases,
                "classes={classes}, aliases={aliases}, visits={}",
                PRIVATE_ALIAS_VISITS.get()
            );
            assert_eq!(candidates, vec!["Base"; classes]);
        }
    }
}

impl ExceptionIndex {
    fn exhaustive_replacements(
        &self,
        path: &Utf8Path,
        name: &str,
        before: usize,
        raised: bool,
        excluded: &BTreeSet<String>,
        budget: (usize, &dyn Fn() -> bool),
    ) -> SiteOutcome<'_> {
        if excluded.contains(name.split('.').next().unwrap()) {
            return SiteOutcome::Skipped(SkipReason::UnresolvedBinding);
        }
        if self.modules.get(path).is_some_and(|module| module.dynamic) {
            return SiteOutcome::Skipped(SkipReason::UntrustedModule);
        }
        let Some(id) = self.binding_id(path, name, before) else {
            return if self.builtin_base(path, name, before).is_some() {
                SiteOutcome::Ineligible(IneligibleReason::NoUserDefinedClass)
            } else {
                SiteOutcome::Skipped(SkipReason::UnresolvedBinding)
            };
        };
        let Some(class) = self.classes.get(&id) else {
            return SiteOutcome::Skipped(SkipReason::UnresolvedBinding);
        };
        if raised && !class.constructible {
            return SiteOutcome::Ineligible(IneligibleReason::ConstructorPolicy);
        }
        let siblings = class.parent.as_ref().and_then(|p| self.children.get(p));
        let children = self.children.get(&id);
        let related = class
            .parent
            .iter()
            .chain(siblings.into_iter().flat_map(|s| s.ids.iter()))
            .chain(children.into_iter().flat_map(|c| c.ids.iter()));
        let mut result = BTreeSet::new();
        let mut has_related = false;
        let mut compatible = false;
        for destination in related {
            if (budget.1)() {
                break;
            }
            if *destination == id {
                continue;
            }
            has_related = true;
            if raised
                && !self
                    .classes
                    .get(destination)
                    .is_some_and(|c| c.constructible)
            {
                continue;
            }
            compatible = true;
            if let Some(aliases) = self.visible.get(path).and_then(|v| v.get(destination))
                && let Some((name, _)) = aliases.iter().find(|(n, end)| {
                    *end <= before && !excluded.contains(n.split('.').next().unwrap())
                })
            {
                result.insert(name.as_str());
                if result.len() > budget.0.saturating_add(1) {
                    result.pop_last();
                }
            }
        }
        if result.is_empty() {
            SiteOutcome::Ineligible(if !has_related {
                IneligibleReason::NoRelatedClass
            } else if !compatible {
                IneligibleReason::ConstructorPolicy
            } else {
                IneligibleReason::NoVisibleDestination
            })
        } else {
            SiteOutcome::Candidates(result.into_iter().collect())
        }
    }
}

#[test]
fn related_lookup_work_is_bounded_by_visible_classes() {
    for siblings in [8, 64, 256] {
        let mut definitions = String::from("class Base(Exception): pass\n");
        for i in 0..siblings {
            writeln!(definitions, "class C{i}(Base): pass").unwrap();
        }
        let sources = vec![
            ("errors.py".into(), definitions),
            ("service.py".into(), "from errors import C0, C1\n".into()),
        ];
        let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
        for sites in [1, 8, 32] {
            RELATED_VISITS.set(0);
            for _ in 0..sites {
                assert_eq!(
                    index.replacements(
                        Utf8Path::new("service.py"),
                        "C0",
                        usize::MAX,
                        true,
                        &BTreeSet::new(),
                        (10, &|| false)
                    ),
                    SiteOutcome::Candidates(vec!["C1"])
                );
            }
            assert!(
                RELATED_VISITS.get() <= 2 * sites,
                "siblings={siblings}, sites={sites}, visits={}",
                RELATED_VISITS.get()
            );
        }
    }
}

#[test]
fn related_lookup_matches_exhaustive_outcomes() {
    let sources = vec![
        ("errors.py".into(), "class Base(Exception): pass\nclass A(Base): pass\nclass B(Base):\n def __init__(self, required): pass\nclass C(A): pass\nclass Lonely(Exception): pass\n".into()),
        ("sparse.py".into(), "from errors import A as z, A as a, B\n".into()),
        ("isolated.py".into(), "from errors import A\n".into()),
    ];
    let index = ExceptionIndex::from_sources(&sources, &[".".into()], &|| false).unwrap();
    for (path, visible) in &index.visible {
        for aliases in visible.values() {
            for (name, _) in aliases {
                for raised in [false, true] {
                    for before in [0, usize::MAX] {
                        for limit in [0, 1, 10] {
                            for excluded in
                                [BTreeSet::new(), BTreeSet::from(["a".into(), "Base".into()])]
                            {
                                let actual = index.replacements(
                                    path,
                                    name,
                                    before,
                                    raised,
                                    &excluded,
                                    (limit, &|| false),
                                );
                                let expected = index.exhaustive_replacements(
                                    path,
                                    name,
                                    before,
                                    raised,
                                    &excluded,
                                    (limit, &|| false),
                                );
                                assert_eq!(
                                    actual, expected,
                                    "{path} {name} raised={raised} before={before} limit={limit}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
