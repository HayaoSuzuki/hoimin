from __future__ import annotations

import dataclasses
import json
import sys
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import Any

import libcst as cst
from libcst.metadata import (
    ByteSpanPositionProvider,
    CodePosition,
    CodeSpan,
    MetadataWrapper,
    PositionProvider,
)


@dataclass(frozen=True)
class AnalyzerRequest:
    effect_id: object
    path: str
    module: str
    lines: tuple[tuple[int, int], ...]
    symbols: tuple[str, ...]
    max_candidates: int


class RequestError(ValueError):
    pass


class CandidateLimitReached(Exception):
    pass


def write_jsonl(record: dict[str, object]) -> None:
    sys.stdout.write(
        json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n"
    )
    sys.stdout.flush()


def parse_request(value: Any) -> AnalyzerRequest:
    if not isinstance(value, dict):
        raise RequestError("request must be an object")
    try:
        effect_id = value["effect_id"]
        path = value["path"]
        module = value["module"]
    except KeyError as error:
        raise RequestError(f"missing field: {error.args[0]}") from error
    if not isinstance(effect_id, (str, int)) or isinstance(effect_id, bool):
        raise RequestError("effect_id must be a string or integer")
    if not isinstance(path, str) or not _normalized_path(path):
        raise RequestError("path must be a normalized relative POSIX path")
    if not isinstance(module, str):
        raise RequestError("module must be source text")

    raw_lines = value.get("lines", [])
    if not isinstance(raw_lines, list):
        raise RequestError("lines must be an array")
    lines: list[tuple[int, int]] = []
    for item in raw_lines:
        if (
            not isinstance(item, list)
            or len(item) != 2
            or not all(
                isinstance(part, int) and not isinstance(part, bool) for part in item
            )
            or item[0] < 1
            or item[0] > item[1]
        ):
            raise RequestError("line ranges must be [positive_start, end] pairs")
        lines.append((item[0], item[1]))

    raw_symbols = value.get("symbols", [])
    if not isinstance(raw_symbols, list) or not all(
        isinstance(symbol, str) and ":" in symbol for symbol in raw_symbols
    ):
        raise RequestError("symbols must contain MODULE:QUALNAME strings")
    max_candidates = value.get("max_candidates", 10_000)
    if (
        not isinstance(max_candidates, int)
        or isinstance(max_candidates, bool)
        or max_candidates < 1
    ):
        raise RequestError("max_candidates must be a positive integer")
    return AnalyzerRequest(
        effect_id=effect_id,
        path=path,
        module=module,
        lines=tuple(lines),
        symbols=tuple(raw_symbols),
        max_candidates=max_candidates,
    )


def _normalized_path(path: str) -> bool:
    if not path or "\\" in path or path.startswith("/"):
        return False
    pure = PurePosixPath(path)
    return (
        path == pure.as_posix()
        and ":" not in pure.parts[0]
        and all(part not in ("", ".", "..") for part in pure.parts)
    )


def _module_name(path: str) -> str:
    parts = list(PurePosixPath(path).with_suffix("").parts)
    if parts and parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


def _replacement_like(
    node: cst.CSTNode, replacement_type: type[cst.CSTNode]
) -> cst.CSTNode:
    replacement_fields = {field.name for field in dataclasses.fields(replacement_type)}
    values = {
        field.name: getattr(node, field.name)
        for field in dataclasses.fields(node)
        if field.name in replacement_fields
    }
    return replacement_type(**values)


