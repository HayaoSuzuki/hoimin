use super::{analyze_selected, analyze_selected_with};
use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection};
use ruff_python_parser::parse_module;

const DELETE: &str = "override_method_delete";
const MOVE: &str = "super_call_move";
const PAIR: &str = "class Base:\n def run(self, value):\n  return value\nclass Child(Base):\n def run(self, value):\n  return value + 1\n";

fn apply(source: &str, candidate: &crate::AnalyzerCandidate) -> String {
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], candidate.original);
    let mut result = source.to_owned();
    result.replace_range(start..end, &candidate.replacement);
    assert!(parse_module(&result).is_ok(), "{result}");
    result
}

#[test]
fn inheritance_operators_are_default_on_and_individually_excludable() {
    let mut defaults = MutationOperatorSelection::default();
    for name in [DELETE, MOVE] {
        let operator = MutationOperator::from_name(name).expect("registered inheritance ID");
        assert!(defaults.contains(operator));
        defaults.exclude(operator);
        assert!(!defaults.contains(operator));
    }
}

#[test]
fn inheritance_override_deletion_removes_definition_and_preserves_neighbors() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = "class Base:\n def run(self, value): return value\nclass Child(Base):\n 'class doc'\n count = 1\n def run(self, value):\n  'method doc'\n  return value + 1 # retain trailing comment\n def other(self): return 7\n".replace('\n', newline);
        let output = analyze_selected(&[DELETE], &source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        let c = &output.candidates[0];
        assert!(c.original.starts_with("def run(self, value):"));
        assert_eq!(c.replacement, "pass");
        assert_eq!(c.symbol.as_deref(), Some("Child.run"));
        let mutant = apply(&source, c);
        assert!(mutant.contains("def run(self, value): return value"));
        assert!(mutant.contains("'class doc'"));
        assert!(!mutant.contains("'method doc'"));
        assert!(mutant.contains("pass # retain trailing comment"));
        assert!(mutant.contains("def other(self): return 7"));
        assert_eq!(
            output.candidates,
            analyze_selected(&[DELETE], &source).candidates
        );
    }
}

#[test]
fn inheritance_override_accepts_unicode_and_matching_parameter_kinds() {
    let source = "class 親:\n def 実行(self, value, /, *, mode): return value\nclass 子(親):\n def 実行(self, value, /, *, mode): return mode\n";
    let output = analyze_selected(&[DELETE], source);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("子.実行"));
    apply(source, &output.candidates[0]);
}

