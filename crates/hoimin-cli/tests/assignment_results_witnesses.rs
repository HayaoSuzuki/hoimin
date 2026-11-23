use camino::Utf8Path;
use hoimin_core::{
    LineRange, MutationOperator, MutationOperatorSelection, MutationProfile, MutationSelector,
};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn mutants(
    source: &str,
    operator: MutationOperator,
    lines: &[LineRange],
    maximum: usize,
) -> Vec<String> {
    let mut operators = MutationOperatorSelection::default();
    for selector in MutationOperatorSelection::selectors() {
        if let MutationSelector::Operator(other) = selector
            && other != operator
        {
            operators.exclude(other);
        }
    }
    let request = AnalyzeRequest {
        path: Utf8Path::new("assignment_results_witness.py"),
        lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: maximum,
        payload: PayloadBudget::default(),
    };
    analyze_source_cancellable(&request, source, || false)
        .unwrap()
        .candidates
        .into_iter()
        .map(|candidate| {
            assert_eq!(candidate.operator, operator);
            let start = usize::try_from(candidate.span.start).unwrap();
            let length = usize::try_from(candidate.span.length).unwrap();
            let mut changed = source.to_owned();
            changed.replace_range(start..start + length, &candidate.replacement);
            changed
        })
        .collect()
}

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(if cfg!(windows) {
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
#[case::alias(
    MutationOperator::AugmentedToBinary,
    "items = alias = [1]\nif items: items += [2]; out = items\n",
    2,
    "assert out is alias and alias == [1, 2]",
    "assert out is not alias and out == [1, 2] and alias == [1]"
)]
#[case::descriptor_order_and_release(
    MutationOperator::AugmentedToBinary,
    r"events = []
class New:
    def __del__(self): events.append('release-new')
class Old:
    def __iadd__(self, rhs): events.append('iadd'); return New()
    def __add__(self, rhs): events.append('add'); return New()
    def __del__(self): events.append('release-old')
class Right:
    def __del__(self): events.append('release-right')
class Target:
    @property
    def value(self): events.append('get'); return Old()
    @value.setter
    def value(self, value): events.append('set')
    def __del__(self): events.append('release-receiver')
def receiver(): events.append('receiver'); return Target()
def rhs(): events.append('rhs'); return Right()
receiver().value += rhs()
",
    18,
    "assert events[:4] == ['receiver', 'get', 'rhs', 'iadd'] and set(events[4:6]) == {'release-old', 'release-right'} and events[6:] == ['set', 'release-receiver', 'release-new']",
    "assert events[:4] == ['receiver', 'get', 'rhs', 'add'] and set(events[4:6]) == {'release-old', 'release-right'} and events[6:] == ['set', 'release-receiver', 'release-new']"
)]
#[case::subscript_slice_identity(
    MutationOperator::AugmentedToBinary,
    r"events = []
class Target:
    def __getitem__(self, key):
        events.append(('get', key))
        return [1]
    def __setitem__(self, key, value):
        events.append(('set', key, value))
def mark(label, value):
    events.append(label)
    return value
slice = 'shadowed'
mark('receiver', Target())[mark('start', 1):mark('stop', 4), *mark('parts', [2, 3])] += mark('right', [2])
",
    12,
    "assert events[:4] == ['receiver', 'start', 'stop', 'parts'] and events[4][0] == 'get' and events[5] == 'right' and events[6] == ('set', events[4][1], [1, 2]) and events[4][1] is events[6][1] and events[4][1][0].start == 1 and events[4][1][1:] == (2, 3)",
    "assert events[:4] == ['receiver', 'start', 'stop', 'parts'] and events[4][0] == 'get' and events[5] == 'right' and events[6] == ('set', events[4][1], [1, 2]) and events[4][1] is events[6][1] and events[4][1][0].start == 1 and events[4][1][1:] == (2, 3)"
)]
#[case::augmented_single_slice_keeps_both_alias_and_assignment_behavior(
    MutationOperator::AugmentedToBinary,
    r"events = []
class Target:
    def __getitem__(self, key): events.append(key); return old
    def __setitem__(self, key, value): events.extend((key, value))
old = [1]
Target()[1:4:2] += [2]
",
    6,
    "assert events[0] is events[1] and events[0].step == 2 and events[2] is old and old == [1, 2]",
    "assert events[0] is events[1] and events[0].step == 2 and events[2] == [1, 2] and old == [1] and events[2] is not old"
)]
#[case::augmented_subscript_native_index_protocol(
    MutationOperator::AugmentedToBinary,
    r"events = []
class Index:
    def __index__(self): events.append('index'); return 0
