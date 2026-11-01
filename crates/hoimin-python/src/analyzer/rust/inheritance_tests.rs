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

#[rstest::rstest]
#[case::delete_1(0)]
#[case::move_2(1)]
fn inheritance_operators_are_default_on_and_individually_excludable(#[case] scenario_index: usize) {
    let mut defaults = MutationOperatorSelection::default();

    let name = [DELETE, MOVE].into_iter().nth(scenario_index).unwrap();

    let operator = MutationOperator::from_name(name).expect("registered inheritance ID");

    assert!(defaults.contains(operator));
    defaults.exclude(operator);
    assert!(!defaults.contains(operator));
}

#[rstest::rstest]
#[case::value("\n")]
#[case::value_2("\r\n")]
#[case::value_3("\r")]
fn inheritance_override_deletion_removes_definition_and_preserves_neighbors(#[case] newline: &str) {
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

#[test]
fn inheritance_override_accepts_unicode_and_matching_parameter_kinds() {
    let source = "class 親:\n def 実行(self, value, /, *, mode): return value\nclass 子(親):\n def 実行(self, value, /, *, mode): return mode\n";

    let output = analyze_selected(&[DELETE], source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("子.実行"));
    apply(source, &output.candidates[0]);
}

#[rstest::rstest]
#[case::base_object("\nBase = object\n")]
#[case::class_base_pass("\nclass Base: pass\n")]
#[case::alias_base("\nalias = Base\n")]
#[case::alias_child("\nalias = Child\n")]
#[case::export_base("\nexport(Base)\n")]
#[case::export_child("\nexport(Child)\n")]
#[case::callback_child_run("\ncallback = Child.run\n")]
#[case::base_run_replacement("\nBase.run = replacement\n")]
#[case::setattr_base_run_replacement("\nsetattr(Base, 'run', replacement)\n")]
#[case::exec_pass("\nexec('pass')\n")]
#[case::from_somewhere_import("\nfrom somewhere import *\n")]
#[case::base_bases_object("\nBase.__bases__ = (object,)\n")]
#[case::namespace_globals("\nnamespace = globals()\n")]
#[case::namespace_base_dict("\nnamespace = Base.__dict__\n")]
#[case::obj_run_replacement("\nobj.run = replacement\n")]
#[case::obj_child_obj_run_replacement("\nobj = Child(); obj.run = replacement\n")]
#[case::from_builtins_import_setattr_as_change_change_obj_run_replacement(
    "\nfrom builtins import setattr as change\nchange(obj, 'run', replacement)\n"
)]
fn inheritance_rejects_unproven_class_identity_and_method_lookup_table_1(#[case] extra: &str) {
    let source = format!("{PAIR}{extra}");

    assert!(
        analyze_selected(&[DELETE], &source).candidates.is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_elsewhere_import_base_class_child_base_def_run_self_pass(
    "from elsewhere import Base\nclass Child(Base):\n def run(self): pass\n"
)]
#[case::class_child_base_def_run_self_pass_class_base_def_run_self_pass(
    "class Child(Base):\n def run(self): pass\nclass Base:\n def run(self): pass\n"
)]
#[case::class_base_object_def_run_self_pass_class_child_base_def_run_self(
    "class Base(object):\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n"
)]
#[case::class_base_def_run_self_pass_class_child_base_other_def_run_self(
    "class Base:\n def run(self): pass\nclass Child(Base, Other):\n def run(self): pass\n"
)]
#[case::class_base_def_run_self_pass_decorate_class_child_base_def_run_se(
    "class Base:\n def run(self): pass\n@decorate\nclass Child(Base):\n def run(self): pass\n"
)]
#[case::class_base_def_run_self_pass_class_child_base_metaclass_meta_def(
    "class Base:\n def run(self): pass\nclass Child(Base, metaclass=Meta):\n def run(self): pass\n"
)]
#[case::class_base_def_run_self_pass_class_child_t_base_def_run_self_pass(
    "class Base:\n def run(self): pass\nclass Child[T](Base):\n def run(self): pass\n"
)]
#[case::class_base_def_getattribute_self_key_pass_def_run_self_pass_class(
    "class Base:\n def __getattribute__(self, key): pass\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n"
)]
#[case::class_base_def_init_subclass_self_pass_def_run_self_pass_class_ch(
    "class Base:\n def __init_subclass__(self): pass\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n"
)]
#[case::def_outer_class_base_def_run_self_pass_class_child_base_def_run_s(
    "def outer():\n class Base:\n  def run(self): pass\n class Child(Base):\n  def run(self): pass\n"
)]
#[case::class_base_def_run_self_pass_if_enabled_class_child_base_def_run(
    "class Base:\n def run(self): pass\nif enabled:\n class Child(Base):\n  def run(self): pass\n"
)]
fn inheritance_rejects_unproven_class_identity_and_method_lookup_table_2(#[case] source: &str) {
    assert!(
        analyze_selected(&[DELETE], source).candidates.is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::def_run_self_pass("def run(self): pass", "@staticmethod\n def run(self): pass")]
#[case::def_run_self_pass_2("def run(self): pass", "async def run(self): pass")]
#[case::def_run_self_pass_3("def run(self): pass", "def run(self): yield 1")]
#[case::def_run_self_pass_4("def run(self): pass", "def run(self):\n  def nested(): pass")]
#[case::def_run_self_value_pass("def run(self, value): pass", "def run(self, other): pass")]
#[case::def_run_self_value_pass_6("def run(self, value): pass", "def run(self, *, value): pass")]
#[case::def_run_self_value_pass_7("def run(self, value): pass", "def run(self, value=1): pass")]
#[case::def_run_self_value_pass_8("def run(self, value): pass", "def run(self, value: int): pass")]
#[case::def_run_self_pass_9("def run(self): pass", "def run(self) -> int: pass")]
#[case::def_run_self_pass_10("def run(self): pass", "def run(self, *args): pass")]
#[case::def_run_self_pass_11("def run(self): pass", "def run(self): pass\n def run(self): pass")]
#[case::def_run_self_pass_12("def run(self): pass", "def run(self): pass\n alias = run")]
#[case::def_init_self_pass("def __init__(self): pass", "def __init__(self): pass")]
#[case::def_private_self_pass("def __private(self): pass", "def __private(self): pass")]
#[case::def_run_self_pass_15("def run(self): pass", "def run(this): pass")]
fn inheritance_rejects_method_shape_and_signature_hazards(
    #[case] parent: &str,
    #[case] child: &str,
) {
    let source = format!("class Base:\n {parent}\nclass Child(Base):\n {child}\n");

    assert!(
        analyze_selected(&[DELETE], &source).candidates.is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::super_run_emit_emit_after(
    "super().run(emit); emit('after')",
    "emit('after'); super().run(emit)"
)]
#[case::emit_before_super_run_emit(
    "emit('before'); super().run(emit)",
    "super().run(emit); emit('before')"
)]
#[case::doc_super_run_emit_emit_after(
    "'doc'; super().run(emit); emit('after')",
    "'doc'; emit('after'); super().run(emit)"
)]
#[case::emit_before_slot_super_run_emit_emit_after(
    "emit('before') # slot\n  (super().run(\n   emit\n  ))\n  emit('after')",
    "(super().run(\n   emit\n  )) # slot\n  emit('before')\n  emit('after')"
)]
fn inheritance_super_move_retains_docstrings_comments_and_inline_separators(
    #[case] body: &str,
    #[case] expected: &str,
    #[values(0, 1, 2)] scenario_index: usize,
) {
    let newline = ["\n", "\r\n", "\r"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

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

#[rstest::rstest]
#[case::super_run_emit("super().run(emit)")]
#[case::return_super_run_emit("return super().run(emit)")]
#[case::result_super_run_emit_emit_after("result = super().run(emit); emit('after')")]
#[case::emit_super_run_emit_emit_after("emit(super().run(emit)); emit('after')")]
#[case::super_child_self_run_emit_emit_after("super(Child, self).run(emit); emit('after')")]
#[case::super_other_emit_emit_after("super().other(emit); emit('after')")]
#[case::super_run_emit_super_run_emit("super().run(emit); super().run(emit)")]
#[case::super_run_emit_emit_super("super().run(emit); emit(super)")]
#[case::super_run_emit_super_other_emit("super().run(emit); super().other(emit)")]
#[case::super_run_emit_emit_after("super().run(*emit); emit('after')")]
#[case::super_run_emit_emit_after_11("super().run(**emit); emit('after')")]
#[case::super_run_emit_value_emit_after("super().run(emit); (value := emit('after'))")]
#[case::super_run_emit_return("super().run(emit); return")]
#[case::super_run_emit_new_docstring("super().run(emit); 'new docstring'")]
#[case::if_ready_super_run_emit_emit_after("if ready: super().run(emit)\n  emit('after')")]
fn inheritance_super_move_requires_builtin_single_standalone_same_method_call_scenario_1(
    #[case] body: &str,
) {
    let parent = "class Base:\n def run(self, emit): emit('parent')\n";

    let source = format!("{parent}class Child(Base):\n def run(self, emit):\n  {body}\n");

    assert!(
        analyze_selected(&[MOVE], &source).candidates.is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::self_emit("self, emit", "emit('before'); super().run(emit)", "\nsuper = fake\n")]
#[case::self_emit_super("self, emit, super", "emit('before'); super().run(emit)", "")]
#[case::self_emit_3("self, emit", "super = fake; super().run(emit); emit('after')", "")]
fn inheritance_super_move_requires_builtin_single_standalone_same_method_call_scenario_2(
    #[case] header: &str,
    #[case] body: &str,
    #[case] tail: &str,
) {
    let source = format!(
        "class Base:\n def run({header}): pass\nclass Child(Base):\n def run({header}):\n  {body}\n{tail}"
    );
    assert!(
        analyze_selected(&[MOVE], &source).candidates.is_empty(),
        "{source}"
    );
}

#[test]
fn inheritance_super_move_requires_builtin_single_standalone_same_method_call() {
    let parent = "class Base:\n def run(self, emit): emit('parent')\n";

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

#[rstest::rstest]
#[case::callback_obj_run("callback = obj.run")]
#[case::import_builtins_builtins_super_fake("import builtins\nbuiltins.super = fake")]
#[case::env_builtins("env = __builtins__")]
#[case::lookup_getattr_lookup_obj_run("lookup = getattr\nlookup(obj, 'run')")]
#[case::from_builtins_import_getattr_as_lookup_lookup_obj_run(
    "from builtins import getattr as lookup\nlookup(obj, 'run')"
)]
#[case::change_obj_setattr_change_run_replacement(
    "change = obj.__setattr__\nchange('run', replacement)"
)]
#[case::lookup_obj_getattribute_lookup_run("lookup = obj.__getattribute__\nlookup('run')")]
fn inheritance_rejects_bound_method_escapes_and_reflection_aliases(#[case] tail: &str) {
    let source = format!("{PAIR}\n{tail}\n");

    assert!(
        analyze_selected(&[DELETE], &source).candidates.is_empty(),
        "{source}"
    );
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

#[rstest::rstest]
#[case::getattribute("__getattribute__")]
#[case::getattr("__getattr__")]
#[case::setattr("__setattr__")]
#[case::delattr("__delattr__")]
#[case::init_subclass("__init_subclass__")]
fn inheritance_rejects_assigned_lookup_and_subclass_hooks(#[case] hook: &str) {
    let source = format!(
        "class Base:\n {hook} = hook\n def run(self): pass\nclass Child(Base):\n def run(self): pass\n"
    );

    assert!(
        analyze_selected(&[DELETE], &source).candidates.is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::descriptor("descriptor")]
#[case::external_descriptor("external.descriptor")]
#[case::make_descriptor("make_descriptor()")]
fn inheritance_rejects_unproven_descriptor_bindings_in_class_suites(#[case] binding: &str) {
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

#[rstest::rstest]
#[case::def_other_self_callback_run_return_callback_self(
    "def other(self, callback=run): return callback(self)"
)]
#[case::def_other_self_callback_run_return_callback_self_2(
    "def other(self, callback: run): return callback(self)"
)]
#[case::def_other_self_run_return_self("def other(self) -> run: return self")]
#[case::def_other_t_run_self_pass("def other[T: run](self): pass")]
#[case::def_other_t_run_self_pass_5("def other[T = run](self): pass")]
#[case::descriptor_def_other_self_pass("@descriptor\n def other(self): pass")]
fn inheritance_rejects_method_header_aliases_and_decorated_descriptors(#[case] extra: &str) {
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

#[rstest::rstest]
#[case::class_other_base_metaclass_externalmeta_pass(
    "class Other(Base, metaclass=ExternalMeta): pass"
)]
#[case::class_other_base_mixin_pass("class Other(Base, Mixin): pass")]
#[case::decorate_class_other_base_pass("@decorate\nclass Other(Base): pass")]
#[case::class_other_t_base_pass("class Other[T](Base): pass")]
fn inheritance_rejects_class_escapes_through_unsupported_subclass_headers(#[case] tail: &str) {
    let source = format!("{PAIR}\n{tail}\n");

    assert!(
        analyze_selected(&[DELETE], &source).candidates.is_empty(),
        "{source}"
    );
}
