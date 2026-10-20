"""Run serial paired release probes after builds/tests have stopped."""
import argparse
import hashlib
import json
import pathlib
import platform
import statistics
import subprocess
import tempfile


def invoke(command, accepted=(0,)):
    result = subprocess.run(command, capture_output=True, text=True, timeout=60)
    if result.returncode not in accepted:
        raise RuntimeError((command, result.returncode, result.stderr))
    return json.loads(result.stdout)


def sha256(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--cli-before", required=True)
    parser.add_argument("--cli-after", required=True)
    args = parser.parse_args()
    binaries = {"before": args.before, "after": args.after}
    cli_binaries = {"before": args.cli_before, "after": args.cli_after}

    contracts = {revision: invoke([binary, "contracts"]) for revision, binary in binaries.items()}
    assert contracts["before"] == contracts["after"], "public API contract mismatch"

    samples = []
    workloads = [
        (8, 1, 0),
        (8, 1, 2),
        (1000, 1, 0),
        (32, 1000, 0),
        (1000, 1000, 0),
        (2000, 2000, 0),
        (1000, 1000, 2),
        (64, 256, 16),
    ]
    for ordinary, aliases, classes in workloads:
        for sample in range(args.samples):
            revisions = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
            for revision in revisions:
                row = invoke([binaries[revision], str(ordinary), str(aliases), str(classes)])
                row.update(revision=revision, sample=sample)
                samples.append(row)
        paired = [row for row in samples if (row["ordinary"], row["aliases"], row["classes"]) == (ordinary, aliases, classes)]
        assert len({row["source_digest"] for row in paired}) == 1
        assert len({row["output_digest"] for row in paired}) == 1
        assert len(paired[0]["output"]["candidates"]) == max(0, classes - 1)
        print(f"measured ordinary={ordinary} aliases={aliases} classes={classes}", flush=True)

    # Preserve expensive samples if a later contract check reveals a harness error.
    args.output.parent.mkdir(parents=True, exist_ok=True)
    sample_checkpoint = args.output.with_suffix(".samples.json")
    sample_checkpoint.write_text(json.dumps(samples, indent=2) + "\n")
    cli = []
    with tempfile.TemporaryDirectory(prefix="hoimin-visibility-") as directory:
        root = pathlib.Path(directory)
        (root / "pkg").mkdir()
        (root / "pkg/__init__.py").write_text("class Root(Exception): pass\n")
        (root / "pkg/errors.py").write_text("from pkg import Root\nclass Child(Root): pass\nclass Other(Root): pass\n")
        for scenario, source in [
            ("loaded", "import pkg as p\nimport pkg.errors as zz\nfrom pkg import Root\ndef target():\n    raise Root()\n"),
            ("unloaded", "import pkg as p\nfrom pkg import Root\ndef target():\n    raise Root()\n"),
            ("shadowed", "import pkg as p\nimport pkg.errors as zz\nfrom pkg import Root\np = 0\ndef target():\n    raise Root()\n"),
            ("diagnostic", "import pkg as p\nimport pkg.errors as zz\nfrom pkg import Root\ndef target():\n    raise Root()\n    raise missing()\n"),
        ]:
            (root / "service.py").write_text(source)
            for limit in [1, 100]:
                projections = {}
                for revision, binary in cli_binaries.items():
                    command = [
                        binary, "plan", "--root", str(root), "--file", "service.py",
                        "--operators", "exception_hierarchy", "--max-candidates", str(limit),
                        "--allow-best-effort-memory", "--min-free-space", "1B",
                        "--", "python3", "-c", "pass",
                    ]
                    # plan exits 4 for a successful but truncated result.
                    document = invoke(command, accepted=(0, 4))
                    projection = {key: document[key] for key in ["candidates", "diagnostics", "truncated"]}
                    projections[revision] = projection
                    cli.append({"scenario": scenario, "limit": limit, "revision": revision, "output": projection})
                assert projections["before"] == projections["after"], (scenario, limit)

        # Large source parsing still exceeds this deadline after scan elimination.
        # This observation is distinct from deterministic cancellation unit tests.
        (root / "errors.py").write_text("".join(f"value{n} = 0\n" for n in range(30_000)))
        (root / "service.py").write_text("".join(f"import errors as alias{n}\n" for n in range(30_000)))
        timeout_results = []
        for revision, binary in cli_binaries.items():
            command = [
                binary, "plan", "--root", str(root), "--file", "service.py",
                "--operators", "exception_hierarchy", "--analyzer-timeout", "10ms",
                "--allow-best-effort-memory", "--min-free-space", "1B",
                "--", "python3", "-c", "pass",
            ]
            result = subprocess.run(command, capture_output=True, text=True, timeout=60)
            assert result.returncode == 2 and not result.stdout
            assert "analyzer.timeout" in result.stderr
            timeout_results.append({
                "revision": revision, "exit": result.returncode,
                "stdout_bytes": len(result.stdout), "stderr": result.stderr,
            })

    metric_names = [
        "elapsed_ns", "cumulative_requested_bytes", "allocation_calls",
        "retained_requested_bytes", "peak_requested_bytes", "cancellation_polls",
    ]
    summary = []
    for ordinary, aliases, classes in workloads:
        for revision in binaries:
            rows = [row for row in samples if (row["ordinary"], row["aliases"], row["classes"], row["revision"]) == (ordinary, aliases, classes, revision)]
            summary.append({
                "ordinary": ordinary, "aliases": aliases, "classes": classes, "revision": revision,
                "metrics": {name: {"median": statistics.median(row[name] for row in rows), "min": min(row[name] for row in rows), "max": max(row[name] for row in rows)} for name in metric_names},
            })
    evidence = {
        "platform": platform.platform(),
        "method": {
            "operation": "ExceptionIndex::from_sources_with_inputs; fixture construction excluded",
            "warmup": "one index construction and drop before measurement per process",
            "samples": f"{args.samples} paired samples per workload; alternating before/after; serial",
            "heap": "successful requested allocation/reallocation bytes; retained and peak deltas relative to live caller setup; excludes post-measurement API output characterization; not RSS or allocator usable bytes",
            "timing": "Instant around one index construction, allocation meter enabled; no CI time threshold",
            "timeout": "30,000 ordinary bindings + 30,000 aliases, CLI 10ms deadline; not a deterministic timing assertion",
        },
        "hashes": {path: sha256(path) for path in [*binaries.values(), *cli_binaries.values(), __file__, str(pathlib.Path(__file__).with_name("visibility_probe.rs")), str(pathlib.Path(__file__).with_name("requested_heap.rs"))]},
        "api_contracts": contracts,
        "samples": samples,
        "summary": summary,
        "cli_contracts": cli,
        "cli_timeout": timeout_results,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    sample_checkpoint.unlink()
    print(f"wrote {args.output}", flush=True)


if __name__ == "__main__":
    main()
