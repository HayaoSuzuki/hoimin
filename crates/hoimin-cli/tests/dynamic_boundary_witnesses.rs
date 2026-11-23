use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn variants(source: &str, operator: MutationOperator) -> Vec<String> {
    let mut selection = MutationOperatorSelection::default();
    for id in selection.names() {
        selection.exclude(MutationOperator::from_name(&id).unwrap());
    }
    selection.include(operator);
    let request = AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &selection,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };
    analyze_source_cancellable(&request, source, || false)
        .unwrap()
        .candidates
        .into_iter()
        .map(|candidate| {
            let start = usize::try_from(candidate.span.start).unwrap();
            let length = usize::try_from(candidate.span.length).unwrap();
            let mut changed = source.to_owned();
            changed.replace_range(start..start + length, &candidate.replacement);
            changed
        })
        .collect()
}

async fn assert_python(source: &str, check: &str) {
    let python = std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    );
    let script = format!(
        "import sys\nassert sys.version_info[:2] in ((3, 14), (3, 15)), sys.version\n{source}\n{check}\n"
    );
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python)
            .args(["-B", "-c", &script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython execution deadline")
    .expect("set HOIMIN_OPERATOR_TEST_PYTHON to CPython 3.14 or 3.15");
    assert!(
        output.status.success(),
        "{script}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[rstest::rstest]
#[case::zip_omitted(
    MutationOperator::ZipStrictToggle,
    0,
    "try: out = list(zip([1, 2], [3], # trailing comma\n))\nexcept ValueError: out = 'mismatch'\n",
    "assert out == [(1, 3)]",
    "assert out == 'mismatch'"
)]
#[case::zip_dynamic_truthiness(
    MutationOperator::ZipStrictToggle,
    0,
    r"events = []
class Iterable:
    def __iter__(self): events.append('iter'); return iter((1, 2))
class Flag:
    def __bool__(self): events.append('bool'); return False
def first(): events.append('first'); return Iterable()
def second(): events.append('second'); return (3,)
def strictness(): events.append('strict'); return Flag()
try: out = list(zip(first(), second(), strict=strictness()))
except ValueError: out = 'mismatch'
",
    "assert out == [(1, 3)] and events == ['first', 'second', 'strict', 'bool', 'iter']",
    "assert out == 'mismatch' and events == ['first', 'second', 'strict', 'bool', 'iter']"
)]
#[case::zip_strict_keeps_temporary_alive_through_iterator_creation(
    MutationOperator::ZipStrictToggle,
    0,
    r"events = []
class Iterable:
    def __iter__(self): events.append('iter'); return iter((1,))
class Flag:
    def __bool__(self): events.append('bool'); return False
    def __del__(self): events.append('release')
try: out = list(zip(Iterable(), (2, 3), strict=Flag()))
except ValueError: out = 'mismatch'
",
    "assert out == [(1, 2)] and events == ['bool', 'iter', 'release']",
    "assert out == 'mismatch' and events == ['bool', 'iter', 'release']"
)]
#[case::zip_constructor_failure_releases_strict_before_handler(
    MutationOperator::ZipStrictToggle,
    0,
    r"events = []
class Iterable:
    def __iter__(self): events.append('iter'); raise ValueError
class Flag:
    def __bool__(self): events.append('bool'); return False
    def __del__(self): events.append('release')
try: zip(Iterable(), strict=Flag())
except ValueError as error: retained = error; events.append('handler')
",
    "assert isinstance(retained, ValueError) and events == ['bool', 'iter', 'release', 'handler']",
    "assert isinstance(retained, ValueError) and events == ['bool', 'iter', 'release', 'handler']"
)]
#[case::zip_truth_failure_releases_strict_before_retained_handler(
    MutationOperator::ZipStrictToggle,
    0,
    r"events = []
class Iterable:
    def __iter__(self): events.append('iter'); return iter(())
class Flag:
    def __bool__(self): events.append('bool'); return 1
    def __del__(self): events.append('release')
try: zip(Iterable(), strict=Flag())
except TypeError as error: retained = error; events.append('handler')
",
    "assert isinstance(retained, TypeError) and events == ['bool', 'release', 'handler']",
    "assert isinstance(retained, TypeError) and events == ['bool', 'release', 'handler']"
)]
#[case::range_converts_in_original_order(
    MutationOperator::RangeBoundNeighbor,
    0,
    r"events = []
class Index:
    def __init__(self, value): self.value = value
    def __index__(self): events.append(('index', self.value)); return self.value
    def __add__(self, _): raise RuntimeError('arithmetic must use the converted integer')
