from __future__ import annotations  # noqa: D100, INP001

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
    MetadataWrapper,
    PositionProvider,
)


@dataclass(frozen=True)
class AnalyzerRequest:  # noqa: D101
    effect_id: object
    path: str
    module: str
    lines: tuple[tuple[int, int], ...]
    symbols: tuple[str, ...]
    max_candidates: int


class RequestError(ValueError):  # noqa: D101
    pass


class CandidateLimitReached(Exception):  # noqa: D101, N818
    pass


def reject_json_constant(value: str) -> None:  # noqa: D103
    raise RequestError(f"non-standard JSON constant: {value}")  # noqa: EM102, TRY003


def write_jsonl(record: dict[str, object]) -> None:  # noqa: D103
    sys.stdout.write(
        json.dumps(record, ensure_ascii=True, allow_nan=False, separators=(",", ":")) + "\n"
    )
    sys.stdout.flush()


def parse_request(value: Any) -> AnalyzerRequest:  # noqa: ANN401, C901, D103
    if not isinstance(value, dict):
        raise RequestError("request must be an object")  # noqa: EM101, TRY003
    try:
        effect_id = value["effect_id"]
        path = value["path"]
        module = value["module"]
    except KeyError as error:
        raise RequestError(f"missing field: {error.args[0]}") from error  # noqa: EM102, TRY003
    if (
        not isinstance(effect_id, (str, int))
        or isinstance(effect_id, bool)
        or (isinstance(effect_id, str) and not _is_utf8(effect_id))
    ):
        raise RequestError("effect_id must be a string or integer")  # noqa: EM101, TRY003
    if not isinstance(path, str) or not _is_utf8(path) or not _normalized_path(path):
        raise RequestError("path must be a normalized relative POSIX path")  # noqa: EM101, TRY003
    if not isinstance(module, str) or not _is_utf8(module):
        raise RequestError("module must be source text")  # noqa: EM101, TRY003

    raw_lines = value.get("lines", [])
    if not isinstance(raw_lines, list):
        raise RequestError("lines must be an array")  # noqa: EM101, TRY003
    lines: list[tuple[int, int]] = []
    for item in raw_lines:
        if (
            not isinstance(item, list)
            or len(item) != 2  # noqa: PLR2004
            or not all(isinstance(part, int) and not isinstance(part, bool) for part in item)
            or item[0] < 1
            or item[0] > item[1]
        ):
            raise RequestError("line ranges must be [positive_start, end] pairs")  # noqa: EM101, TRY003
        lines.append((item[0], item[1]))

    raw_symbols = value.get("symbols", [])
    if not isinstance(raw_symbols, list) or not all(
        isinstance(symbol, str) and _is_utf8(symbol) and ":" in symbol for symbol in raw_symbols
    ):
        raise RequestError("symbols must contain MODULE:QUALNAME strings")  # noqa: EM101, TRY003
    max_candidates = value.get("max_candidates", 10_000)
    if (
        not isinstance(max_candidates, int)
        or isinstance(max_candidates, bool)
        or max_candidates < 1
    ):
        raise RequestError("max_candidates must be a positive integer")  # noqa: EM101, TRY003
    return AnalyzerRequest(
        effect_id=effect_id,
        path=path,
        module=module,
        lines=tuple(lines),
        symbols=tuple(raw_symbols),
        max_candidates=max_candidates,
    )


def _is_utf8(value: str) -> bool:
    try:
        value.encode("utf-8")
    except UnicodeEncodeError:
        return False
    return True


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


def _replacement_like(node: cst.CSTNode, replacement_type: type[cst.CSTNode]) -> cst.CSTNode:
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


