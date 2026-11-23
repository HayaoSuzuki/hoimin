use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap();
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn assert_python(source: &str, assertion: &str) {
    let script = format!("{source}\n{assertion}");
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args(["-B", "-c", &script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython execution deadline")
    .expect("set HOIMIN_OPERATOR_TEST_PYTHON to a CPython executable");
    assert!(
        output.status.success(),
        "{script}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[rstest::rstest]
#[case::branch(
    MutationOperator::ConditionalBranchSwap,
    "events = []\ndef see(value):\n    events.append(value)\n    return value\nout = see('left') if see(True) else see('right')\n",
    "assert out == 'left' and events == [True, 'left']",
    "assert out == 'right' and events == [True, 'right']"
)]
#[case::await_operand(
    MutationOperator::AwaitRemove,
    "events = []\nclass Awaitable:\n    def __await__(self):\n        events.append('await')\n        yield from ()\n        return 7\ndef operand():\n    events.append('operand')\n    return Awaitable()\nasync def run(): return await operand()\ntry: run().send(None)\nexcept StopIteration as stop: out = stop.value\n",
    "assert out == 7 and events == ['operand', 'await']",
    "assert isinstance(out, Awaitable) and events == ['operand']"
)]
#[case::unpack_order(
    MutationOperator::DictUnpackOrderSwap,
    "events = []\nclass Mapping:\n    def __init__(self, value):\n        self.value = value\n    def keys(self):\n        events.append(('keys', self.value))\n        return ['same']\n    def __getitem__(self, key):\n        events.append(('getitem', self.value))\n        return self.value\ndef operand(value):\n    events.append(('operand', value))\n    return Mapping(value)\nout = {**operand(1), **operand(2)}\n",
    "assert list(out.items()) == [('same', 2)] and events == [('operand', 1), ('keys', 1), ('getitem', 1), ('operand', 2), ('keys', 2), ('getitem', 2)]",
    "assert list(out.items()) == [('same', 1)] and events == [('operand', 1), ('keys', 1), ('getitem', 1), ('operand', 2), ('keys', 2), ('getitem', 2)]"
)]
#[case::unpack_invalid_left(
    MutationOperator::DictUnpackOrderSwap,
    "events = []\ndef operand(value):\n    events.append(value)\n    return value\ntry:\n    out = {**operand(None), **operand({})}\nexcept TypeError:\n    rejected = True\n",
    "assert rejected and events == [None]",
    "assert rejected and events == [None]"
)]
#[case::unpack_prefix_custom_equality(
    MutationOperator::DictUnpackOrderSwap,
    "events = []\nclass Key:\n    def __hash__(self): return 1\n    def __eq__(self, other): raise RuntimeError('collision')\ndef operand(label):\n    events.append(label)\n    return {Key(): label} if label == 'left' else {}\ntry:\n    out = {Key(): 0, **operand('left'), **operand('right')}\nexcept RuntimeError:\n    rejected = True\n",
    "assert rejected and events == ['left']",
    "assert rejected and events == ['left', 'right']"
)]
#[case::unpack_named_expression(
    MutationOperator::DictUnpackDelete,
    "slot = 17\nevents = []\ndef operand():\n    events.append('unpacked')\n    return {'key': 3}\ndef run():\n    out = {**(slot := operand()), 'tail': 9}\n    try:\n        return out, slot\n    except UnboundLocalError:\n        return out, None\nresult = run()\n",
    "assert result == ({'key': 3, 'tail': 9}, {'key': 3}) and events == ['unpacked']",
    "assert result == ({'tail': 9}, None) and events == []"
)]
#[case::del_binding(
    MutationOperator::DelRemove,
    "name = 'global'\ndef run():\n    try: del name; return 'continued'\n    except UnboundLocalError: return 'local'\n",
    "assert run() == 'local'",
    "assert run() == 'continued' and 'name' in run.__code__.co_varnames"
)]
#[case::del_item(
    MutationOperator::DelRemove,
    "events = []\ndef key():\n    events.append('key')\n    return 'item'\nitems = {'item': 1}\nif items: del items[key()]; events.append('tail')\n",
    "assert items == {} and events == ['key', 'tail']",
    "assert items == {'item': 1} and events == ['tail']"
)]
#[tokio::test]
async fn generated_expression_mutation_has_the_expected_cpython_behavior(
    #[case] operator: MutationOperator,
    #[case] source: &str,
    #[case] baseline: &str,
    #[case] changed: &str,
) {
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        path: Utf8Path::new("expression_witness.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1_000,
        payload: PayloadBudget::default(),
    };
    assert_python(source, baseline).await;

    let analyzed = analyze_source_cancellable(&request, source, || false).unwrap();
    let mut candidates = analyzed
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == operator);
    let mutation = candidates.next().expect("matching analyzer candidate");
    let start = usize::try_from(mutation.span.start).unwrap();
    let length = usize::try_from(mutation.span.length).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..start + length, &mutation.replacement);

    assert!(candidates.next().is_none());
    assert_python(&mutant, changed).await;
}

