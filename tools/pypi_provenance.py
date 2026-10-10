"""Verify and stage build provenance for unchanged release wheels."""

from __future__ import annotations

import argparse
import base64
import hashlib
import http
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

from pypi_attestations import Attestation, Distribution, GitHubPublisher
from sigstore.models import Bundle

from tools.ci_selection import object_mapping
from tools.pypi_release import prepare_wheels

REPOSITORY = "HayaoSuzuki/hoimin"
WORKFLOW = "release.yml"
SLSA = "https://slsa.dev/provenance/v1"
PUBLISH = "https://docs.pypi.org/attestations/publish/v1"
INDEXES = {"pypi": "https://pypi.org", "testpypi": "https://test.pypi.org"}
PREDICATES = {SLSA, PUBLISH}
PLATFORMS = 3
PROPAGATION_ATTEMPTS = 6


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def statement_of(value: object) -> dict[str, object]:
    envelope = object_mapping(object_mapping(value)["envelope"])
    encoded = envelope["statement"]
    if not isinstance(encoded, str):
        msg = "Invalid encoded attestation statement"
        raise TypeError(msg)
    return object_mapping(json.loads(base64.b64decode(encoded, validate=True)))


def statement_types(
    values: list[dict[str, object]], name: str, digest: str
) -> set[str]:
    types: list[str] = []
    for value in values:
        statement = statement_of(value)
        if statement.get("subject") != [{"name": name, "digest": {"sha256": digest}}]:
            msg = "Expected exactly one matching wheel subject"
            raise ValueError(msg)
        predicate = statement.get("predicateType")
        if not isinstance(predicate, str):
            msg = "Missing predicate type"
            raise TypeError(msg)
        types.append(predicate)
    if len(types) != len(PREDICATES) or set(types) != PREDICATES:
        msg = "Expected one SLSA and one publish attestation"
        raise ValueError(msg)
    return set(types)


def check_inventory(data: dict[str, object], wheels: list[Path]) -> None:
    records = data.get("urls")
    if not isinstance(records, list):
        msg = "Missing index file inventory"
        raise TypeError(msg)
    expected = {wheel.name: sha256(wheel) for wheel in wheels}
    actual: dict[str, object] = {}
    for value in records:
        record = object_mapping(value)
        name = record.get("filename")
        if (
            not isinstance(name, str)
            or name in actual
            or record.get("yanked") is not False
        ):
            msg = "Duplicate, invalid or yanked index file"
            raise ValueError(msg)
        actual[name] = object_mapping(record["digests"]).get("sha256")
    if actual != expected:
        msg = "Partial or mismatching index publication; refusing automatic repair"
        raise ValueError(msg)


def run_gh(arguments: list[str], *, cwd: Path | None = None) -> None:
    gh = shutil.which("gh")
    if gh is None:
        msg = "GitHub CLI is required"
        raise FileNotFoundError(msg)
    subprocess.run(  # noqa: S603 -- resolved executable, separate arguments, no shell
        [gh, *arguments],
        cwd=cwd,
        check=True,
        timeout=120,
        stdout=subprocess.DEVNULL,
    )


def verify_build(wheel: Path, bundle: Path, commit: str) -> None:
    if re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        msg = "Expected full release commit SHA"
        raise ValueError(msg)
    run_gh(
        [
            "attestation",
            "verify",
            str(wheel.resolve()),
            "--bundle",
            str(bundle.resolve()),
            "--repo",
            REPOSITORY,
            "--signer-workflow",
            f"{REPOSITORY}/.github/workflows/{WORKFLOW}",
            "--source-digest",
            commit,
            "--signer-digest",
            commit,
            "--deny-self-hosted-runners",
            "--predicate-type",
            SLSA,
        ]
    )


def stage(
    tag: str, commit: str, assets: Path, release_json: Path, output: Path
) -> None:
    # Nothing is staged in the destination until all three signatures verify.
    if output.exists():
        msg = "Output already exists"
        raise ValueError(msg)
    with tempfile.TemporaryDirectory(prefix="hoimin-pypi-stage-") as temporary:
        root = Path(temporary)
        staged = root / "dist"
        prepare_wheels(tag, release_json, assets, staged)
        for wheel in sorted(staged.glob("*.whl")):
            download = root / wheel.name
            download.mkdir()
            run_gh(
                [
                    "attestation",
                    "download",
                    str(wheel.resolve()),
                    "--repo",
                    REPOSITORY,
                    "--predicate-type",
                    SLSA,
                ],
                cwd=download,
            )
            candidates = list(download.glob("*.jsonl"))
            if len(candidates) != 1:
                msg = "Missing GitHub build attestations"
                raise ValueError(msg)
            matching: list[Attestation] = []
            for line in candidates[0].read_text(encoding="utf-8").splitlines():
                raw = object_mapping(json.loads(line))["bundle"]
                bundle = Bundle.from_json(json.dumps(raw))
                value = Attestation.from_bundle(bundle)
                statement = statement_of(value.model_dump(mode="json"))
                if statement.get("subject") != [
                    {"name": wheel.name, "digest": {"sha256": sha256(wheel)}}
                ]:
                    continue
                bundle_path = root / "selected-bundle.json"
                bundle_path.write_text(json.dumps(raw), encoding="utf-8")
                # A same-subject invalid signature is fatal, not a reason to fall back.
                verify_build(wheel, bundle_path, commit)
                value.verify(
                    GitHubPublisher(repository=REPOSITORY, workflow=WORKFLOW),
                    Distribution.from_file(wheel),
                )
                matching.append(value)
            if not matching:
                msg = "Missing original single-wheel build attestation"
                raise ValueError(msg)
            # Rerunning the original build may add equivalent valid signatures.
            # Verify all matching candidates, then choose deterministically.
            selected = min(matching, key=lambda value: value.model_dump_json())
            wheel.with_suffix(".whl.slsa.attestation").write_text(
                selected.model_dump_json(), encoding="utf-8"
            )
        shutil.copytree(staged, output)


