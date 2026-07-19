from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import libcst as cst
import pytest

ROOT = Path(__file__).parents[2]
ANALYZER = ROOT / "python" / "hoimin_analyzer.py"
FIXTURES = Path(__file__).parent / "fixtures"


def invoke(  # noqa: PLR0913 -- Helper mirrors the six independent analyzer request fields.
    source: str,
    *,
    path: str = "pkg/sample.py",
    lines: list[list[int]] | None = None,
    symbols: list[str] | None = None,
    max_candidates: int = 10_000,
    effect_id: str = "effect-7",
) -> list[dict[str, object]]:
    request = {
        "effect_id": effect_id,
        "path": path,
        "module": source,
        "lines": lines or [],
        "symbols": symbols or [],
        "max_candidates": max_candidates,
    }
    completed = subprocess.run(  # noqa: S603 -- Tests execute the repository-owned analyzer with a fixed interpreter.
        [sys.executable, str(ANALYZER)],
        input=json.dumps(request) + "\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    assert completed.stderr == ""
    assert completed.returncode == 0
    return [json.loads(line) for line in completed.stdout.splitlines()]


def invoke_raw(payload: str) -> list[dict[str, object]]:
    completed = subprocess.run(  # noqa: S603 -- Tests execute the repository-owned analyzer with a fixed interpreter.
        [sys.executable, str(ANALYZER)],
        input=payload + "\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    assert completed.stderr == ""
    assert completed.returncode == 0
    return [json.loads(line) for line in completed.stdout.splitlines()]


def candidates(events: list[dict[str, object]]) -> list[dict[str, object]]:
    return [event for event in events if event["kind"] == "candidate"]


def apply_candidate(source: str, candidate: dict[str, object]) -> str:
    raw = source.encode()
    span = candidate["span"]
    assert isinstance(span, dict)
    start = span["start"]
    end = start + span["length"]
    mutated = raw[:start] + str(candidate["replacement"]).encode() + raw[end:]
    return mutated.decode()


def test_emits_every_mvp_operator_as_one_location_mutants():
    source = (FIXTURES / "operators.py").read_text(encoding="utf-8")
    events = invoke(source, path="python/tests/fixtures/operators.py")
    operators = {event["operator"] for event in candidates(events)}
    assert operators == {
        "compare_eq_ne",
        "compare_order",
        "membership",
        "identity",
        "boolean_and_or",
        "binary_add_sub",
        "augmented_add_sub",
        "binary_mul_div",
        "binary_floor_mod",
        "unary_sign",
        "remove_not",
        "boolean_literal",
        "break_continue",
    }
    for candidate in candidates(events):
        cst.parse_module(apply_candidate(source, candidate))


def test_exact_bidirectional_replacement_table():
    source = """\
def replacements(a, b, items, flag):
    values = [
        a == b, a != b, a < b, a <= b, a > b, a >= b,
        a in items, a not in items, a is b, a is not b,
        a and b, a or b, a + b, a - b, a * b, a / b, a // b, a % b,
        +a, -a, not flag, True, False,
    ]
    a += b
    a -= b
    for item in items:
        if item:
            break
        continue
    return values
"""
    pairs = {(record["original"], record["replacement"]) for record in candidates(invoke(source))}
    assert pairs == {
        ("==", "!="),
        ("!=", "=="),
        ("<", "<="),
        ("<=", "<"),
        (">", ">="),
        (">=", ">"),
        ("in", "not in"),
        ("not in", "in"),
        ("is", "is not"),
        ("is not", "is"),
        ("and", "or"),
        ("or", "and"),
        ("+", "-"),
        ("-", "+"),
        ("+=", "-="),
        ("-=", "+="),
        ("*", "/"),
        ("/", "*"),
        ("//", "%"),
        ("%", "//"),
        ("not flag", "flag"),
        ("True", "False"),
        ("False", "True"),
        ("break", "continue"),
        ("continue", "break"),
    }


def test_unicode_and_crlf_spans_are_utf8_bytes_and_round_trip():
    source = '# \u65e5\u672c\u8a9e\r\nvalue = "\u96ea"\r\nresult = value == "\u96ea"\r\n'
    event = candidates(invoke(source))[0]
    span = event["span"]
    raw = source.encode()
    assert raw[span["start"] : span["start"] + span["length"]].decode() == event["original"]
    assert event["line"] == 3
    assert event["column"] == 15


def test_line_filtering_compares_complete_candidate_record():
    source = "first = left == right\nsecond = left + right\n"
    assert candidates(invoke(source, lines=[[2, 2]])) == [
        {
            "kind": "candidate",
            "effect_id": "effect-7",
            "path": "pkg/sample.py",
            "span": {"start": 36, "length": 1},
            "original": "+",
            "replacement": "-",
            "operator": "binary_add_sub",
            "line": 2,
            "column": 14,
            "symbol": None,
        }
    ]


def test_module_qualname_symbol_filtering():
    source = "def selected():\n    return 1 + 2\n\ndef ignored():\n    return 3 - 4\n"
    records = candidates(invoke(source, symbols=["pkg.sample:selected"]))
    assert [(r["original"], r["replacement"], r["symbol"]) for r in records] == [
        ("+", "-", "selected")
    ]
    assert candidates(invoke(source, symbols=["other.module:selected"])) == []


def test_nested_class_and_function_qualnames_are_syntactic():
    source = (
        "class Outer:\n"
        "    class Inner:\n"
        "        def method(self):\n"
        "            def nested():\n"
        "                return 1 == 2\n"
        "            return nested()\n"
    )
    records = candidates(invoke(source, symbols=["pkg.sample:Outer.Inner.method.nested"]))
    assert len(records) == 1
    assert records[0]["symbol"] == "Outer.Inner.method.nested"


def test_decorators_and_async_functions_belong_to_the_declared_symbol():
    source = "@decorate(enabled=True)\nasync def fetch():\n    return False\n"
    records = candidates(invoke(source, symbols=["pkg.sample:fetch"]))
    assert [(r["original"], r["replacement"], r["symbol"]) for r in records] == [
        ("True", "False", "fetch"),
        ("False", "True", "fetch"),
    ]


def test_formatting_comments_and_parentheses_survive_local_replacements():
    source = "result = (\n    left  # keep left\n    +  # keep operator\n    right\n)\n"
    record = candidates(invoke(source))[0]
    mutated = apply_candidate(source, record)
    assert "# keep left" in mutated
    assert "# keep operator" in mutated
    assert "    -  # keep operator" in mutated
    cst.parse_module(mutated)


def test_compound_comparisons_preserve_noncanonical_keyword_spacing():
    source = "membership = item not  in values\nidentity = item is\t not sentinel\n"
    events = invoke(source)
    records = [
        record for record in candidates(events) if record["operator"] in {"membership", "identity"}
    ]
    assert [(record["original"], record["replacement"]) for record in records] == [
        ("not  in", "in"),
        ("is\t not", "is"),
    ]
    assert not [event for event in events if event.get("code") == "unreconstructable_span"]
    for record in records:
        cst.parse_module(apply_candidate(source, record))


@pytest.mark.parametrize("newline", ["\n", "\r\n"], ids=["lf", "crlf"])
def test_newline_styles_have_round_tripping_byte_spans(newline: str):
    source = newline.join(["name = '\u732b'", "answer = name is None", ""])
    for record in candidates(invoke(source)):
        raw = source.encode()
        span = record["span"]
        assert raw[span["start"] : span["start"] + span["length"]].decode() == record["original"]


def test_invalid_syntax_emits_exact_diagnostic_code_and_summary():
    events = invoke("def broken(:\n    pass\n")
    assert [event["code"] for event in events if event["kind"] == "diagnostic"] == [
        "invalid_syntax"
    ]
    assert events[-1] == {
        "kind": "summary",
        "effect_id": "effect-7",
        "candidate_count": 0,
        "diagnostic_count": 1,
        "truncated": False,
    }


def test_non_normalized_path_emits_invalid_request():
    events = invoke("value = True\n", path="pkg//sample.py")
    assert [event["code"] for event in events if event["kind"] == "diagnostic"] == [
        "invalid_request"
    ]


def test_supported_python_syntax_is_analyzed():
    source = (
        "type Pair[T] = tuple[T, T]\n"
        "def compare[T](left: T, right: T) -> bool:\n"
        "    return left != right\n"
    )
    records = candidates(invoke(source))
    assert [(r["original"], r["replacement"]) for r in records] == [("!=", "==")]


def test_candidate_limit_is_enforced_without_emitting_unbounded_records():
    events = invoke("value = True + False == True\n", max_candidates=2)
    assert len(candidates(events)) == 2
    assert events[-1] == {
        "kind": "summary",
        "effect_id": "effect-7",
        "candidate_count": 2,
        "diagnostic_count": 1,
        "truncated": True,
    }
    assert [e["code"] for e in events if e["kind"] == "diagnostic"] == ["candidate_limit_exceeded"]


def test_effect_id_is_echoed_on_every_record_and_order_is_deterministic():
    source = "result = True and False == (1 + 2)\n"
    first = invoke(source, effect_id="e-99")
    second = invoke(source, effect_id="e-99")
    assert first == second
    assert all(record["effect_id"] == "e-99" for record in first)
    starts = [r["span"]["start"] for r in candidates(first)]
    assert starts == sorted(starts)


@pytest.mark.parametrize("constant", ["NaN", "Infinity", "-Infinity"])
def test_nonstandard_json_constants_emit_bounded_invalid_request(constant: str):
    events = invoke_raw('{"effect_id":' + constant + ',"path":"pkg/a.py","module":"x = 1\\n"}')
    assert [event["kind"] for event in events] == ["diagnostic", "summary"]
    assert events[0]["code"] == "invalid_request"
    assert events[0]["effect_id"] == "unknown"


def test_oversized_json_integer_emits_bounded_invalid_request():
    events = invoke_raw('{"effect_id":' + "9" * 5000 + "}")
    assert [event["kind"] for event in events] == ["diagnostic", "summary"]
    assert events[0]["code"] == "invalid_request"


def test_lone_surrogate_emits_bounded_ascii_safe_invalid_request():
    payload = json.dumps(
        {"effect_id": "e", "path": "pkg/a.py", "module": "\ud800"},
        ensure_ascii=True,
    )
    events = invoke_raw(payload)
    assert [event["kind"] for event in events] == ["diagnostic", "summary"]
    assert events[0]["code"] == "invalid_request"