values = [[1]]
old = values[0]
def key(): events.append('key'); return Index()
def right(): events.append('right'); return [2]
values[key()] += right()
",
    8,
    "assert events == ['key', 'index', 'right', 'index'] and old == [1, 2] and values[0] is old",
    "assert events == ['key', 'index', 'right', 'index'] and values == [[1, 2]] and old == [1] and values[0] is not old"
)]
#[case::augmented_inline_await_and_private_attribute(
    MutationOperator::AugmentedToBinary,
    r"events = []
async def value():
    events.append('await')
    return [2]
class Box:
    __value = old = [1]
    async def run(self): self.__value += await value(); return self.__value
box = Box()
try: box.run().send(None)
except StopIteration as stop: result = stop.value
",
    7,
    "assert result is box.old and result == [1, 2] and events == ['await']",
    "assert result == [1, 2] and box.old == [1] and result is not box.old and events == ['await']"
)]
#[case::augmented_operand_walrus_and_locals_are_in_the_original_frame(
    MutationOperator::AugmentedToBinary,
    r"class Box:
    items = old = [1]
    def run(self):
        self.items += (right := sorted(locals()))
        return right, sorted(locals())
receiver = Box()
result = receiver.run()
",
    4,
    "assert result == (['self'], ['right', 'self']) and receiver.old == [1, 'self']",
    "assert result == (['self'], ['right', 'self']) and receiver.items == [1, 'self'] and receiver.old == [1]"
)]
#[case::nested_unpacking_store_order(
    MutationOperator::AssignmentTargetSwap,
    r"events = []
class Target:
    def __setitem__(self, key, value): events.append((key, value))
target = Target()
def source():
    events.append('rhs')
    return iter((iter((1, 2)), 3))
(
    (
        target['left'], target['right']
    ),
    tail,
) = source()
",
    10,
    "assert events == ['rhs', ('left', 1), ('right', 2)] and tail == 3",
    "assert events == ['rhs', ('right', 1), ('left', 2)] and tail == 3"
)]
#[case::star_swaps_with_grouped_target(
    MutationOperator::AssignmentTargetSwap,
    "def run():\n    (first), *tail = [1, 2, 3]\n    return first, tail\n",
    2,
    "assert run() == (1, [2, 3])",
    "assert run() == (3, [1, 2])"
)]
#[case::return_walrus_then_finally(
    MutationOperator::ReturnValueNone,
    r"events = []
def value():
    class Value:
        def __del__(self): events.append('release')
    return Value()
def run():
    try: return (slot := 2), value()
    finally: events.append(slot)
out = run()
",
    7,
    "assert out[0] == 2 and events == [2]",
    "assert out is None and events == ['release', 2]"
)]
#[case::return_string_is_not_a_docstring(
    MutationOperator::ReturnValueNone,
    "def run(): return'hello'\n",
    1,
    "assert run.__doc__ is None and run() == 'hello'",
    "assert run.__doc__ is None and run() is None"
)]
#[case::return_async_await(
    MutationOperator::ReturnValueNone,
    r"events = []
class Awaitable:
    def __await__(self):
        events.append('await')
        yield 'suspend'
        return 5
async def run(): return await Awaitable()
task = run()
suspension = task.send(None)
try: task.send(None)
except StopIteration as stop: result = stop.value
",
    7,
    "assert suspension == 'suspend' and result == 5 and events == ['await']",
    "assert suspension == 'suspend' and result is None and events == ['await']"
)]
#[case::return_sync_generator(
    MutationOperator::ReturnValueNone,
    r"def generator(): return (yield 'start')
it = generator()
start = next(it)
try: it.send('sent')
except StopIteration as stop: result = stop.value
",
    1,
    "assert start == 'start' and result == 'sent'",
    "assert start == 'start' and result is None"
)]
#[case::identity_chain_order_and_short_circuit(
    MutationOperator::IdentityEqualitySwap,
    r"events = []
class Value:
    def __eq__(self, other): events.append('eq'); return False
value = Value()
def mark(label):
    events.append(label)
    return value
result = (mark('first') is
          mark('second')
          is mark('third'))
",
    8,
    "assert result and events == ['first', 'second', 'third']",
    "assert not result and events == ['first', 'second', 'eq']"
)]
#[case::identity_two_tokens_keep_comment(
    MutationOperator::IdentityEqualitySwap,
    "class Value:\n    def __ne__(self, other): return False\na, b = Value(), Value()\nresult = (a is # note\n          not b)\n",
    4,
    "assert result",
    "assert not result"
)]
#[case::no_space_equality_delimiters(
    MutationOperator::IdentityEqualitySwap,
    "left, right = [1], [1]\nresult = left==right\n",
    2,
    "assert result",
    "assert not result"
)]
#[case::no_space_inequality_delimiters(
    MutationOperator::IdentityEqualitySwap,
    "left, right = [1], [1]\nresult = left!=right\n",
    2,
    "assert not result",
    "assert result"
)]
#[case::inline_loop_else_scope(
    MutationOperator::ElseSuiteRemove,
    r"slot = 'global'