def fetch_json(url: str) -> dict[str, object]:
    # Callers construct URLs from fixed HTTPS index hosts and validated filenames.
    with urllib.request.urlopen(url, timeout=60) as response:  # noqa: S310 -- fixed HTTPS hosts
        return object_mapping(json.load(response))


def verify_index(
    index: str, tag: str, commit: str, directory: Path, *, allow_missing: bool = False
) -> bool:
    if (
        re.fullmatch(r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", tag)
        is None
    ):
        msg = "Expected stable release tag"
        raise ValueError(msg)
    base = INDEXES[index]
    try:
        data = fetch_json(f"{base}/pypi/hoimin/{tag[1:]}/json")
    except urllib.error.HTTPError as error:
        if allow_missing and error.code == http.HTTPStatus.NOT_FOUND:
            return False
        raise
    wheels = sorted(directory.glob("*.whl"))
    if len(wheels) != PLATFORMS:
        msg = "Expected three staged wheels"
        raise ValueError(msg)
    check_inventory(data, wheels)
    with tempfile.TemporaryDirectory(prefix="hoimin-pypi-verify-") as temporary:
        root = Path(temporary)
        for wheel in wheels:
            verify_index_wheel(
                f"{base}/integrity/hoimin/{tag[1:]}/{wheel.name}/provenance",
                commit,
                wheel,
                data,
                root=root,
            )
    return True


def verify_index_wheel(
    provenance_url: str,
    commit: str,
    wheel: Path,
    data: dict[str, object],
    *,
    root: Path,
) -> None:
    records = data["urls"]
    assert isinstance(records, list)
    record = next(
        object_mapping(item)
        for item in records
        if object_mapping(item)["filename"] == wheel.name
    )
    url = record["url"]
    host = (
        "files.pythonhosted.org"
        if provenance_url.startswith(INDEXES["pypi"] + "/")
        else "test-files.pythonhosted.org"
    )
    if not isinstance(url, str) or not url.startswith(f"https://{host}/packages/"):
        msg = "Unexpected index download host"
        raise ValueError(msg)
    downloaded = root / wheel.name
    with (
        urllib.request.urlopen(url, timeout=120) as response,  # noqa: S310 -- validated HTTPS index host
        downloaded.open("wb") as target,
    ):
        shutil.copyfileobj(response, target)
    if sha256(downloaded) != sha256(wheel):
        msg = "Downloaded index wheel differs from GitHub wheel"
        raise ValueError(msg)
    provenance = fetch_json(provenance_url)
    bundles = provenance.get("attestation_bundles")
    if not isinstance(bundles, list) or len(bundles) != 1:
        msg = "Expected exactly one attestation bundle"
        raise ValueError(msg)
    container = object_mapping(bundles[0])
    publisher = GitHubPublisher(repository=REPOSITORY, workflow=WORKFLOW)
    actual_publisher = object_mapping(container["publisher"])
    if (
        actual_publisher.get("kind") != "GitHub"
        or actual_publisher.get("repository") != REPOSITORY
        or actual_publisher.get("workflow") != WORKFLOW
    ):
        msg = "Unexpected index publisher identity"
        raise ValueError(msg)
    values = container.get("attestations")
    if not isinstance(values, list):
        msg = "Missing attestations"
        raise TypeError(msg)
    statements = [object_mapping(value) for value in values]
    statement_types(statements, wheel.name, sha256(wheel))
    for value in statements:
        attestation = Attestation.model_validate(value)
        predicate, _ = attestation.verify(publisher, Distribution.from_file(downloaded))
        if predicate == SLSA:
            bundle = root / "index-build.json"
            bundle.write_text(attestation.to_bundle().to_json(), encoding="utf-8")
            verify_build(downloaded, bundle, commit)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["stage", "inspect", "verify"])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--release-json", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--index", choices=INDEXES)
    args = parser.parse_args()
    if args.mode == "stage":
        if args.output is None or args.release_json is None:
            parser.error("stage requires --output and --release-json")
        stage(args.tag, args.commit, args.directory, args.release_json, args.output)
    else:
        if args.index is None:
            parser.error("inspect/verify require --index")
        # Index/CDN propagation may briefly return 404 after a successful upload.
        published = False
        for attempt in range(PROPAGATION_ATTEMPTS):
            try:
                published = verify_index(
                    args.index,
                    args.tag,
                    args.commit,
                    args.directory,
                    allow_missing=args.mode == "inspect",
                )
                break
            except urllib.error.HTTPError as error:
                if (
                    args.mode != "verify"
                    or error.code != http.HTTPStatus.NOT_FOUND
                    or attempt == PROPAGATION_ATTEMPTS - 1
                ):
                    raise
                time.sleep(10)
        if args.mode == "inspect":
            with Path(os.environ["GITHUB_OUTPUT"]).open(
                "a", encoding="utf-8"
            ) as output:
                output.write(f"published={str(published).lower()}\n")


if __name__ == "__main__":
    main()
