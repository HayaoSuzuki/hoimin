from __future__ import annotations

import base64
import copy
import hashlib
import io
import json
import subprocess
import urllib.error
from email.message import Message
from pathlib import Path

import pytest
import yaml
from pypi_attestations import (
    Attestation,
    Distribution,
    GitHubPublisher,
    VerificationError,
)
from sigstore.models import Bundle
from sigstore.verify import policy
from test_pypi_release import PLATFORMS, write_checksums, write_wheel

from tools import pypi_provenance
from tools.ci_selection import object_mapping

ROOT = Path(__file__).resolve().parents[1]


def recorded_bundle() -> dict[str, object]:
    return object_mapping(
        json.loads((ROOT / "tests/fixtures/pypi-provenance-build.json").read_text())[
            "bundle"
        ]
    )


def attestation(predicate: str, name: str, digest: str) -> dict[str, object]:
    statement = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [{"name": name, "digest": {"sha256": digest}}],
        "predicateType": predicate,
        "predicate": {},
    }
    return {
        "version": 1,
        "envelope": {
            "statement": base64.b64encode(json.dumps(statement).encode()).decode(),
            "signature": "ZmFrZQ==",
        },
        "verification_material": {},
    }


def test_accepts_one_build_and_one_publish_statement() -> None:
    statements = [
        attestation("https://slsa.dev/provenance/v1", "a.whl", "a" * 64),
        attestation("https://docs.pypi.org/attestations/publish/v1", "a.whl", "a" * 64),
    ]
    assert pypi_provenance.statement_types(statements, "a.whl", "a" * 64) == {
        "https://slsa.dev/provenance/v1",
        "https://docs.pypi.org/attestations/publish/v1",
    }


@pytest.mark.parametrize(
    "damage",
    ["digest", "name", "subjects", "duplicate", "missing", "unknown", "encoding"],
)
def test_rejects_invalid_index_statement_sets(damage: str) -> None:
    statements = [
        attestation("https://slsa.dev/provenance/v1", "a.whl", "a" * 64),
        attestation("https://docs.pypi.org/attestations/publish/v1", "a.whl", "a" * 64),
    ]
    if damage == "duplicate":
        statements[1] = statements[0]
    elif damage == "missing":
        statements.pop()
    elif damage == "encoding":
        statements[0]["envelope"] = {"statement": "!"}
    else:
        predicate = (
            "unknown" if damage == "unknown" else "https://slsa.dev/provenance/v1"
        )
        statements[0] = attestation(
            predicate,
            "b.whl" if damage == "name" else "a.whl",
            "b" * 64 if damage == "digest" else "a" * 64,
        )
        if damage == "subjects":
            statement = {
                "subject": [{"name": "a.whl", "digest": {"sha256": "a" * 64}}] * 2
            }
            statements[0]["envelope"] = {
                "statement": base64.b64encode(json.dumps(statement).encode()).decode()
            }
    with pytest.raises(ValueError, match=r"subject|attestation|base64"):
        pypi_provenance.statement_types(statements, "a.whl", "a" * 64)


def test_index_inventory_rejects_partial_or_changed_publication(tmp_path: Path) -> None:
    wheel = tmp_path / "a.whl"
    wheel.write_bytes(b"wheel")
    record = {
        "filename": "a.whl",
        "digests": {"sha256": hashlib.sha256(b"wheel").hexdigest()},
        "yanked": False,
    }
    pypi_provenance.check_inventory({"urls": [record]}, [wheel])
    for records in (
        [],
        [record, record],
        [{**record, "yanked": True}],
        [{**record, "digests": {"sha256": "0" * 64}}],
    ):
        with pytest.raises(ValueError, match="index"):
            pypi_provenance.check_inventory({"urls": records}, [wheel])