def arg(value): events.append(('arg', value)); return Index(value)
out = range(arg(1), arg(8), arg(2))
",
    "assert list(out) == [1, 3, 5, 7] and events == [('arg', 1), ('arg', 8), ('arg', 2), ('index', 1), ('index', 8), ('index', 2)]",
    "assert list(out) == [2, 4, 6] and events == [('arg', 1), ('arg', 8), ('arg', 2), ('index', 1), ('index', 8), ('index', 2)]"
)]
#[case::index_receiver_and_operand_effects(
    MutationOperator::StructureIndexNeighbor,
    0,
    r"events = []
class Index:
    def __index__(self): events.append('index'); return 0
    def __del__(self): events.append('release')
def target(): events.append('target'); return ('a', 'b')
def position(): events.append('position'); return Index()
out = target()[position()]
",
    "assert out == 'a' and events == ['target', 'position', 'index', 'release']",
    "assert out == 'b' and events == ['target', 'position', 'index', 'release']"
)]
#[case::comprehension_iterable(
    MutationOperator::StructureIndexNeighbor,
    0,
    "rows = [[3], [4]]\ni = 0\nout = [x for x in rows[i]]\n",
    "assert out == [3]",
    "assert out == [4]"
)]
#[case::lambda_in_generator_iterable(
    MutationOperator::StructureIndexNeighbor,
    0,
    "rows = [[3], [4]]\ni = 0\nout = list(x for x in (lambda: rows[i])())\n",
    "assert out == [3]",
    "assert out == [4]"
)]
#[case::slice_in_later_comprehension_iterable(
    MutationOperator::StructureSliceNeighbor,
    0,
    "class Key:\n    def __index__(self): return 0\nrows = [3, 4]\nout = [x for i in [Key()] for x in rows[i:]]\n",
    "assert out == [3, 4]",
    "assert out == [4]"
)]
#[case::index_keeps_a_runtime_noninteger_mapping_key(
    MutationOperator::StructureIndexNeighbor,
    0,
    "events=[]\nvalues={'name': 7}\ndef key(): events.append('key'); return 'name'\nout = values[key()]\n",
    "assert out == 7 and events == ['key']",
    "assert out == 7 and events == ['key']"
)]
#[case::index_keeps_integer_like_key_when_receiver_is_a_dictionary(
    MutationOperator::StructureIndexNeighbor,
    0,
    r"events = []
class Key:
    def __index__(self): events.append('index'); return 9
    def __hash__(self): events.append('hash'); return 2
key = Key()
values = {key: 'value'}
events.clear()
out = values[key]
",
    "assert out == 'value' and events == ['hash']",
    "assert out == 'value' and events == ['hash']"
)]
#[case::index_receiver_retries_failed_conversion_on_same_key(
    MutationOperator::StructureIndexNeighbor,
    0,
    r"from operator import index
events = []
class Key:
    def __index__(self):
        events.append('convert')
        if len(events) == 1: raise ValueError('retry')
        return 1
class Receiver:
    def __getitem__(self, key):
        try: return index(key)
        except ValueError:
            events.append('caught')
            return index(key)
out = Receiver()[Key()]
",
    "assert out == 1 and events == ['convert', 'caught', 'convert']",
    "assert out == 2 and events == ['convert', 'caught', 'convert']"
)]
#[case::index_failure_releases_temporary_before_retained_handler(
    MutationOperator::StructureIndexNeighbor,
    0,
    r"events = []
class Key:
    def __index__(self): events.append('index'); return 'invalid'
    def __del__(self): events.append('release')
try: out = [0, 1, 2][Key()]
except TypeError as error: retained = error; events.append('handler')
",
    "assert isinstance(retained, TypeError) and events == ['index', 'release', 'handler']",
    "assert isinstance(retained, TypeError) and events == ['index', 'release', 'handler']"
)]
#[case::slice_converts_after_arguments_in_step_start_stop_order(
    MutationOperator::StructureSliceNeighbor,
    0,
    r"events = []
class Index:
    def __init__(self, value): self.value = value
    def __index__(self): events.append(('index', self.value)); return self.value
def arg(value): events.append(('arg', value)); return Index(value)
out = [0, 1, 2, 3, 4, 5][arg(4):arg(0):arg(-2)]
",
    "assert out == [4, 2] and events == [('arg', 4), ('arg', 0), ('arg', -2), ('index', -2), ('index', 4), ('index', 0)]",
    "assert out == [5, 3, 1] and events == [('arg', 4), ('arg', 0), ('arg', -2), ('index', -2), ('index', 4), ('index', 0)]"
)]
#[case::slice_stride_negative_neighbor(
    MutationOperator::StructureSliceNeighbor,
    1,
    r"events = []
class Index:
    def __index__(self): events.append('index'); return -2