class MutationVisitor(cst.CSTVisitor):  # noqa: D101
    METADATA_DEPENDENCIES = (PositionProvider, ByteSpanPositionProvider)

    def __init__(self, request: AnalyzerRequest, module: cst.Module) -> None:  # noqa: D107
        self.request = request
        self.module = module
        self.source_bytes = request.module.encode("utf-8")
        self.module_name = _module_name(request.path)
        self.scope: list[str] = []
        self.location_count = 0
        self.candidate_count = 0
        self.diagnostic_count = 0

    def on_visit(self, node: cst.CSTNode) -> bool:  # noqa: D102
        if isinstance(node, (cst.ClassDef, cst.FunctionDef)):
            self.scope.append(node.name.value)

        replacement_info = REPLACEMENTS.get(type(node))
        if replacement_info is not None:
            replacement_type, operator, _, _ = replacement_info
            self.emit_candidate(
                node,
                _replacement_like(node, replacement_type),
                operator,
            )
        elif isinstance(node, cst.UnaryOperation) and isinstance(node.operator, cst.Not):
            self.emit_candidate(
                node,
                node.expression,
                "remove_not",
            )
        elif isinstance(node, cst.Name) and node.value in ("True", "False"):
            replacement = node.with_changes(value="False" if node.value == "True" else "True")
            self.emit_candidate(node, replacement, "boolean_literal")
        return True

    def on_leave(self, original_node: cst.CSTNode) -> None:  # noqa: D102
        if isinstance(original_node, (cst.ClassDef, cst.FunctionDef)):
            self.scope.pop()

    def emit_candidate(  # noqa: D102
        self,
        node: cst.CSTNode,
        replacement: cst.CSTNode,
        operator: str,
    ) -> None:
        position = self.get_metadata(PositionProvider, node)
        symbol = ".".join(self.scope) or None
        if not self._selected(position.start, symbol):
            return
        if self.location_count >= self.request.max_candidates:
            raise CandidateLimitReached
        self.location_count += 1

        byte_span = self.get_metadata(ByteSpanPositionProvider, node)
        span_end = byte_span.start + byte_span.length
        try:
            original = self.source_bytes[byte_span.start : span_end].decode("utf-8")
        except UnicodeDecodeError:
            self.emit_diagnostic("unreconstructable_span", position.start)
            return
        try:
            mutated_from_cst = self.module.deep_replace(node, replacement).code
            mutated_bytes = mutated_from_cst.encode("utf-8")
            prefix = self.source_bytes[: byte_span.start]
            suffix = self.source_bytes[span_end:]
            if (
                not mutated_bytes.startswith(prefix)
                or not mutated_bytes.endswith(suffix)
                or len(mutated_bytes) < len(prefix) + len(suffix)
            ):
                raise ValueError("replacement does not reconstruct from its span")  # noqa: EM101, TRY003, TRY301
            replacement_end = len(mutated_bytes) - len(suffix) if suffix else None
            replacement_text = mutated_bytes[len(prefix) : replacement_end].decode("utf-8")
            cst.parse_module(mutated_from_cst)
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
                "original": original,
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
        line_selected = any(start <= position.line <= end for start, end in self.request.lines)
        symbol_selected = any(
            self._symbol_matches(selector, symbol) for selector in self.request.symbols
        )
        return line_selected or symbol_selected

    def _symbol_matches(self, selector: str, symbol: str | None) -> bool:
        selected_module, selected_qualname = selector.rsplit(":", 1)
        module_matches = self.module_name == selected_module or self.module_name.endswith(
            "." + selected_module
        )
        return bool(
            module_matches
            and symbol
            and (symbol == selected_qualname or symbol.startswith(selected_qualname + "."))
        )

    def emit_diagnostic(self, code: str, position: CodePosition | None = None) -> None:  # noqa: D102
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


def analyze(request: AnalyzerRequest) -> None:  # noqa: D103
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


def main() -> int:  # noqa: D103
    effect_id: object = "unknown"
    try:
        lines = [line for line in sys.stdin if line.strip()]
        if len(lines) != 1:
            raise RequestError("exactly one JSONL request is required")  # noqa: EM101, TRY003
        value = json.loads(lines[0], parse_constant=reject_json_constant)
        if isinstance(value, dict):
            candidate_effect_id = value.get("effect_id", effect_id)
            if (
                isinstance(candidate_effect_id, int) and not isinstance(candidate_effect_id, bool)
            ) or (isinstance(candidate_effect_id, str) and _is_utf8(candidate_effect_id)):
                effect_id = candidate_effect_id
        request = parse_request(value)
    except (ValueError, UnicodeError) as error:
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