def test_release_automates_test_index_before_production_with_opt_in() -> None:
    root = Path(__file__).resolve().parents[1]
    workflow = yaml.safe_load((root / ".github/workflows/release.yml").read_text())
    jobs = workflow["jobs"]
    assert "vars.PYPI_AUTO_PUBLISH == 'true'" in jobs["dispatch-pypi"]["if"]
    assert "publish" in jobs["dispatch-pypi"]["needs"]
    assert "github.event_name == 'workflow_dispatch'" in jobs["pypi-prepare"]["if"]
    assert "pull_request_target" not in jobs["pypi-prepare"]["if"]
    assert jobs["testpypi"]["needs"] == [
        "pypi-prepare",
        "testpypi-inspect",
        "testpypi-publish",
    ]
    assert "testpypi" in jobs["pypi-inspect"]["needs"]
    for index in ("testpypi", "pypi"):
        upload = jobs[index + "-publish"]
        assert "uses" not in upload
        assert upload["permissions"] == {"id-token": "write"}
        assert upload["environment"]["name"] == index
        assert all("run" not in step for step in upload["steps"])


def test_real_github_aggregate_signature_is_valid_but_not_pypi_compatible() -> None:
    bundle = Bundle.from_json(json.dumps(recorded_bundle()))
    value = Attestation.from_bundle(bundle)
    statement = pypi_provenance.statement_of(value.model_dump(mode="json"))
    subjects = statement["subject"]
    assert isinstance(subjects, list)
    subject = object_mapping(
        next(
            item
            for item in subjects
            if object_mapping(item)["name"] == "hoimin-0.3.5-py3-none-win_amd64.whl"
        )
    )
    digest = str(object_mapping(subject["digest"])["sha256"])
    # Verification reaches the subject-count check only after authenticating
    # the real GitHub certificate, DSSE signature and transparency entry.
    with pytest.raises(VerificationError, match="too many subjects"):
        value.verify(
            GitHubPublisher(repository="HayaoSuzuki/hoimin", workflow="release.yml"),
            Distribution(name=str(subject["name"]), digest=digest),
            offline=True,
        )


@pytest.mark.parametrize("damage", ["signature", "repository", "workflow", "source"])
def test_real_signature_rejects_corruption_and_wrong_identity(damage: str) -> None:
    raw = recorded_bundle()
    if damage == "signature":
        envelope = object_mapping(raw["dsseEnvelope"])
        signatures = envelope["signatures"]
        assert isinstance(signatures, list)
        signature = object_mapping(signatures[0])
        signature["sig"] = base64.b64encode(b"invalid signature").decode()
        envelope["signatures"] = [signature]
        raw["dsseEnvelope"] = envelope
    value = Attestation.from_bundle(Bundle.from_json(json.dumps(raw)))
    publisher = GitHubPublisher(
        repository="actions/attest" if damage == "repository" else "HayaoSuzuki/hoimin",
        workflow="publish-pypi.yml" if damage == "workflow" else "release.yml",
    )
    with pytest.raises(VerificationError, match="Verification failed"):
        value.verify(
            policy.OIDCSourceRepositoryDigest("0" * 40)
            if damage == "source"
            else publisher,
            Distribution(name="hoimin-0.3.5-py3-none-win_amd64.whl", digest="a" * 64),
            offline=True,
        )


@pytest.mark.parametrize("status", [404, 403, 500])
def test_only_inspection_404_means_not_published(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, status: int
) -> None:
    def unavailable(_url: str) -> dict[str, object]:
        msg = "fixture"
        url = "https://pypi.org/"
        raise urllib.error.HTTPError(url, status, msg, Message(), None)

    monkeypatch.setattr(pypi_provenance, "fetch_json", unavailable)
    if status == 404:
        assert (
            pypi_provenance.verify_index(
                "pypi", "v1.2.3", "a" * 40, tmp_path, allow_missing=True
            )
            is False
        )
    else:
        with pytest.raises(urllib.error.HTTPError):
            pypi_provenance.verify_index(
                "pypi", "v1.2.3", "a" * 40, tmp_path, allow_missing=True
            )
    with pytest.raises(urllib.error.HTTPError):
        pypi_provenance.verify_index("pypi", "v1.2.3", "a" * 40, tmp_path)