def outer():
    captured = 'closure'
    def run():
        for _ in (): pass
        else: slot = captured; yield slot
        try: slot
        except UnboundLocalError: pass
        else_slot = 'finished'
        yield else_slot
    return run
run = outer()
",
    6,
    "assert list(run()) == ['closure', 'finished'] and run.__code__.co_freevars == ('captured',)",
    "assert list(run()) == ['finished'] and run.__code__.co_freevars == ('captured',) and 'slot' in run.__code__.co_varnames"
)]
#[case::while_else_transfer_targets_the_outer_loop(
    MutationOperator::ElseSuiteRemove,
    r"events = []
for i in range(2):
    while False: pass
    else: continue
    events.append(i)
",
    4,
    "assert events == []",
    "assert events == [0, 1]"
)]
#[case::try_else_preserves_finally_and_generator_classification(
    MutationOperator::ElseSuiteRemove,
    r"events = []
def run():
    try: events.append('body')
    except ValueError: events.append('except')
    else:
        if events:
            events.append('else')
            yield 'only yield'
    finally: events.append('finally')
",
    5,
    "assert list(run()) == ['only yield'] and events == ['body', 'else', 'finally']",
    "assert list(run()) == [] and events == ['body', 'finally']"
)]
#[case::inline_try_else_preserves_nonlocal_directive(
    MutationOperator::ElseSuiteRemove,
    r"def run():
    value = 1
    def inner():
        try: pass
        except ValueError: pass
        else: nonlocal value; value = 2
        return value
    return inner()
",
    6,
    "assert run() == 2",
    "assert run() == 1"
)]
#[case::continued_indentation_in_try_else(
    MutationOperator::ElseSuiteRemove,
    "events = []\ntry: pass\nexcept ValueError: pass\nelse:\n\\\n    # preserved comment\n\\\n    if True:\n\\\n        events.append(1)\n\\\n    events.append(2)\nfinally: events.append(3)\n",
    4,
    "assert events == [1, 2, 3]",
    "assert events == [3]"
)]
#[case::tab_indented_try_else_preserves_async_generator_classification(
    MutationOperator::ElseSuiteRemove,
    "async def run():\n\ttry: pass\n\texcept ValueError: pass\n\telse:\n\t\t# first line\n\t\tif True:\n\t\t\tyield (\n'original column'\n\t\t\t)\n\tfinally: events.append('finally')\nevents = []\nit = run()\ntry: anext(it).send(None)\nexcept StopIteration as stop:\n    result = stop.value\n    try: anext(it).send(None)\n    except StopAsyncIteration: pass\nexcept StopAsyncIteration: result = None\n",
    4,
    "assert result == 'original column' and events == ['finally']",
    "assert result is None and events == ['finally']"
)]
#[tokio::test]
async fn assignment_and_result_mutations_preserve_the_remaining_python_effects(
    #[case] operator: MutationOperator,
    #[case] source: &str,
    #[case] line: u32,
    #[case] baseline: &str,
    #[case] changed: &str,
) {
    let lines = [LineRange {
        start: line,
        end: line,
    }];
    assert_python(source, baseline).await;

    let candidates = mutants(source, operator, &lines, 10);

    let [mutant] = candidates.as_slice() else {
        panic!("expected one selected {operator:?} mutation, found {candidates:?}");
    };
    assert_python(mutant, changed).await;
}

#[test]
fn identity_operator_edit_preserves_comment_trivia() {
    let source = "result = (a is # keep\n          not b)\n";

    let actual = mutants(source, MutationOperator::IdentityEqualitySwap, &[], 1);

    assert_eq!(actual.len(), 1);
    assert!(actual[0].contains("# keep\n"));
}

#[tokio::test]
async fn binary_assignment_keeps_comments_inside_rewritten_target_grouping() {
    let source = "class Box: items = [1]\nbox = Box()\n(box.items # keep target note\n) += [2]\n";
    assert_python(source, "assert box.items is Box.items").await;

    let actual = mutants(source, MutationOperator::AugmentedToBinary, &[], 1);

    assert_eq!(actual.len(), 1);
    assert!(actual[0].contains("# keep target note\n"));
    assert_python(
        &actual[0],
        "assert box.items == [1, 2] and Box.items == [1]",
    )
    .await;
}