def stride(): events.append('stride'); return Index()
out = [0, 1, 2, 3, 4, 5][::stride()]
",
    "assert out == [5, 3, 1] and events == ['stride', 'index']",
    "assert out == [5, 2] and events == ['stride', 'index']"
)]
#[case::slice_keeps_runtime_none_as_an_omitted_bound(
    MutationOperator::StructureSliceNeighbor,
    0,
    "events=[]\ndef bound(): events.append('bound'); return None\nout = [0, 1, 2][:bound()]\n",
    "assert out == [0, 1, 2] and events == ['bound']",
    "assert out == [0, 1, 2] and events == ['bound']"
)]
#[case::comparison_chain_short_circuits(
    MutationOperator::CompareDirection,
    0,
    "events=[]\ndef arg(x): events.append(x); return x\nout = arg(1) <= arg(2) < arg(3)\n",
    "assert out is True and events == [1, 2, 3]",
    "assert out is False and events == [1, 2]"
)]
#[case::tuple_swaps_results_after_walrus_and_descriptors(
    MutationOperator::ReturnTupleSwap,
    0,
    r"events = []
class Descriptor:
    @property
    def value(self): events.append('get'); return object()
def last(arg): events.append(arg); return 3
def result(): return (captured := Descriptor().value), last(captured), captured
out = result()
",
    "assert out[0] is out[2] is events[1] and events[0] == 'get' and out[1] == 3",
    "assert out[1] is out[2] is events[1] and events[0] == 'get' and out[0] == 3"
)]
#[case::tuple_unbound_names_fail_in_source_order(
    MutationOperator::ReturnTupleSwap,
    0,
    "def result(): return first, second\ntry: result()\nexcept NameError as error: out = error.name\n",
    "assert out == 'first'",
    "assert out == 'first'"
)]
#[case::boolean_value_keeps_operand_identity_free_name_and_short_circuit(
    MutationOperator::ConditionClauseDelete,
    0,
    r"events = []
class Value:
    def __bool__(self): events.append('truth'); return True
sentinel = Value()
def outer():
    def skipped(): events.append('first'); return sentinel
    def inner(): return skipped() or sentinel or events.append('third')
    return inner
run = outer()
out = run()
",
    "assert out is sentinel and events == ['first', 'truth'] and run.__code__.co_freevars == ('skipped',)",
    "assert out is sentinel and events == ['truth'] and run.__code__.co_freevars == ('skipped',)"
)]
#[case::while_retains_removed_walrus_local(
    MutationOperator::ConditionClauseDelete,
    1,
    r"events = []
def check(): events.append('check'); return True
def read(): events.append('read'); return 1
def run():
    while check() and (value := read()): break
    return value
try: out = run()
except UnboundLocalError: out = 'unbound'
",
    "assert out == 1 and events == ['check', 'read']",
    "assert out == 'unbound' and events == ['check']"
)]
#[case::boolean_value_retains_generator_classification(
    MutationOperator::ConditionClauseDelete,
    1,
    "events=[]\ndef marker(): events.append('marker'); return True\ndef stream(): return marker() and (yield 8)\nout = next(stream(), 'done')\n",
    "assert out == 8 and events == ['marker']",
    "assert out == 'done' and events == ['marker']"
)]
#[case::boolean_value_retains_class_cell(
    MutationOperator::ConditionClauseDelete,
    0,
    "class Base:\n def read(self): return 1\nclass Child(Base):\n def read(self): return super().read() or 2\nout = Child().read()\n",
    "assert out == 1 and Child.read.__code__.co_freevars == ('__class__',)",
    "assert out == 2 and Child.read.__code__.co_freevars == ('__class__',)"
)]
#[tokio::test]
async fn python_observes_the_selected_boundary_change(
    #[case] operator: MutationOperator,
    #[case] choice: usize,
    #[case] source: &str,
    #[case] original: &str,
    #[case] changed: &str,
) {
    let mutants = variants(source, operator);
    let mutant = mutants.get(choice).expect("expected boundary mutation");

    assert_python(source, original).await;
    assert_python(mutant, changed).await;
}

#[rstest::rstest]
#[case::exact_parser_limit(194, 2)]
#[case::adapter_would_exceed_parser_limit(195, 0)]
#[tokio::test]
async fn nested_subscript_adapter_respects_cpython_delimiter_limit(
    #[case] depth: usize,
    #[case] expected: usize,
) {
    let source = format!(
        "def idx(): return 1\nout = {}[10, 20, 30][idx()]{}\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let mutants = variants(&source, MutationOperator::StructureIndexNeighbor);

    assert_eq!(mutants.len(), expected);
    assert_python(&source, "assert out == 20").await;
    for mutant in mutants {
        assert_python(&mutant, "assert out in (10, 30)").await;
    }
}