#[rstest::rstest]
#[case::force_true(
    MutationOperator::ConditionConstant,
    0,
    "def outer():\n    captured = False\n    return lambda: 'yes' if captured else 'no'\nrun = outer()\n",
    "assert run() == 'no' and run.__code__.co_freevars == ('captured',)",
    "assert run() == 'yes' and run.__code__.co_freevars == ('captured',)"
)]
#[case::force_false(
    MutationOperator::ConditionConstant,
    1,
    "def outer():\n    captured = True\n    return lambda: 'yes' if captured else 'no'\nrun = outer()\n",
    "assert run() == 'yes' and run.__code__.co_freevars == ('captured',)",
    "assert run() == 'no' and run.__code__.co_freevars == ('captured',)"
)]
#[case::delete_first_clause_once(
    MutationOperator::ConditionClauseDelete,
    0,
    "events = []\nclass Stateful:\n    def __bool__(self):\n        events.append('truth')\n        return events.count('truth') == 1\ndef outer():\n    captured = False\n    return lambda: 'yes' if captured and Stateful() else 'no'\nrun = outer()\n",
    "assert run() == 'no' and events == [] and run.__code__.co_freevars == ('captured',)",
    "assert run() == 'yes' and events == ['truth'] and run.__code__.co_freevars == ('captured',)"
)]
#[tokio::test]
async fn conditional_test_mutations_keep_closure_bindings(
    #[case] operator: MutationOperator,
    #[case] index: usize,
    #[case] source: &str,
    #[case] baseline: &str,
    #[case] changed: &str,
) {
    let mut operators = MutationOperatorSelection::default();
    operators.include(operator);
    let request = AnalyzeRequest {
        path: Utf8Path::new("conditional_witness.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };
    assert_python(source, baseline).await;

    let analyzed = analyze_source_cancellable(&request, source, || false).unwrap();
    let candidates: Vec<_> = analyzed
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == operator)
        .collect();
    let mutation = candidates.get(index).expect("matching analyzer candidate");
    let start = usize::try_from(mutation.span.start).unwrap();
    let length = usize::try_from(mutation.span.length).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..start + length, &mutation.replacement);

    assert_eq!(candidates.len(), 2);
    assert_python(&mutant, changed).await;
}

#[rstest::rstest]
#[case::swap(MutationOperator::DictUnpackOrderSwap, "**left,\n    **right")]
#[case::delete(MutationOperator::DictUnpackDelete, "left")]
fn unpack_edits_use_the_selected_item_line(
    #[case] operator: MutationOperator,
    #[case] original: &str,
) {
    let source = "out = {\n    **left,\n    **right,\n    **tail,\n}\n";
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        path: Utf8Path::new("expression_selection.py"),
        lines: &[LineRange { start: 2, end: 2 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };

    let analyzed = analyze_source_cancellable(&request, source, || false).unwrap();
    let mut candidates = analyzed
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == operator);
    let candidate = candidates.next().expect("selected unpack");
    let start = usize::try_from(candidate.span.start).unwrap();
    let length = usize::try_from(candidate.span.length).unwrap();

    assert_eq!(&source[start..start + length], original);
    assert!(candidates.next().is_none());
}