#[test]
fn assignment_swap_ignores_the_same_name_with_different_grouping() {
    let source = "name, (name) = iterable\n";

    let actual = mutants(source, MutationOperator::AssignmentTargetSwap, &[], 1);

    assert_eq!(actual, Vec::<String>::new());
}

#[rstest::rstest]
#[case::fits_cpython_limit(196, true)]
#[case::exceeds_cpython_limit(197, false)]
#[tokio::test]
async fn complex_augmented_assignment_respects_cpython_delimiter_limits(
    #[case] depth: usize,
    #[case] admitted: bool,
) {
    let source = format!(
        "class Box: values = [1]\nbox = Box()\nbox.values += {}[2]{}\n",
        "(".repeat(depth),
        ")".repeat(depth),
    );
    assert_python(&source, "assert box.values is Box.values").await;

    let candidates = mutants(&source, MutationOperator::AugmentedToBinary, &[], 10);

    assert_eq!(candidates.len(), usize::from(admitted));
    if let [candidate] = candidates.as_slice() {
        assert_python(
            candidate,
            "assert box.values == [1, 2] and Box.values == [1]",
        )
        .await;
    }
}

#[rstest::rstest]
#[case::getter_raises("Target()[0] += 1", "IndexError")]
#[case::binary_dispatch_raises("Target([1])[0] += object()", "TypeError")]
#[case::rhs_raises("Target([1])[0] += raise_rhs()", "RuntimeError")]
#[tokio::test]
async fn failed_augmented_operation_releases_receiver_before_handler(
    #[case] statement: &str,
    #[case] exception: &str,
) {
    let source = format!(
        "events = []\nclass Target(list):\n    def __del__(self): events.append('release')\n\
         def raise_rhs(): raise RuntimeError('rhs')\n\
         try: {statement}\nexcept {exception} as error:\n    observed = tuple(events)\n"
    );
    let expected = "assert observed == ('release',)";
    assert_python(&source, expected).await;

    let candidates = mutants(&source, MutationOperator::AugmentedToBinary, &[], 1);

    let [candidate] = candidates.as_slice() else {
        panic!("expected one augmented assignment mutation, found {candidates:?}");
    };
    assert_python(candidate, expected).await;
}

#[rstest::rstest]
#[case::fits_after_guard(98, true)]
#[case::exceeds_limit_after_guard(99, false)]
#[tokio::test]
async fn try_else_mutation_counts_its_extra_indent_in_cpython(
    #[case] depth: usize,
    #[case] admitted: bool,
) {
    let mut source = "effect = 0\ntry: pass\nexcept ValueError: pass\nelse:\n".to_owned();
    for indent in 1..depth {
        source.push_str(&" ".repeat(indent));
        source.push_str("if True:\n");
    }
    source.push_str(&" ".repeat(depth));
    source.push_str("effect = 1\n");
    assert_python(&source, "assert effect == 1").await;

    let candidates = mutants(&source, MutationOperator::ElseSuiteRemove, &[], 1);

    assert_eq!(candidates.len(), usize::from(admitted));
    if let [candidate] = candidates.as_slice() {
        assert_python(candidate, "assert effect == 0").await;
    }
}

proptest! {
    #[test]
    fn assignment_swap_candidate_caps_keep_the_same_prefix(
        width in 2..16usize,
        maximum in 1..16usize,
    ) {
        let targets = (0..width).map(|index| format!("v{index}")).collect::<Vec<_>>().join(", ");
        let source = format!("{targets} = values\n");
        let operator = MutationOperator::AssignmentTargetSwap;

        let all = mutants(&source, operator, &[], 16);
        let limited = mutants(&source, operator, &[], maximum);

        prop_assert_eq!(all.len(), width - 1);
        prop_assert_eq!(limited.as_slice(), &all[..maximum.min(width - 1)]);
        for (index, mutant) in all.iter().enumerate() {
            let parsed = ruff_python_parser::parse_module(mutant.as_str()).unwrap();
            let statement = parsed.syntax().body[0].as_assign_stmt().unwrap();
            let actual = statement.targets[0].as_tuple_expr().unwrap().elts.iter()
                .map(|target| target.as_name_expr().unwrap().id.as_str())
                .collect::<Vec<_>>();
            let mut expected = (0..width).map(|position| format!("v{position}")).collect::<Vec<_>>();
            expected.swap(index, index + 1);
            prop_assert_eq!(actual, expected);
            prop_assert_eq!(statement.value.as_name_expr().unwrap().id.as_str(), "values");
        }
    }
}