REPLACEMENTS: dict[type[cst.CSTNode], tuple[type[cst.CSTNode], str, str, str]] = {
    cst.Equal: (cst.NotEqual, "compare_eq_ne", "==", "!="),
    cst.NotEqual: (cst.Equal, "compare_eq_ne", "!=", "=="),
    cst.LessThan: (cst.LessThanEqual, "compare_order", "<", "<="),
    cst.LessThanEqual: (cst.LessThan, "compare_order", "<=", "<"),
    cst.GreaterThan: (cst.GreaterThanEqual, "compare_order", ">", ">="),
    cst.GreaterThanEqual: (cst.GreaterThan, "compare_order", ">=", ">"),
    cst.In: (cst.NotIn, "membership", "in", "not in"),
    cst.NotIn: (cst.In, "membership", "not in", "in"),
    cst.Is: (cst.IsNot, "identity", "is", "is not"),
    cst.IsNot: (cst.Is, "identity", "is not", "is"),
    cst.And: (cst.Or, "boolean_and_or", "and", "or"),
    cst.Or: (cst.And, "boolean_and_or", "or", "and"),
    cst.Add: (cst.Subtract, "binary_add_sub", "+", "-"),
    cst.Subtract: (cst.Add, "binary_add_sub", "-", "+"),
    cst.AddAssign: (cst.SubtractAssign, "augmented_add_sub", "+=", "-="),
    cst.SubtractAssign: (cst.AddAssign, "augmented_add_sub", "-=", "+="),
    cst.Multiply: (cst.Divide, "binary_mul_div", "*", "/"),
    cst.Divide: (cst.Multiply, "binary_mul_div", "/", "*"),
    cst.FloorDivide: (cst.Modulo, "binary_floor_mod", "//", "%"),
    cst.Modulo: (cst.FloorDivide, "binary_floor_mod", "%", "//"),
    cst.Plus: (cst.Minus, "unary_sign", "+", "-"),
    cst.Minus: (cst.Plus, "unary_sign", "-", "+"),
    cst.Break: (cst.Continue, "break_continue", "break", "continue"),
    cst.Continue: (cst.Break, "break_continue", "continue", "break"),
}


class MutationVisitor(cst.CSTVisitor):
    METADATA_DEPENDENCIES = (PositionProvider, ByteSpanPositionProvider)

    def __init__(self, request: AnalyzerRequest, module: cst.Module) -> None:
        self.request = request
        self.module = module
        self.source_bytes = request.module.encode("utf-8")
        self.module_name = _module_name(request.path)
        self.scope: list[str] = []
        self.location_count = 0
        self.candidate_count = 0
        self.diagnostic_count = 0

    def on_visit(self, node: cst.CSTNode) -> bool:
        if isinstance(node, (cst.ClassDef, cst.FunctionDef)):
            self.scope.append(node.name.value)

        replacement_info = REPLACEMENTS.get(type(node))
        if replacement_info is not None:
            replacement_type, operator, original, replacement_text = replacement_info
            self.emit_candidate(
                node,
                _replacement_like(node, replacement_type),
                operator,
                original,
                replacement_text,
            )
        elif isinstance(node, cst.UnaryOperation) and isinstance(
            node.operator, cst.Not
        ):
            self.emit_candidate(
                node,
                node.expression,
                "remove_not",
                self.module.code_for_node(node),
                self.module.code_for_node(node.expression),
            )
        elif isinstance(node, cst.Name) and node.value in ("True", "False"):
            replacement = node.with_changes(
                value="False" if node.value == "True" else "True"
            )
            self.emit_candidate(
                node, replacement, "boolean_literal", node.value, replacement.value
            )
        return True

    def on_leave(self, original_node: cst.CSTNode) -> None:
        if isinstance(original_node, (cst.ClassDef, cst.FunctionDef)):
            self.scope.pop()

    def emit_candidate(
        self,
        node: cst.CSTNode,
        replacement: cst.CSTNode,
        operator: str,
        expected_original: str,
        replacement_text: str,
    ) -> None:
        position = self.get_metadata(PositionProvider, node)
        symbol = ".".join(self.scope) or None
        if not self._selected(position.start, symbol):
            return
        if self.location_count >= self.request.max_candidates:
            raise CandidateLimitReached
        self.location_count += 1

        byte_span = self.get_metadata(ByteSpanPositionProvider, node)
        if not self._span_reconstructs(byte_span, expected_original):
            self.emit_diagnostic("unreconstructable_span", position.start)
            return
        try:
            mutated_from_cst = self.module.deep_replace(node, replacement).code
            mutated_bytes = (
                self.source_bytes[: byte_span.start]
                + replacement_text.encode("utf-8")
                + self.source_bytes[byte_span.start + byte_span.length :]
            )
            mutated = mutated_bytes.decode("utf-8")
            if mutated_from_cst != mutated:
                raise ValueError("replacement does not reconstruct from its span")
            cst.parse_module(mutated)
        except (
            UnicodeDecodeError,
            ValueError,
            cst.ParserSyntaxError,
            cst.CSTValidationError,
        ):
            self.emit_diagnostic("unparseable_replacement", position.start)
            return

        write_jsonl(
            {
                "kind": "candidate",
                "effect_id": self.request.effect_id,
                "path": self.request.path,
                "span": {"start": byte_span.start, "length": byte_span.length},
                "original": expected_original,
                "replacement": replacement_text,
                "operator": operator,
                "line": position.start.line,
                "column": position.start.column,
                "symbol": symbol,
            }
        )
        self.candidate_count += 1

    def _selected(self, position: CodePosition, symbol: str | None) -> bool:
        if not self.request.lines and not self.request.symbols:
            return True
        line_selected = any(
            start <= position.line <= end for start, end in self.request.lines
        )
        symbol_selected = any(
            self._symbol_matches(selector, symbol) for selector in self.request.symbols
        )
        return line_selected or symbol_selected

    def _symbol_matches(self, selector: str, symbol: str | None) -> bool:
        selected_module, selected_qualname = selector.rsplit(":", 1)
        module_matches = (
            self.module_name == selected_module
            or self.module_name.endswith("." + selected_module)
        )
        return bool(
            module_matches
            and symbol
            and (
                symbol == selected_qualname
                or symbol.startswith(selected_qualname + ".")
            )
        )

    def _span_reconstructs(self, span: CodeSpan, original: str) -> bool:
        end = span.start + span.length
        return self.source_bytes[span.start : end] == original.encode("utf-8")

    def emit_diagnostic(self, code: str, position: CodePosition | None = None) -> None:
        record: dict[str, object] = {
            "kind": "diagnostic",
            "effect_id": self.request.effect_id,
            "code": code,
            "path": self.request.path,
        }
        if position is not None:
            record.update(line=position.line, column=position.column)
        write_jsonl(record)
        self.diagnostic_count += 1