@pytest.mark.parametrize("commit", ["main", "-x", "a" * 39, "a" * 41])
def test_build_verification_rejects_unpinned_source(
    tmp_path: Path, commit: str
) -> None:
    with pytest.raises(ValueError, match="full release commit SHA"):
        pypi_provenance.verify_build(
            tmp_path / "a.whl", tmp_path / "bundle.json", commit
        )


@pytest.mark.parametrize(
    "mode", ["valid", "missing", "digest", "signature", "duplicate"]
)
def test_stage_is_atomic_and_preserves_signed_statement_bytes(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, mode: str
) -> None:
    # Only GitHub/TUF signature services are doubled here. The recorded bundle
    # converter, wheel validator, policy arguments and staged bytes are real.
    assets = tmp_path / "assets"
    assets.mkdir()
    for platform in PLATFORMS:
        write_wheel(assets / f"hoimin-1.2.3-py3-none-{platform}.whl")
    write_checksums(assets)
    release_json = tmp_path / "release.json"
    release_json.write_text(
        json.dumps({"tagName": "v1.2.3", "isDraft": False, "isPrerelease": False})
    )
    source_statements: dict[str, bytes] = {}

    def github(arguments: list[str], *, cwd: Path | None = None) -> None:
        wheel = Path(arguments[2])
        if arguments[1] == "verify":
            expected = {
                "--repo": "HayaoSuzuki/hoimin",
                "--signer-workflow": "HayaoSuzuki/hoimin/.github/workflows/release.yml",
                "--source-digest": "a" * 40,
                "--signer-digest": "a" * 40,
                "--predicate-type": "https://slsa.dev/provenance/v1",
            }
            assert all(
                arguments[arguments.index(key) + 1] == value
                for key, value in expected.items()
            )
            assert "--deny-self-hosted-runners" in arguments
            if mode == "signature":
                raise subprocess.CalledProcessError(1, arguments)
            return
        assert cwd is not None
        if mode == "missing":
            return
        raw = copy.deepcopy(recorded_bundle())
        envelope = object_mapping(raw["dsseEnvelope"])
        statement = object_mapping(
            json.loads(base64.b64decode(str(envelope["payload"])))
        )
        statement["subject"] = [
            {
                "name": wheel.name,
                "digest": {
                    "sha256": "0" * 64
                    if mode == "digest"
                    else pypi_provenance.sha256(wheel)
                },
            }
        ]
        payload = json.dumps(statement).encode()
        envelope["payload"] = base64.b64encode(payload).decode()
        raw["dsseEnvelope"] = envelope
        source_statements[wheel.name] = payload
        line = json.dumps({"bundle": raw}) + "\n"
        (cwd / "download.jsonl").write_text(line * (2 if mode == "duplicate" else 1))

    def authenticated(
        _self: Attestation, _publisher: GitHubPublisher, _dist: Distribution
    ) -> tuple[str, None]:
        return "https://slsa.dev/provenance/v1", None

    monkeypatch.setattr(pypi_provenance, "run_gh", github)
    monkeypatch.setattr(Attestation, "verify", authenticated)
    output = tmp_path / "dist"
    if mode not in {"valid", "duplicate"}:
        with pytest.raises(
            (ValueError, subprocess.CalledProcessError), match=r"attestation|non-zero"
        ):
            pypi_provenance.stage("v1.2.3", "a" * 40, assets, release_json, output)
        assert not output.exists()
        return
    pypi_provenance.stage("v1.2.3", "a" * 40, assets, release_json, output)
    assert len(list(output.iterdir())) == 6
    for wheel in output.glob("*.whl"):
        assert wheel.read_bytes() == (assets / wheel.name).read_bytes()
        value = Attestation.model_validate_json(
            wheel.with_suffix(".whl.slsa.attestation").read_text(encoding="utf-8")
        )
        assert value.envelope.statement == source_statements[wheel.name]


