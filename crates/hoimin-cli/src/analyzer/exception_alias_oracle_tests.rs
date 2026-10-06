use super::{Escapes, oracle_corpus as corpus};
use ruff_python_ast::visitor::Visitor;

#[test]
fn lean_exception_hierarchy_alias_extraction_correspondence() {
    let cases = corpus::selected_kind("internal-fixture", Some("alias-extraction"));
    let checked = cases.len();
    let mut mismatches = Vec::new();
    for case in cases {
        let source = &case
            .files
            .iter()
            .find(|(path, _)| path == "patcher.py")
            .unwrap()
            .1;
        let parsed = ruff_python_parser::parse_module(source)
            .unwrap_or_else(|error| panic!("infrastructure-error {}: {error}", case.id));
        let mut extracted = Escapes::default();
        extracted.visit_body(&parsed.syntax().body);
        assert!(
            !extracted.limit_exceeded,
            "infrastructure-error {}",
            case.id
        );
        let encode = |key: super::BindingKey| (key.scope, key.name);
        let affected = extracted.affected_roots().into_iter().map(encode).collect();
        let mut actual = corpus::AliasFacts {
            imports: extracted
                .import_aliases
                .into_iter()
                .map(|(key, origin, level)| (encode(key), origin, level))
                .collect(),
            assignments: extracted
                .assignments
                .into_iter()
                .map(|(target, source)| (encode(target), encode(source)))
                .collect(),
            writes: extracted.attribute_roots.into_iter().map(encode).collect(),
            affected,
        };
        actual.sort();
        let expected = case.expected_alias_facts.as_ref().unwrap();
        if &actual != expected {
            mismatches.push(format!(
                "case={} expected={expected:?} actual={actual:?}",
                case.id
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "semantic mismatches:\n{}",
        mismatches.join("\n")
    );
    eprintln!("exception hierarchy alias extraction: internal-fixture={checked}");
}

#[test]
fn hierarchy_direct_write_roots_have_an_independent_limit() {
    use std::fmt::Write as _;
    let mut source = String::new();
    for index in 0..65_536 {
        writeln!(source, "root{index}.attribute = 0").unwrap();
    }
    source.push_str("root0.attribute = 1\n");
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(super::Module::parse(parsed.syntax()).is_ok());
    source.push_str("additional.attribute = 0\n");
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(matches!(
        super::Module::parse(parsed.syntax()),
        Err(super::AnalysisError::HierarchyLimit)
    ));
}

#[test]
fn hierarchy_scope_count_has_an_independent_limit() {
    // Module scope plus 65535 separate lambda scopes are accepted.
    let mut source = "lambda: None\n".repeat(65_535);
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(super::Module::parse(parsed.syntax()).is_ok());
    source.push_str("lambda: None\n");
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(matches!(
        super::Module::parse(parsed.syntax()),
        Err(super::AnalysisError::HierarchyLimit)
    ));
}

#[test]
fn hierarchy_declarations_bound_each_scope_and_total_names() {
    use std::fmt::Write as _;
    let mut source = String::new();
    for function in ["first", "second"] {
        writeln!(source, "def {function}():").unwrap();
        for index in 0..65_535 {
            writeln!(source, "    local{index} = 0").unwrap();
        }
    }
    // Two function declarations plus two sets of 65535 locals.
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(super::Module::parse(parsed.syntax()).is_ok());
    source.push_str("    additional = 0\n");
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(matches!(
        super::Module::parse(parsed.syntax()),
        Err(super::AnalysisError::HierarchyLimit)
    ));
    let mut source = "def single():\n".to_owned();
    for index in 0..65_536 {
        writeln!(source, "    local{index} = 0").unwrap();
    }
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(super::Module::parse(parsed.syntax()).is_ok());
    source.push_str("    additional = 0\n");
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    assert!(matches!(
        super::Module::parse(parsed.syntax()),
        Err(super::AnalysisError::HierarchyLimit)
    ));
}
