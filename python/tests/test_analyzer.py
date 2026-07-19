from __future__ import annotations

import importlib.util
import io
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

import libcst as cst
import pytest

if TYPE_CHECKING:
    from types import ModuleType

ROOT = Path(__file__).parents[2]
ANALYZER = ROOT / "python" / "hoimin_analyzer.py"
FIXTURES = Path(__file__).parent / "fixtures"


def load_analyzer() -> ModuleType:
    spec = importlib.util.spec_from_file_location("hoimin_analyzer_direct", ANALYZER)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


analyzer = load_analyzer()


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


@pytest.mark.parametrize(
    ("path", "expected"),
    [
        ("pkg/sample.py", True),
        ("", False),
        ("pkg\\sample.py", False),
        ("/pkg/sample.py", False),
        ("../sample.py", False),
        ("C:/sample.py", False),
    ],
)
def test_normalized_path_contract(path: str, expected: object) -> None:
    assert analyzer._normalized_path(path) is expected  # noqa: SLF001 -- Directly exercises the validation contract.


@pytest.mark.parametrize(
    ("value", "message"),
    [
        (None, "request must be an object"),
        ({}, "missing field: effect_id"),
        (
            {"effect_id": True, "path": "pkg/sample.py", "module": "pass"},
            "effect_id must be a string or integer",
        ),
        (
            {"effect_id": "e", "path": "pkg/sample.py", "module": "pass", "lines": "1"},
            "lines must be an array",
        ),
        (
            {
                "effect_id": "e",
                "path": "pkg/sample.py",
                "module": "pass",
                "lines": [[True, 2]],
            },
            "line ranges must be [positive_start, end] pairs",
        ),
        (
            {
                "effect_id": "e",
                "path": "pkg/sample.py",
                "module": "pass",
                "lines": [[2, 1]],
            },
            "line ranges must be [positive_start, end] pairs",
        ),
        (
            {
                "effect_id": "e",
                "path": "pkg/sample.py",
                "module": "pass",
                "symbols": ["pkg.sample"],
            },
            "symbols must contain MODULE:QUALNAME strings",
        ),
        (
            {
                "effect_id": "e",
                "path": "pkg/sample.py",
                "module": "pass",
                "max_candidates": 0,
            },
            "max_candidates must be a positive integer",
        ),
        (
            {"effect_id": "e", "path": "pkg\\sample.py", "module": "pass"},
            "path must be a normalized relative POSIX path",
        ),
        (
            {"effect_id": "e", "path": "pkg/sample.py", "module": "\ud800"},
            "module must be source text",
        ),
    ],
)
def test_parse_request_rejects_invalid_protocol_values(value: Any, message: str) -> None:
    with pytest.raises(analyzer.RequestError, match=re.escape(message)):
        analyzer.parse_request(value)


def test_parse_request_returns_immutable_normalized_request() -> None:
    request = analyzer.parse_request(
        {
            "effect_id": 7,
            "path": "pkg/__init__.py",
            "module": "value = True\n",
            "lines": [[1, 2]],
            "symbols": ["pkg:method"],
            "max_candidates": 3,
        }
    )
    assert request == analyzer.AnalyzerRequest(
        7,
        "pkg/__init__.py",
        "value = True\n",
        ((1, 2),),
        ("pkg:method",),
        3,
    )
    assert analyzer._module_name(request.path) == "pkg"  # noqa: SLF001 -- Directly exercises the path-to-module helper.


def test_emit_candidate_reports_an_unreconstructable_utf8_span(
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = "value = True\n"
    request = analyzer.parse_request({"effect_id": "e", "path": "pkg/sample.py", "module": source})
    module = cst.parse_module(source)
    visitor = analyzer.MutationVisitor(request, module)
    visitor.source_bytes = b"value = \xff\xff\xff\xff\n"

    cst.MetadataWrapper(module, unsafe_skip_copy=True).visit(visitor)

    assert [json.loads(line) for line in capsys.readouterr().out.splitlines()] == [
        {
            "kind": "diagnostic",
            "effect_id": "e",
            "code": "unreconstructable_span",
            "path": "pkg/sample.py",
            "line": 1,
            "column": 8,
        }
    ]


def test_emit_candidate_reports_an_unparseable_replacement(
    capsys: pytest.CaptureFixture[str],
) -> None:
    class InvalidReplacementModule:
        def deep_replace(
            self, node: cst.CSTNode, replacement: cst.CSTNode
        ) -> InvalidReplacementModule:
            del node, replacement
            return self

        @property
        def code(self) -> str:
            return "value = (\n"

    source = "value = True\n"
    request = analyzer.parse_request({"effect_id": "e", "path": "pkg/sample.py", "module": source})
    module = cst.parse_module(source)
    visitor = analyzer.MutationVisitor(request, InvalidReplacementModule())

    cst.MetadataWrapper(module, unsafe_skip_copy=True).visit(visitor)

    assert [json.loads(line) for line in capsys.readouterr().out.splitlines()] == [
        {
            "kind": "diagnostic",
            "effect_id": "e",
            "code": "unparseable_replacement",
            "path": "pkg/sample.py",
            "line": 1,
            "column": 8,
        }
    ]


def test_main_rejects_empty_or_multiple_jsonl_requests(monkeypatch: pytest.MonkeyPatch) -> None:
    stdout = io.StringIO()
    monkeypatch.setattr(analyzer.sys, "stdin", io.StringIO("\n\n"))
    monkeypatch.setattr(analyzer.sys, "stdout", stdout)

    assert analyzer.main() == 0
    assert [json.loads(line) for line in stdout.getvalue().splitlines()] == [
        {
            "kind": "diagnostic",
            "effect_id": "unknown",
            "code": "invalid_request",
            "message": "exactly one JSONL request is required",
        },
        {
            "kind": "summary",
            "effect_id": "unknown",
            "candidate_count": 0,
            "diagnostic_count": 1,
            "truncated": False,
        },
    ]


def test_emit_candidate_reports_a_nonlocal_replacement(capsys: pytest.CaptureFixture[str]) -> None:
    class NonlocalReplacementModule:
        def deep_replace(
            self, node: cst.CSTNode, replacement: cst.CSTNode
        ) -> NonlocalReplacementModule:
            del node, replacement
            return self

        @property
        def code(self) -> str:
            return "other = False\n"

    source = "value = True\n"
    request = analyzer.parse_request({"effect_id": "e", "path": "pkg/sample.py", "module": source})
    module = cst.parse_module(source)
    visitor = analyzer.MutationVisitor(request, NonlocalReplacementModule())

    cst.MetadataWrapper(module, unsafe_skip_copy=True).visit(visitor)

    assert json.loads(capsys.readouterr().out)["code"] == "unparseable_replacement"


def test_main_rejects_two_valid_jsonl_requests(monkeypatch: pytest.MonkeyPatch) -> None:
    request = json.dumps({"effect_id": "effect-9", "path": "pkg/sample.py", "module": "value = 1\n"})
    stdout = io.StringIO()
    monkeypatch.setattr(analyzer.sys, "stdin", io.StringIO(f"{request}\n{request}\n"))
    monkeypatch.setattr(analyzer.sys, "stdout", stdout)

    assert analyzer.main() == 0
    assert [json.loads(line) for line in stdout.getvalue().splitlines()] == [
        {
            "kind": "diagnostic",
            "effect_id": "unknown",
            "code": "invalid_request",
            "message": "exactly one JSONL request is required",
        },
        {
            "kind": "summary",
            "effect_id": "unknown",
            "candidate_count": 0,
            "diagnostic_count": 1,
            "truncated": False,
        },
    ]
