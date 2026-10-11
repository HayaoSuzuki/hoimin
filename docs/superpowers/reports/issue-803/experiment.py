"""One bounded Linux corpus experiment; no cache replacement or product API."""
# ruff: noqa: INP001 -- Standalone research script, not an importable package.

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from tools.ci_fuzz import Campaign

ROOT = Path(__file__).resolve().parents[4]
MAX_FILES = 50_000
MAX_BYTES = 64 * 1024 * 1024
MAX_INPUT = 4096
HEADER_LINES = 2


def record_values(line: str, marker: str, index: int) -> list[int]:
    fields = line.split()
    if fields[:HEADER_LINES] != [marker, str(index)]:
        msg = "incomplete or incompatible merge control"
        raise ValueError(msg)
    values = [int(value) for value in fields[HEADER_LINES:]]
    if any(value < 0 for value in values):
        msg = "negative control value"
        raise ValueError(msg)
    if marker == "STARTED" and (len(values) != 1 or values[0] > MAX_INPUT):
        msg = "invalid input size in control"
        raise ValueError(msg)
    return values


def read_features(control: Path) -> tuple[set[int], set[int]]:
    """Read the pinned libFuzzer merge format, rejecting incomplete runs."""
    if control.stat().st_size > MAX_BYTES:
        msg = "control exceeds experiment limit"
        raise ValueError(msg)
    text = control.read_text(encoding="utf-8")
    lines = text.splitlines()
    if not text.endswith("\n") or len(lines) < HEADER_LINES:
        msg = "incomplete merge control"
        raise ValueError(msg)
    count = int(lines[0])
    if not 0 < count <= MAX_FILES or not 0 <= int(lines[1]) <= count:
        msg = "invalid control header"
        raise ValueError(msg)
    records = lines[HEADER_LINES + count :]
    if len(records) != 3 * count:
        msg = "incomplete merge control"
        raise ValueError(msg)
    features: set[int] = set()
    coverage: set[int] = set()
    for index in range(count):
        for offset, marker in enumerate(("STARTED", "FT", "COV")):
            values = record_values(records[3 * index + offset], marker, index)
            if marker == "FT":
                features.update(values)
            elif marker == "COV":
                coverage.update(values)
    return features, coverage


def compare_features(
    original: tuple[set[int], set[int]], compact: tuple[set[int], set[int]]
) -> dict[str, object]:
    return {
        "preserved": original == compact,
        "missing_features": sorted(original[0] - compact[0]),
        "added_features": sorted(compact[0] - original[0]),
        "missing_pcs": sorted(original[1] - compact[1]),
        "added_pcs": sorted(compact[1] - original[1]),
    }


def inventory(directory: Path) -> dict[str, object]:
    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or not path.is_file():
            msg = "snapshot must contain only regular inputs"
            raise ValueError(msg)
        size = path.stat().st_size
        count += 1
        total += size
        if size > MAX_INPUT or count > MAX_FILES or total > MAX_BYTES:
            msg = "snapshot exceeds experiment limits"
            raise ValueError(msg)
        digest.update(path.name.encode())
        digest.update(b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    if not count:
        msg = "empty snapshot"
        raise ValueError(msg)
    return {"files": count, "bytes": total, "manifest_sha256": digest.hexdigest()}


def run(deadline: float, report: Path) -> None:
    campaign = Campaign(deadline, report, seed=1)
    source = ROOT / "fuzz/corpus/python_analyzer"
    seeds = ROOT / "fuzz/seeds/python_analyzer"
    original = inventory(source)
    managed = inventory(seeds)
    if int(original["files"]) + int(managed["files"]) > MAX_FILES:
        msg = "combined snapshot exceeds experiment limits"
        raise ValueError(msg)
    if shutil.disk_usage(ROOT).free < 8 * 1024**3:
        msg = "insufficient free disk for experiment"
        raise ValueError(msg)
    campaign.run(
        "build",
        [
            "cargo",
            "+nightly-2026-10-08",
            "fuzz",
            "build",
            "python_analyzer",
            "--codegen-units",
            "16",
        ],
        reserve=120,
    )
    binary = ROOT / "fuzz/target/x86_64-unknown-linux-gnu/release/python_analyzer"
    common = [
        "prlimit",
        "--fsize=67108864",
        "--",
        str(binary),
        "-seed=1",
        "-max_len=4096",
        "-timeout=5",
        "-rss_limit_mb=1024",
    ]
    compact = report / "compact"
    compact.mkdir()
    durations: dict[str, list[float]] = {"before": [], "after": []}
    for label, corpus in (("before", source), ("after", compact)):
        for iteration in range(3):
            output = report / f"{label}-replay-{iteration}"
            output.mkdir()
            campaign.run(
                f"{label}-replay-{iteration}",
                [*common, "-runs=0", str(output), str(corpus), str(seeds)],
                timeout=120,
                reserve=30,
            )
            durations[label].append(float(campaign.stages[-1]["elapsed_seconds"]))
        control = report / f"{label}.control"
        output = compact if label == "before" else report / "verification-output"
        output.mkdir(exist_ok=True)
        campaign.run(
            f"{label}-merge",
            [
                *common,
                "-merge=1",
                f"-merge_control_file={control}",
                str(output),
                str(corpus),
                str(seeds),
            ],
            timeout=180,
            reserve=30,
        )
    before = read_features(report / "before.control")
    after = read_features(report / "after.control")
    if not before[0] or not before[1]:
        msg = "empty feature measurement"
        raise ValueError(msg)
    comparison = compare_features(before, after)
    assert inventory(source) == original, "original corpus changed"
    assert inventory(seeds) == managed, "managed seeds changed"
    evidence = {
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "original": original,
        "managed_seeds": managed,
        "compact": inventory(compact),
        "replay_seconds": durations,
        "features_before": len(before[0]),
        "features_after": len(after[0]),
        "pcs_before": len(before[1]),
        "pcs_after": len(after[1]),
        "comparison": comparison,
    }
    for label, measured in (("before", before), ("after", after)):
        contents = json.dumps([sorted(measured[0]), sorted(measured[1])])
        (report / f"{label}-feature-ids.json").write_text(contents, encoding="utf-8")
        evidence[f"{label}_feature_ids_sha256"] = hashlib.sha256(
            contents.encode()
        ).hexdigest()
    (report / "evidence.json").write_text(
        json.dumps(evidence, indent=2) + "\n", encoding="utf-8"
    )
    campaign.save("success" if comparison["preserved"] else "feature_mismatch")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deadline", type=float, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    run(args.deadline, args.report)
