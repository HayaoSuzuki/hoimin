use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct ContractCase<'a> {
    name: &'a str,
    operator: &'a str,
    original: &'a str,
    replacement: &'a str,
    source: &'a str,
    harness: &'a str,
    baseline_stdout: &'a str,
    mutant_stdout: &'a str,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn python_executable() -> PathBuf {
    env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            if cfg!(windows) {
                repo_root().join(".venv/Scripts/python.exe")
            } else {
                repo_root().join(".venv/bin/python")
            }
        },
        PathBuf::from,
    )
}

async fn plan(root: &Path, operator: &str) -> serde_json::Value {
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("subject.py"),
        OsString::from("--operators"),
        OsString::from(operator),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.into_os_string(),
        OsString::from("-c"),
        OsString::from("pass"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    serde_json::from_slice(&stdout).expect("plan writes one JSON document")
}

#[tokio::test]
async fn planned_pattern_candidates_compile_with_cpython() {
    let directory = tempfile::tempdir().unwrap();
    let source = concat!(
        "class Point:\n    __match_args__ = ('x',)\n",
        "def classify(value, guard):\n",
        "    subject = -value\n",
        "    match subject:\n",
        "        case -1 | -1.5 | -2j | -3-4j | -3+4j:\n            pass\n",
        "        case [(-5), Point(-6), {-7: True}]:\n            pass\n",
        "        case False if +guard:\n            return +value\n",
        "    return -value\n",
    );
    fs::write(directory.path().join("subject.py"), source).unwrap();

    let manifest = plan(
        directory.path(),
        "unary_sign,binary_add_sub,boolean_literal",
    )
    .await;
    let candidates = manifest["candidates"].as_array().unwrap();
    assert!(
        !candidates.is_empty(),
        "the CPython check must not be vacuous"
    );
    assert_eq!(
        candidates
            .iter()
            .filter(|candidate| candidate["operator"] == "unary_sign")
            .count(),
        4,
        "only expression-context unary signs should remain: {manifest}"
    );
    assert!(
        candidates.iter().any(|candidate| {
            candidate["operator"] == "binary_add_sub" && candidate["original"] == "-"
        }),
        "the complex separator should remain eligible: {manifest}"
    );
    assert_eq!(
        candidates
            .iter()
            .filter(|candidate| candidate["operator"] == "boolean_literal")
            .count(),
        2,
        "boolean pattern candidates should remain eligible: {manifest}"
    );

    for candidate in candidates {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutated = source.to_owned();
        mutated.replace_range(
            start..start + length,
            candidate["replacement"].as_str().unwrap(),
        );
        fs::write(directory.path().join("subject.py"), mutated).unwrap();
        let output = run_python(
            directory.path(),
            "compile(open('subject.py').read(), 'subject.py', 'exec')",
        );
        assert!(
            output.status.success(),
            "candidate {} failed CPython compilation: {}",
            candidate["id"],
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn run_python(root: &Path, harness: &str) -> Output {
    Command::new(python_executable())
        .args(["-c", harness])
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("controlled Python interpreter runs")
}

fn assert_python_output(name: &str, phase: &str, output: &Output, expected: &str) {
    assert!(
        output.status.success(),
        "{name} {phase} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "{name} {phase} stdout"
    );
}

async fn assert_contract(case: ContractCase<'_>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("subject.py");
    fs::write(&path, case.source).unwrap();

    let manifest = plan(directory.path(), case.operator).await;
    let candidates = manifest["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "{}: {manifest}", case.name);
    let candidate = &candidates[0];
    assert_eq!(candidate["path"], "subject.py", "{}", case.name);
    assert_eq!(candidate["operator"], case.operator, "{}", case.name);
    assert_eq!(candidate["original"], case.original, "{}", case.name);
    let replacement = candidate["replacement"].as_str().unwrap();

    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    let end = start.checked_add(length).unwrap();
    assert_eq!(&case.source[start..end], case.original, "{}", case.name);

    let baseline = run_python(directory.path(), case.harness);
    assert_python_output(case.name, "baseline", &baseline, case.baseline_stdout);

    let mut mutated = case.source.to_owned();
    mutated.replace_range(start..end, replacement);
    fs::write(&path, mutated).unwrap();
    let mutant = run_python(directory.path(), case.harness);
    assert_python_output(case.name, "mutant", &mutant, case.mutant_stdout);
    assert_ne!(
        baseline.stdout, mutant.stdout,
        "{} must change externally observable behavior",
        case.name
    );
    assert_eq!(replacement, case.replacement, "{}", case.name);
}

const JSON_RUN_HARNESS: &str = concat!(
    "import json, subject\n",
    "print(json.dumps(subject.run(), separators=(',', ':')))\n",
);

#[tokio::test]
async fn structural_mapping_and_append_mutants_keep_grouped_calls() {
    let cases = [
        ContractCase {
            name: "grouped mapping get",
            operator: "structure_mapping_get_subscript",
            original: "(holder\n        .mapping).get(key())",
            replacement: "(holder\n        .mapping)[key()]",
            source: r"events = []
class Mapping:
    def get(self, key):
        events.append(['get', key])
        return ['get', key]
    def __getitem__(self, key):
        events.append(['item', key])
        return ['item', key]
class Holder:
    pass
holder = Holder()
holder.mapping = Mapping()
def key():
    events.append('key')
    return 'x'
def run():
    events.clear()
    return [(holder
        .mapping).get(key()), events]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[[\"get\",\"x\"],[\"key\",[\"get\",\"x\"]]]\n",
            mutant_stdout: "[[\"item\",\"x\"],[\"key\",[\"item\",\"x\"]]]\n",
        },
        ContractCase {
            name: "grouped append",
            operator: "structure_append_extend",
            original: "(items\n        .append)(value())",
            replacement: "(items\n        .extend)([value()])",
            source: r"events = []
class Items:
    def append(self, value):
        events.insert(len(events), ['append', value])
    def extend(self, values):
        events.insert(len(events), ['extend', values])
items = Items()
def value():
    events.insert(len(events), 'value')
    return 'x'
def run():
    events.clear()
    (items
        .append)(value())
    return events
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"value\",[\"append\",\"x\"]]\n",
            mutant_stdout: "[\"value\",[\"extend\",[\"x\"]]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn collection_mutants_keep_parenthesized_argument_evaluation() {
    let cases = [
        ContractCase {
            name: "parenthesized append argument",
            operator: "collection_append_insert",
            original: "items.append((value()))",
            replacement: "items.insert(0, (value()))",
            source: r"events = []
class Items:
    def append(self, value):
        events.insert(len(events), ['append', value])
    def insert(self, index, value):
        events.insert(len(events), ['insert', index, value])
items = Items()
def value():
    events.insert(len(events), 'value')
    return 'x'
def run():
    events.clear()
    items.append((value()))
    return events
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"value\",[\"append\",\"x\"]]\n",
            mutant_stdout: "[\"value\",[\"insert\",0,\"x\"]]\n",
        },
        ContractCase {
            name: "parenthesized insert arguments",
            operator: "collection_append_insert",
            original: "items.insert((0), ((value())))",
            replacement: "items.append(((value())))",
            source: r"events = []
class Items:
    def append(self, value):
        events.insert(len(events), ['append', value])
    def insert(self, index, value):
        events.insert(len(events), ['insert', index, value])
items = Items()
def value():
    events.insert(len(events), 'value')
    return 'x'
def run():
    events.clear()
    items.insert((0), ((value())))
    return events
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"value\",[\"insert\",0,\"x\"]]\n",
            mutant_stdout: "[\"value\",[\"append\",\"x\"]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn structural_extend_keeps_call_trailing_comma() {
    assert_contract(ContractCase {
        name: "extend call trailing comma",
        operator: "structure_append_extend",
        original: "items.extend([value,],)",
        replacement: "items.append(value,)",
        source: r"events = []
class Items:
    def append(self, value):
        events.insert(len(events), ['append', value])
    def extend(self, values):
        events.insert(len(events), ['extend', values])
items = Items()
value = 'x'
def run():
    events.clear()
    items.extend([value,],)
    return events
",
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[[\"extend\",[\"x\"]]]\n",
        mutant_stdout: "[[\"append\",\"x\"]]\n",
    })
    .await;
}

#[tokio::test]
async fn structural_extend_keeps_tuple_element_and_comment() {
    assert_contract(ContractCase {
        name: "extend tuple element comment",
        operator: "structure_append_extend",
        original: "items.extend([\n        (value(),), # kept\n    ])",
        replacement: "items.append(\n        (value(),) # kept\n    )",
        source: r"events = []
class Items:
    def append(self, value):
        events.insert(len(events), ['append', value])
    def extend(self, values):
        events.insert(len(events), ['extend', values])
items = Items()
def value():
    events.insert(len(events), 'value')
    return 'x'
def run():
    events.clear()
    items.extend([
        (value(),), # kept
    ])
    return events
",
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[\"value\",[\"extend\",[[\"x\"]]]]\n",
        mutant_stdout: "[\"value\",[\"append\",[\"x\"]]]\n",
    })
    .await;
}

macro_rules! augmented_source {
    ($original_method:literal, $original_result:literal, $replacement_method:literal, $replacement_result:literal, $operator:literal) => {
        concat!(
            "events = []\n",
            "class Probe:\n",
            "    def __init__(self, name): self.name = name\n",
            "    def ",
            $original_method,
            "(self, other): events.append('",
            $original_method,
            "'); return ",
            stringify!($original_result),
            "\n",
            "    def ",
            $replacement_method,
            "(self, other): events.append('",
            $replacement_method,
            "'); return ",
            stringify!($replacement_result),
            "\n",
            "left = Probe('left')\n",
            "right = Probe('right')\n",
            "def operand(value):\n",
            "    events.append(value)\n",
            "    return value\n",
            "def run():\n",
            "    events.clear()\n",
            "    result = operand(left)\n",
            "    result ",
            $operator,
            " operand(right)\n",
            "    observed = [item.name if isinstance(item, Probe) else item for item in events]\n",
            "    return [result, observed]\n",
        )
    };
}

#[tokio::test]
async fn syntax_mutants_change_results_and_special_method_dispatch() {
    let cases = [
        ContractCase {
            name: "binary power",
            operator: "binary_power",
            original: "**",
            replacement: "*",
            source: r"events = []
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = operand(2) ** operand(3)
    return [result, events]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[8,[2,3]]\n",
            mutant_stdout: "[6,[2,3]]\n",
        },
        ContractCase {
            name: "binary matrix multiplication",
            operator: "binary_matmul",
            original: "@",
            replacement: "*",
            source: r"events = []
class Probe:
    def __init__(self, name): self.name = name
    def __matmul__(self, other): events.append('__matmul__'); return 11
    def __mul__(self, other): events.append('__mul__'); return 7
left = Probe('left')
right = Probe('right')
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = operand(left) @ operand(right)
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[11,[\"left\",\"right\",\"__matmul__\"]]\n",
            mutant_stdout: "[7,[\"left\",\"right\",\"__mul__\"]]\n",
        },
        ContractCase {
            name: "binary bitwise xor",
            operator: "bitwise_xor",
            original: "^",
            replacement: "&",
            source: r"events = []
class Probe:
    def __init__(self, name): self.name = name
    def __xor__(self, other): events.append('__xor__'); return 12
    def __and__(self, other): events.append('__and__'); return 4
left = Probe('left')
right = Probe('right')
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = operand(left) ^ operand(right)
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[12,[\"left\",\"right\",\"__xor__\"]]\n",
            mutant_stdout: "[4,[\"left\",\"right\",\"__and__\"]]\n",
        },
        ContractCase {
            name: "bitwise invert",
            operator: "bitwise_invert",
            original: "~",
            replacement: "+",
            source: r"events = []
class Probe:
    name = 'value'
    def __invert__(self): events.append('__invert__'); return 9
    def __pos__(self): events.append('__pos__'); return 3
value = Probe()
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = ~operand(value)
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[9,[\"value\",\"__invert__\"]]\n",
            mutant_stdout: "[3,[\"value\",\"__pos__\"]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn augmented_syntax_mutants_change_in_place_protocol_dispatch() {
    let cases = [
        ContractCase {
            name: "augmented power",
            operator: "augmented_power",
            original: "**=",
            replacement: "*=",
            source: augmented_source!("__ipow__", 8, "__imul__", 6, "**="),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[8,[\"left\",\"right\",\"__ipow__\"]]\n",
            mutant_stdout: "[6,[\"left\",\"right\",\"__imul__\"]]\n",
        },
        ContractCase {
            name: "augmented matrix multiplication",
            operator: "augmented_matmul",
            original: "@=",
            replacement: "*=",
            source: augmented_source!("__imatmul__", 13, "__imul__", 6, "@="),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[13,[\"left\",\"right\",\"__imatmul__\"]]\n",
            mutant_stdout: "[6,[\"left\",\"right\",\"__imul__\"]]\n",
        },
        ContractCase {
            name: "augmented bitwise and/or",
            operator: "augmented_bitwise_and_or",
            original: "&=",
            replacement: "|=",
            source: augmented_source!("__iand__", 1, "__ior__", 5, "&="),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[1,[\"left\",\"right\",\"__iand__\"]]\n",
            mutant_stdout: "[5,[\"left\",\"right\",\"__ior__\"]]\n",
        },
        ContractCase {
            name: "augmented bitwise xor",
            operator: "augmented_bitwise_xor",
            original: "^=",
            replacement: "&=",
            source: augmented_source!("__ixor__", 12, "__iand__", 4, "^="),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[12,[\"left\",\"right\",\"__ixor__\"]]\n",
            mutant_stdout: "[4,[\"left\",\"right\",\"__iand__\"]]\n",
        },
        ContractCase {
            name: "augmented bitwise shift",
            operator: "augmented_bitwise_shift",
            original: "<<=",
            replacement: ">>=",
            source: augmented_source!("__ilshift__", 16, "__irshift__", 2, "<<="),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[16,[\"left\",\"right\",\"__ilshift__\"]]\n",
            mutant_stdout: "[2,[\"left\",\"right\",\"__irshift__\"]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn operator_function_arithmetic_and_matrix_protocol_replacements_execute() {
    let cases = [
        ContractCase {
            name: "operator.pow",
            operator: "operator_function",
            original: "pow",
            replacement: "mul",
            source: r"import operator as op
events = []
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = op.pow(operand(2), operand(3))
    return [result, events]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[8,[2,3]]\n",
            mutant_stdout: "[6,[2,3]]\n",
        },
        ContractCase {
            name: "operator.matmul",
            operator: "operator_function",
            original: "matmul",
            replacement: "mul",
            source: r"import operator as op
events = []
class Probe:
    def __init__(self, name): self.name = name
    def __matmul__(self, other): events.append('__matmul__'); return 11
    def __mul__(self, other): events.append('__mul__'); return 7
left = Probe('left')
right = Probe('right')
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = op.matmul(operand(left), operand(right))
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[11,[\"left\",\"right\",\"__matmul__\"]]\n",
            mutant_stdout: "[7,[\"left\",\"right\",\"__mul__\"]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn class_comprehension_function_replacement_uses_module_binding() {
    assert_contract(ContractCase {
        name: "class comprehension module binding",
        operator: "operator_function",
        original: "add",
        replacement: "sub",
        source: r"import operator as op
class ClassOperator:
    @staticmethod
    def add(left, right): return (0,)
class Meta(type):
    @classmethod
    def __prepare__(mcls, name, bases): return {'op': ClassOperator}
class Subject(metaclass=Meta):
    direct = op.add(2, 3)
    values = [op.add(2, 3) for _ in op.add(None, None)]
",
        harness: "from subject import Subject; print(Subject.values)\n",
        baseline_stdout: "[5]\n",
        mutant_stdout: "[-1]\n",
    })
    .await;
}

#[tokio::test]
async fn operator_function_bitwise_unary_and_inplace_replacements_execute() {
    let cases = [
        ContractCase {
            name: "operator.xor",
            operator: "operator_function",
            original: "xor",
            replacement: "and_",
            source: r"import operator as op
events = []
class Probe:
    name = 'value'
    def __xor__(self, other): events.append('__xor__'); return 12
    def __and__(self, other): events.append('__and__'); return 4
value = Probe()
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = op.xor(operand(value), operand(value))
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[12,[\"value\",\"value\",\"__xor__\"]]\n",
            mutant_stdout: "[4,[\"value\",\"value\",\"__and__\"]]\n",
        },
        ContractCase {
            name: "operator.invert",
            operator: "operator_function",
            original: "invert",
            replacement: "pos",
            source: r"import operator as op
events = []
class Probe:
    name = 'value'
    def __invert__(self): events.append('__invert__'); return 9
    def __pos__(self): events.append('__pos__'); return 3
value = Probe()
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = op.invert(operand(value))
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[9,[\"value\",\"__invert__\"]]\n",
            mutant_stdout: "[3,[\"value\",\"__pos__\"]]\n",
        },
        ContractCase {
            name: "operator.ipow",
            operator: "operator_function",
            original: "ipow",
            replacement: "imul",
            source: r"import operator as op
events = []
class Probe:
    name = 'value'
    def __ipow__(self, other): events.append('__ipow__'); return 8
    def __imul__(self, other): events.append('__imul__'); return 6
value = Probe()
def operand(value):
    events.append(value)
    return value
def run():
    events.clear()
    result = op.ipow(operand(value), operand(value))
    observed = [item.name if isinstance(item, Probe) else item for item in events]
    return [result, observed]
",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[8,[\"value\",\"value\",\"__ipow__\"]]\n",
            mutant_stdout: "[6,[\"value\",\"value\",\"__imul__\"]]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn contains_and_setitem_mutants_keep_argument_order_and_remove_effects() {
    let cases = [
        ContractCase {
            name: "contains higher-order reference",
            operator: "operator_function",
            original: "op.contains",
            replacement: "(lambda container, item, /: item not in container)",
            source: "import operator as op\naction = op.contains\n",
            harness: concat!(
                "import json, subject\n",
                "events = []\n",
                "def operand(value):\n",
                "    events.append(value)\n",
                "    return value\n",
                "result = subject.action(operand(['needle']), operand('needle'))\n",
                "try:\n",
                "    subject.action(container=['needle'], item='needle')\n",
                "except TypeError as error:\n",
                "    positional_only = type(error).__name__\n",
                "else:\n",
                "    positional_only = 'accepted'\n",
                "print(json.dumps([result, events, positional_only], separators=(',', ':')))\n",
            ),
            baseline_stdout: "[true,[[\"needle\"],\"needle\"],\"TypeError\"]\n",
            mutant_stdout: "[false,[[\"needle\"],\"needle\"],\"TypeError\"]\n",
        },
        ContractCase {
            name: "from-import setitem reference",
            operator: "operator_function",
            original: "update",
            replacement: "(lambda container, key, value, /: None)",
            source: "from operator import setitem as update\naction = update\n",
            harness: concat!(
                "import json, subject\n",
                "events = []\n",
                "target = {}\n",
                "def operand(value):\n",
                "    events.append(value)\n",
                "    return value\n",
                "subject.action(operand(target), operand('key'), operand('value'))\n",
                "try:\n",
                "    subject.action(container=target, key='x', value='y')\n",
                "except TypeError as error:\n",
                "    positional_only = type(error).__name__\n",
                "else:\n",
                "    positional_only = 'accepted'\n",
                "observed = ['target' if item is target else item for item in events]\n",
                "print(json.dumps([target, observed, positional_only], separators=(',', ':'), sort_keys=True))\n",
            ),
            baseline_stdout: "[{\"key\":\"value\"},[\"target\",\"key\",\"value\"],\"TypeError\"]\n",
            mutant_stdout: "[{},[\"target\",\"key\",\"value\"],\"TypeError\"]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn delitem_and_call_mutants_keep_argument_order_and_remove_effects() {
    let cases = [
        ContractCase {
            name: "dunder delitem reference",
            operator: "operator_function",
            original: "op.__delitem__",
            replacement: "(lambda container, key, /: None)",
            source: "import operator as op\naction = op.__delitem__\n",
            harness: concat!(
                "import json, subject\n",
                "events = []\n",
                "target = {'key': 'value'}\n",
                "def operand(value):\n",
                "    events.append(value)\n",
                "    return value\n",
                "subject.action(operand(target), operand('key'))\n",
                "try:\n",
                "    subject.action(container=target, key='x')\n",
                "except TypeError as error:\n",
                "    positional_only = type(error).__name__\n",
                "else:\n",
                "    positional_only = 'accepted'\n",
                "observed = ['target' if item is target else item for item in events]\n",
                "print(json.dumps([target, observed, positional_only], separators=(',', ':'), sort_keys=True))\n",
            ),
            baseline_stdout: "[{},[\"target\",\"key\"],\"TypeError\"]\n",
            mutant_stdout: "[{\"key\":\"value\"},[\"target\",\"key\"],\"TypeError\"]\n",
        },
        ContractCase {
            name: "from-import call reference",
            operator: "operator_function",
            original: "invoke",
            replacement: "(lambda target, /, *args, **kwargs: None)",
            source: "from operator import call as invoke\naction = invoke\n",
            harness: concat!(
                "import json, subject\n",
                "events = []\n",
                "def operand(value):\n",
                "    events.append(value)\n",
                "    return value\n",
                "def target(value, *, named):\n",
                "    events.append('called')\n",
                "    return value + named\n",
                "result = subject.action(operand(target), operand(4), named=operand(5))\n",
                "try:\n",
                "    subject.action(target=target)\n",
                "except TypeError as error:\n",
                "    positional_only = type(error).__name__\n",
                "else:\n",
                "    positional_only = 'accepted'\n",
                "observed = ['target' if item is target else item for item in events]\n",
                "print(json.dumps([result, observed, positional_only], separators=(',', ':')))\n",
            ),
            baseline_stdout: "[9,[\"target\",4,5,\"called\"],\"TypeError\"]\n",
            mutant_stdout: "[null,[\"target\",4,5],\"TypeError\"]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn from_import_dunder_and_higher_order_function_replacements_execute() {
    let cases = [
        ContractCase {
            name: "from-import function pair",
            operator: "operator_function",
            original: "plus",
            replacement: "__import__('operator').sub",
            source: "from operator import add as plus\naction = plus\n",
            harness: "import subject; print(subject.action(8, 3))\n",
            baseline_stdout: "11\n",
            mutant_stdout: "5\n",
        },
        ContractCase {
            name: "documented dunder pair",
            operator: "operator_function",
            original: "__add__",
            replacement: "__sub__",
            source: "import operator as op\naction = op.__add__\n",
            harness: "import subject; print(subject.action(8, 3))\n",
            baseline_stdout: "11\n",
            mutant_stdout: "5\n",
        },
        ContractCase {
            name: "map higher-order reference",
            operator: "operator_function",
            original: "add",
            replacement: "sub",
            source: concat!(
                "import operator as op\n",
                "def run():\n",
                "    return list(map(op.add, [10, 20], [1, 2]))\n",
            ),
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[11,22]\n",
            mutant_stdout: "[9,18]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn nullable_annotation_removal_preserves_type_members_at_annotation_sites() {
    let cases = [
        ContractCase {
            name: "variable Optional grouped union",
            operator: "type_nullable_remove",
            original: "Optional[(int\n | str)]",
            replacement: "(int\n | str)",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[(int\n | str)]\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "parameter Optional grouped union",
            operator: "type_nullable_remove",
            original: "Optional[(int\n | str)]",
            replacement: "(int\n | str)",
            source: "from typing import Optional, get_type_hints\ndef target(value: Optional[(int\n | str)]):\n    pass\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(target)[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "return Optional grouped union",
            operator: "type_nullable_remove",
            original: "Optional[(int\n | str)]",
            replacement: "(int\n | str)",
            source: "from typing import Optional, get_type_hints\ndef target() -> Optional[(int\n | str)]:\n    pass\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(target)[\"return\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn nullable_optional_removal_preserves_source_layout() {
    let cases = [
        ContractCase {
            name: "Optional unparenthesized multiline union",
            operator: "type_nullable_remove",
            original: "Optional[int\n | str]",
            replacement: "(int\n | str)",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[int\n | str]\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "Optional comments",
            operator: "type_nullable_remove",
            original: "Optional[\n    # retained leading comment\n    int\n    | str  # retained trailing comment\n]",
            replacement: "(\n    # retained leading comment\n    int\n    | str  # retained trailing comment\n)",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[\n    # retained leading comment\n    int\n    | str  # retained trailing comment\n]\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "Optional nested grouping",
            operator: "type_nullable_remove",
            original: "Optional[((int\n | str))]",
            replacement: "((int\n | str))",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[((int\n | str))]\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "Optional grouped union surrounding whitespace",
            operator: "type_nullable_remove",
            original: "Optional[\n    (int | str)\n]",
            replacement: "(\n    (int | str)\n)",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[\n    (int | str)\n]\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "ordinary Optional",
            operator: "type_nullable_remove",
            original: "Optional[int]",
            replacement: "int",
            source: "from typing import Optional, get_type_hints\nvalue: Optional[int]\ndef run():\n    annotation = get_type_hints(__import__(__name__))[\"value\"]\n    return sorted(member.__name__ for member in getattr(annotation, \"__args__\", (annotation,)))\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\"]\n",
            mutant_stdout: "[\"int\"]\n",
        },
        ContractCase {
            name: "Optional hash string literal",
            operator: "type_nullable_remove",
            original: "Optional[resolve(\"#\")]",
            replacement: "resolve(\"#\")",
            source: "from typing import Optional, get_type_hints\ndef resolve(value):\n    return int\nvalue: Optional[resolve(\"#\")]\ndef run():\n    annotation = get_type_hints(__import__(__name__))[\"value\"]\n    return sorted(member.__name__ for member in getattr(annotation, \"__args__\", (annotation,)))\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\"]\n",
            mutant_stdout: "[\"int\"]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn nullable_union_removal_preserves_source_layout() {
    let cases = [
        ContractCase {
            name: "None leading grouped union",
            operator: "type_nullable_remove",
            original: "None | (int\n | str)",
            replacement: "(int\n | str)",
            source: "from typing import get_type_hints\nvalue: None | (int\n | str)\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "None trailing grouped union",
            operator: "type_nullable_remove",
            original: "(int\n | str) | None",
            replacement: "(int\n | str)",
            source: "from typing import get_type_hints\nvalue: (int\n | str) | None\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "None trailing ungrouped multiline union",
            operator: "type_nullable_remove",
            original: "int\n | str\n | None",
            replacement: "(int\n | str)",
            source: "from typing import get_type_hints\nvalue: (int\n | str\n | None)\ndef run():\n    return sorted(member.__name__ for member in get_type_hints(__import__(__name__))[\"value\"].__args__)\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\",\"str\"]\n",
            mutant_stdout: "[\"int\",\"str\"]\n",
        },
        ContractCase {
            name: "ordinary trailing None",
            operator: "type_nullable_remove",
            original: "int | None",
            replacement: "int",
            source: "from typing import get_type_hints\nvalue: int | None\ndef run():\n    annotation = get_type_hints(__import__(__name__))[\"value\"]\n    return sorted(member.__name__ for member in getattr(annotation, \"__args__\", (annotation,)))\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\"]\n",
            mutant_stdout: "[\"int\"]\n",
        },
        ContractCase {
            name: "None trailing hash string literal",
            operator: "type_nullable_remove",
            original: "resolve(\"#\") | None",
            replacement: "resolve(\"#\")",
            source: "from typing import get_type_hints\ndef resolve(value):\n    return int\nvalue: resolve(\"#\") | None\ndef run():\n    annotation = get_type_hints(__import__(__name__))[\"value\"]\n    return sorted(member.__name__ for member in getattr(annotation, \"__args__\", (annotation,)))\n",
            harness: JSON_RUN_HARNESS,
            baseline_stdout: "[\"NoneType\",\"int\"]\n",
            mutant_stdout: "[\"int\"]\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}

#[tokio::test]
async fn python_314_none_identity_function_replacements_execute() {
    let cases = [
        ContractCase {
            name: "operator.is_none",
            operator: "operator_function",
            original: "is_none",
            replacement: "is_not_none",
            source: "import operator as op\naction = op.is_none\n",
            harness: "import subject; print(subject.action(None))\n",
            baseline_stdout: "True\n",
            mutant_stdout: "False\n",
        },
        ContractCase {
            name: "from-import operator.is_not_none",
            operator: "operator_function",
            original: "present",
            replacement: "__import__('operator').is_none",
            source: "from operator import is_not_none as present\naction = present\n",
            harness: "import subject; print(subject.action(None))\n",
            baseline_stdout: "False\n",
            mutant_stdout: "True\n",
        },
    ];

    for case in cases {
        assert_contract(case).await;
    }
}