#[test]
fn inheritance_rejects_unproven_class_identity_and_method_lookup() {
    for extra in [
        "\nBase = object\n",
        "\nclass Base: pass\n",
        "\nalias = Base\n",
        "\nalias = Child\n",
        "\nexport(Base)\n",
        "\nexport(Child)\n",
        "\ncallback = Child.run\n",
        "\nBase.run = replacement\n",
        "\nsetattr(Base, 'run', replacement)\n",
        "\nexec('pass')\n",
        "\nfrom somewhere import *\n",
        "\nBase.__bases__ = (object,)\n",
        "\nnamespace = globals()\n",
        "\nnamespace = Base.__dict__\n",
        "\nobj.run = replacement\n",
        "\nobj = Child(); obj.run = replacement\n",
        "\nfrom builtins import setattr as change\nchange(obj, 'run', replacement)\n",
    ] {
        let source = format!("{PAIR}{extra}");
        assert!(
            analyze_selected(&[DELETE], &source).candidates.is_empty(),
            "{source}"
        );
    }
    for source in [
        "from elsewhere import Base\nclass Child(Base):\n def run(self): pass\n",
        "class Child(Base):\n def run(self): pass\nclass Base:\n def run(self): pass\n",
        "class Base(object):\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n",
        "class Base:\n def run(self): pass\nclass Child(Base, Other):\n def run(self): pass\n",
        "class Base:\n def run(self): pass\n@decorate\nclass Child(Base):\n def run(self): pass\n",
        "class Base:\n def run(self): pass\nclass Child(Base, metaclass=Meta):\n def run(self): pass\n",
        "class Base:\n def run(self): pass\nclass Child[T](Base):\n def run(self): pass\n",
        "class Base:\n def __getattribute__(self, key): pass\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n",
        "class Base:\n def __init_subclass__(self): pass\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n",
        "def outer():\n class Base:\n  def run(self): pass\n class Child(Base):\n  def run(self): pass\n",
        "class Base:\n def run(self): pass\nif enabled:\n class Child(Base):\n  def run(self): pass\n",
    ] {
        assert!(
            analyze_selected(&[DELETE], source).candidates.is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_rejects_method_shape_and_signature_hazards() {
    for (parent, child) in [
        ("def run(self): pass", "@staticmethod\n def run(self): pass"),
        ("def run(self): pass", "async def run(self): pass"),
        ("def run(self): pass", "def run(self): yield 1"),
        (
            "def run(self): pass",
            "def run(self):\n  def nested(): pass",
        ),
        ("def run(self, value): pass", "def run(self, other): pass"),
        (
            "def run(self, value): pass",
            "def run(self, *, value): pass",
        ),
        ("def run(self, value): pass", "def run(self, value=1): pass"),
        (
            "def run(self, value): pass",
            "def run(self, value: int): pass",
        ),
        ("def run(self): pass", "def run(self) -> int: pass"),
        ("def run(self): pass", "def run(self, *args): pass"),
        (
            "def run(self): pass",
            "def run(self): pass\n def run(self): pass",
        ),
        ("def run(self): pass", "def run(self): pass\n alias = run"),
        ("def __init__(self): pass", "def __init__(self): pass"),
        ("def __private(self): pass", "def __private(self): pass"),
        ("def run(self): pass", "def run(this): pass"),
    ] {
        let source = format!("class Base:\n {parent}\nclass Child(Base):\n {child}\n");
        assert!(
            analyze_selected(&[DELETE], &source).candidates.is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_super_move_retains_docstrings_comments_and_inline_separators() {
    for (body, expected) in [
        (
            "super().run(emit); emit('after')",
            "emit('after'); super().run(emit)",
        ),
        (
            "emit('before'); super().run(emit)",
            "super().run(emit); emit('before')",
        ),
        (
            "'doc'; super().run(emit); emit('after')",
            "'doc'; emit('after'); super().run(emit)",
        ),
        (
            "emit('before') # slot\n  (super().run(\n   emit\n  ))\n  emit('after')",
            "(super().run(\n   emit\n  )) # slot\n  emit('before')\n  emit('after')",
        ),
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = format!("class Base:\n def run(self, emit): emit('parent')\nclass Child(Base):\n def run(self, emit):\n  {body}\n").replace('\n', newline);
            let want = source.replace(
                &body.replace('\n', newline),
                &expected.replace('\n', newline),
            );
            let output = analyze_selected(&[MOVE], &source);
            assert_eq!(output.candidates.len(), 1, "{source}");
            assert_eq!(apply(&source, &output.candidates[0]), want);
            assert_eq!(output.candidates[0].symbol.as_deref(), Some("Child.run"));
        }
    }
}

#[test]
fn inheritance_super_move_requires_builtin_single_standalone_same_method_call() {
    let parent = "class Base:\n def run(self, emit): emit('parent')\n";
    for body in [
        "super().run(emit)",
        "return super().run(emit)",
        "result = super().run(emit); emit('after')",
        "emit(super().run(emit)); emit('after')",
        "super(Child, self).run(emit); emit('after')",
        "super().other(emit); emit('after')",
        "super().run(emit); super().run(emit)",
        "super().run(emit); emit(super)",
        "super().run(emit); super().other(emit)",
        "super().run(*emit); emit('after')",
        "super().run(**emit); emit('after')",
        "super().run(emit); (value := emit('after'))",
        "super().run(emit); return",
        "super().run(emit); 'new docstring'",
        "if ready: super().run(emit)\n  emit('after')",
    ] {
        let source = format!("{parent}class Child(Base):\n def run(self, emit):\n  {body}\n");
        assert!(
            analyze_selected(&[MOVE], &source).candidates.is_empty(),
            "{source}"
        );
    }
    for (header, body, tail) in [
        (
            "self, emit",
            "emit('before'); super().run(emit)",
            "\nsuper = fake\n",
        ),
        ("self, emit, super", "emit('before'); super().run(emit)", ""),
        (
            "self, emit",
            "super = fake; super().run(emit); emit('after')",
            "",
        ),
    ] {
        let source = format!(
            "class Base:\n def run({header}): pass\nclass Child(Base):\n def run({header}):\n  {body}\n{tail}"
        );
        assert!(
            analyze_selected(&[MOVE], &source).candidates.is_empty(),
            "{source}"
        );
    }
    let source = format!(
        "{parent}class Child(Base):\n super = 1\n def run(self, emit): emit('before'); super().run(emit)\n"
    );
    assert_eq!(analyze_selected(&[MOVE], &source).candidates.len(), 1);
}

#[test]
fn inheritance_selection_uses_the_edited_method_span() {
    let full = analyze_selected(&[DELETE], PAIR);
    let symbols = ["pkg.sample:Child.run".to_owned()];
    let selected = analyze_selected_with(
        &[DELETE],
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 5, end: 5 }],
        &symbols,
        10,
        PAIR,
    );
    assert_eq!(selected.candidates, full.candidates);
    let excluded = analyze_selected_with(
        &[DELETE],
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &[],
        10,
        PAIR,
    );
    assert!(
        excluded.candidates.is_empty(),
        "parent lines must not select child deletion"
    );
}

#[test]
fn inheritance_rejects_bound_method_escapes_and_reflection_aliases() {
    for tail in [
        "callback = obj.run",
        "import builtins\nbuiltins.super = fake",
        "env = __builtins__",
        "lookup = getattr\nlookup(obj, 'run')",
        "from builtins import getattr as lookup\nlookup(obj, 'run')",
        "change = obj.__setattr__\nchange('run', replacement)",
        "lookup = obj.__getattribute__\nlookup('run')",
    ] {
        let source = format!("{PAIR}\n{tail}\n");
        assert!(
            analyze_selected(&[DELETE], &source).candidates.is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_shared_index_selection_caps_cancellation_and_payload() {
    use super::super::{
        AnalysisError, AnalyzeRequest, analyze_source, analyze_source_cancellable, inheritance,
        payload::PayloadBudget,
    };
    use hoimin_core::MutationProfile;
    let source = "class Base:\n def run(self, emit): emit('base')\nclass Child(Base):\n def run(self, emit):\n  super().run(emit); emit('after')\nclass Other(Base):\n def run(self, emit):\n  emit('before'); super().run(emit)\n";
    let operators = serde_json::from_value(serde_json::json!([DELETE, MOVE])).unwrap();
    let budget = PayloadBudget::default();
    let mut request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    inheritance::reset_work();
    let full = analyze_source(&request, source);
    assert_eq!(full.candidates.len(), 4);
    assert_eq!(inheritance::work(), [1, 3, 2]);
    drop(full);
    assert_eq!(budget.snapshot().used, 0);
    request.lines = &[LineRange { start: 1, end: 1 }];
    inheritance::reset_work();
    assert!(
        analyze_source(&request, source).candidates.is_empty(),
        "excluded selectors must avoid replacement builders"
    );
    assert_eq!(inheritance::work(), [1, 3, 0]);
    request.lines = &[];
    request.max_candidates = 1;
    let capped = analyze_source(&request, source);
    assert_eq!(capped.candidates.len(), 1);
    assert_eq!(capped.candidates[0].operator.as_str(), DELETE);
    assert!(capped.truncated);
    drop(capped);
    inheritance::reset_work();
    assert!(matches!(
        analyze_source_cancellable(&request, source, || inheritance::work()[2] > 0),
        Err(AnalysisError::Cancelled)
    ));
    assert_eq!(inheritance::work()[2], 1);
    assert_eq!(budget.snapshot().used, 0);
    let tiny = PayloadBudget::new(0);
    request.payload = tiny.clone();
    assert!(matches!(
        analyze_source_cancellable(&request, source, || false),
        Err(AnalysisError::PayloadLimit { limit: 0 })
    ));
    assert_eq!(tiny.snapshot().used, 0);
    let operators = serde_json::from_value(serde_json::json!(["base_order_shuffle"])).unwrap();
    request.operators = &operators;
    request.payload = budget;
    inheritance::reset_work();
    analyze_source(&request, source);
    assert_eq!(inheritance::work(), [0, 0, 0]);
}

#[test]
fn inheritance_method_checks_are_linear_and_index_cancellation_is_observed() {
    use super::super::{
        AnalysisError, AnalyzeRequest, analyze_source_cancellable, inheritance,
        payload::PayloadBudget,
    };
    use hoimin_core::MutationProfile;
    use std::fmt::Write as _;
    let mut source = "class Base:\n".to_owned();
    for index in 0..200 {
        writeln!(source, " def m{index}(self): return 1").unwrap();
    }
    source.push_str("class Child(Base):\n");
    for index in 0..200 {
        writeln!(source, " def m{index}(self): return 2").unwrap();
    }
    inheritance::reset_work();
    assert_eq!(analyze_selected(&[DELETE], &source).candidates.len(), 200);
    assert_eq!(inheritance::work(), [1, 400, 0]);
    let operators = serde_json::from_value(serde_json::json!([DELETE])).unwrap();
    let budget = PayloadBudget::default();
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    inheritance::reset_work();
    assert!(matches!(
        analyze_source_cancellable(&request, &source, || inheritance::work()[1] >= 100),
        Err(AnalysisError::Cancelled)
    ));
    assert_eq!(inheritance::work()[1], 100);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn inheritance_rejects_assigned_lookup_and_subclass_hooks() {
    for hook in [
        "__getattribute__",
        "__getattr__",
        "__setattr__",
        "__delattr__",
        "__init_subclass__",
    ] {
        let source = format!(
            "class Base:\n {hook} = hook\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n"
        );
        assert!(
            analyze_selected(&[DELETE], &source).candidates.is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_rejects_unproven_descriptor_bindings_in_class_suites() {
    for binding in ["descriptor", "external.descriptor", "make_descriptor()"] {
        let source = format!(
            "from external import descriptor\nclass Base:\n def run(self): pass\nclass Child(Base):\n trap = {binding}\n def run(self): pass\n"
        );
        assert!(
            analyze_selected(&[DELETE, MOVE], &source)
                .candidates
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_rejects_method_header_aliases_and_decorated_descriptors() {
    for extra in [
        "def other(self, callback=run): return callback(self)",
        "def other(self, callback: run): return callback(self)",
        "def other(self) -> run: return self",
        "def other[T: run](self): pass",
        "def other[T = run](self): pass",
        "@descriptor\n def other(self): pass",
    ] {
        let source = format!(
            "from external import descriptor\nclass Base:\n def run(self): return 1\nclass Child(Base):\n def run(self): return 2\n {extra}\n"
        );
        assert!(
            analyze_selected(&[DELETE, MOVE], &source)
                .candidates
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn inheritance_rejects_class_escapes_through_unsupported_subclass_headers() {
    for tail in [
        "class Other(Base, metaclass=ExternalMeta): pass",
        "class Other(Base, Mixin): pass",
        "@decorate\nclass Other(Base): pass",
        "class Other[T](Base): pass",
    ] {
        let source = format!("{PAIR}\n{tail}\n");
        assert!(
            analyze_selected(&[DELETE], &source).candidates.is_empty(),
            "{source}"
        );
    }
}