def analyze(request: AnalyzerRequest) -> None:
    candidate_count = 0
    diagnostic_count = 0
    truncated = False
    try:
        module = cst.parse_module(request.module)
    except (cst.ParserSyntaxError, cst.CSTValidationError):
        write_jsonl(
            {
                "kind": "diagnostic",
                "effect_id": request.effect_id,
                "code": "invalid_syntax",
                "path": request.path,
            }
        )
        diagnostic_count = 1
    else:
        wrapper = MetadataWrapper(module, unsafe_skip_copy=True)
        visitor = MutationVisitor(request, module)
        try:
            wrapper.visit(visitor)
        except CandidateLimitReached:
            visitor.emit_diagnostic("candidate_limit_exceeded")
            truncated = True
        candidate_count = visitor.candidate_count
        diagnostic_count = visitor.diagnostic_count
        del visitor, wrapper, module

    write_jsonl(
        {
            "kind": "summary",
            "effect_id": request.effect_id,
            "candidate_count": candidate_count,
            "diagnostic_count": diagnostic_count,
            "truncated": truncated,
        }
    )


def main() -> int:
    lines = [line for line in sys.stdin if line.strip()]
    effect_id: object = "unknown"
    try:
        if len(lines) != 1:
            raise RequestError("exactly one JSONL request is required")
        value = json.loads(lines[0])
        if isinstance(value, dict):
            effect_id = value.get("effect_id", effect_id)
        request = parse_request(value)
    except (json.JSONDecodeError, RequestError) as error:
        write_jsonl(
            {
                "kind": "diagnostic",
                "effect_id": effect_id,
                "code": "invalid_request",
                "message": str(error),
            }
        )
        write_jsonl(
            {
                "kind": "summary",
                "effect_id": effect_id,
                "candidate_count": 0,
                "diagnostic_count": 1,
                "truncated": False,
            }
        )
        return 0
    analyze(request)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