def index_data(
    url: str, records: list[dict[str, object]], template: dict[str, object], damage: str
) -> dict[str, object]:
    if url.endswith("/json"):
        return {"urls": records}
    name = url.split("/")[-2]
    values: list[dict[str, object]] = []
    for predicate in (
        "https://slsa.dev/provenance/v1",
        "https://docs.pypi.org/attestations/publish/v1",
    ):
        value = copy.deepcopy(template)
        value["envelope"] = attestation(
            predicate, name, hashlib.sha256(b"wheel").hexdigest()
        )["envelope"]
        values.append(value)
    if damage == "missing":
        values.pop()
    if damage == "duplicate":
        values[1] = values[0]
    return {
        "attestation_bundles": [
            {
                "publisher": {
                    "kind": "GitHub",
                    "repository": "other/repo"
                    if damage == "publisher"
                    else "HayaoSuzuki/hoimin",
                    "workflow": "release.yml",
                    "environment": "testpypi",
                },
                "attestations": values,
            }
        ]
    }


@pytest.mark.parametrize(
    "damage", ["valid", "bytes", "publisher", "missing", "duplicate", "source", "host"]
)
def test_index_verification_gates_on_downloads_and_both_signatures(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, damage: str
) -> None:
    # External index and cryptographic services are controlled at their boundaries;
    # separate recorded-bundle tests exercise real signature/identity rejection.
    template = Attestation.from_bundle(
        Bundle.from_json(json.dumps(recorded_bundle()))
    ).model_dump(mode="json")
    records: list[dict[str, object]] = []
    for platform in PLATFORMS:
        name = f"hoimin-1.2.3-py3-none-{platform}.whl"
        (tmp_path / name).write_bytes(b"wheel")
        records.append(
            {
                "filename": name,
                "digests": {"sha256": hashlib.sha256(b"wheel").hexdigest()},
                "yanked": False,
                "url": (
                    "http://invalid/"
                    if damage == "host"
                    else "https://test-files.pythonhosted.org/packages/"
                )
                + name,
            }
        )

    def download(_url: str, *, timeout: int) -> io.BytesIO:
        assert timeout > 0
        return io.BytesIO(b"modified" if damage == "bytes" else b"wheel")

    verified: list[str] = []

    def signature(
        self: Attestation, publisher: GitHubPublisher, dist: Distribution
    ) -> tuple[str, None]:
        assert publisher.repository == "HayaoSuzuki/hoimin"
        assert publisher.workflow == "release.yml"
        assert dist.digest == hashlib.sha256(b"wheel").hexdigest()
        verified.append(dist.name)
        return str(
            pypi_provenance.statement_of(self.model_dump(mode="json"))["predicateType"]
        ), None

    def build(_wheel: Path, _bundle: Path, commit: str) -> None:
        assert commit == "a" * 40
        if damage == "source":
            raise subprocess.CalledProcessError(1, ["gh"])

    monkeypatch.setattr(
        pypi_provenance,
        "fetch_json",
        lambda url: index_data(url, records, template, damage),
    )
    monkeypatch.setattr(pypi_provenance.urllib.request, "urlopen", download)
    monkeypatch.setattr(Attestation, "verify", signature)
    monkeypatch.setattr(pypi_provenance, "verify_build", build)
    if damage == "valid":
        assert pypi_provenance.verify_index("testpypi", "v1.2.3", "a" * 40, tmp_path)
        assert len(verified) == 6
    else:
        with pytest.raises(
            (ValueError, subprocess.CalledProcessError),
            match=r"differs|publisher|attestation|non-zero|host",
        ):
            pypi_provenance.verify_index("testpypi", "v1.2.3", "a" * 40, tmp_path)
