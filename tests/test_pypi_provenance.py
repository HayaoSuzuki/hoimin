from __future__ import annotations

import base64
import copy
import hashlib
import io
import json
import os
import subprocess
import sys
import urllib.error
from collections.abc import Callable
from email.message import Message
from operator import setitem
from pathlib import Path
from unittest.mock import create_autospec, patch

import pytest
import yaml
from hypothesis import given, settings
from hypothesis import strategies as st
from pypi_attestations import (
    Attestation,
    Distribution,
    GitHubPublisher,
    VerificationError,
)
from pytest_mock import MockerFixture
from sigstore.models import Bundle
from sigstore.verify import policy
from test_pypi_release import PLATFORMS, write_checksums, write_wheel

from tools import pypi_provenance
from tools.ci_selection import object_mapping
from tools.pypi_provenance import fetch_json as fetch_json_spec

ROOT = Path(__file__).resolve().parents[1]


def test_propagation_uses_capped_exponential_backoff() -> None:
    unavailable = urllib.error.HTTPError(
        "https://pypi.org/", 404, "Not Found", Message(), None
    )
    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=[unavailable] * 4 + [True]
        ),
        patch.object(pypi_provenance.time, "sleep") as sleep,
        patch.object(pypi_provenance.time, "monotonic", return_value=0),
    ):
        result = pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert result is True
    assert [call.args[0] for call in sleep.call_args_list] == [10, 20, 40, 60]


@settings(max_examples=20)
@given(failures=st.integers(min_value=0, max_value=11))
def test_propagation_verification_waits_for_delayed_publication(failures: int) -> None:
    unavailable = urllib.error.HTTPError(
        "https://test.pypi.org/pypi/hoimin/0.4.2/json",
        404,
        "Not Found",
        Message(),
        None,
    )
    responses = [unavailable] * failures + [True]
    clock = [0.0]

    def advance(seconds: float) -> None:
        clock[0] += seconds

    with (
        patch.object(pypi_provenance, "verify_index", side_effect=responses) as verify,
        patch.object(pypi_provenance.time, "sleep", side_effect=advance) as sleep,
        patch.object(pypi_provenance.time, "monotonic", side_effect=lambda: clock[0]),
    ):
        result = pypi_provenance.verify_with_propagation(
            "testpypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert result is True
    assert verify.call_count == failures + 1
    assert sleep.call_count == failures
    assert sum(call.args[0] for call in sleep.call_args_list) <= 600


def test_propagation_backoff_exhausts_budget_with_shortened_final_sleep() -> None:
    expected_delays = [10, 20, 40] + [60] * 8 + [50]
    unavailable = urllib.error.HTTPError(
        "https://pypi.org/", 404, "Not Found", Message(), None
    )
    clock = [0.0]

    def advance(seconds: float) -> None:
        clock[0] += seconds

    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=unavailable
        ) as verify,
        patch.object(pypi_provenance.time, "sleep", side_effect=advance) as sleep,
        patch.object(pypi_provenance.time, "monotonic", side_effect=lambda: clock[0]),
        pytest.raises(urllib.error.HTTPError) as raised,
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert raised.value is unavailable
    assert verify.call_count == 12
    assert [call.args[0] for call in sleep.call_args_list] == expected_delays
    assert clock[0] == 600


def test_propagation_stops_after_bounded_attempts() -> None:
    unavailable = urllib.error.HTTPError(
        "https://pypi.org/pypi/hoimin/0.4.2/json", 404, "Not Found", Message(), None
    )
    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=unavailable
        ) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
        patch.object(pypi_provenance.time, "monotonic", return_value=0),
        pytest.raises(urllib.error.HTTPError) as raised,
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert raised.value is unavailable
    assert verify.call_count == 61
    assert sleep.call_count == 60


def test_propagation_stops_when_request_time_consumes_budget() -> None:
    unavailable = urllib.error.HTTPError(
        "https://pypi.org/pypi/hoimin/0.4.2/json", 404, "Not Found", Message(), None
    )
    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=unavailable
        ) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
        patch.object(pypi_provenance.time, "monotonic", side_effect=[0, 601]),
        pytest.raises(urllib.error.HTTPError),
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert verify.call_count == 1
    sleep.assert_not_called()


@pytest.mark.parametrize("status", [403, 429, 500, 502, 503])
def test_propagation_does_not_retry_other_http_errors(status: int) -> None:
    rejected = urllib.error.HTTPError(
        "https://pypi.org/", status, "rejected", Message(), None
    )
    with (
        patch.object(pypi_provenance, "verify_index", side_effect=rejected) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
        pytest.raises(urllib.error.HTTPError) as raised,
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert raised.value is rejected
    verify.assert_called_once()
    sleep.assert_not_called()


def test_propagation_does_not_start_verification_at_expired_deadline() -> None:
    unavailable = urllib.error.HTTPError(
        "https://pypi.org/", 404, "Not Found", Message(), None
    )
    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=unavailable
        ) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
        patch.object(pypi_provenance.time, "monotonic", side_effect=[0, 599, 600]),
        pytest.raises(urllib.error.HTTPError) as raised,
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    assert raised.value is unavailable
    verify.assert_called_once()
    sleep.assert_called_once_with(1)


def test_propagation_does_not_retry_invalid_publication() -> None:
    with (
        patch.object(
            pypi_provenance, "verify_index", side_effect=ValueError("digest")
        ) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
        pytest.raises(ValueError, match="digest"),
    ):
        pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=False
        )

    verify.assert_called_once()
    sleep.assert_not_called()


def test_inspection_remains_immediate_for_missing_release() -> None:
    with (
        patch.object(pypi_provenance, "verify_index", return_value=False) as verify,
        patch.object(pypi_provenance.time, "sleep") as sleep,
    ):
        result = pypi_provenance.verify_with_propagation(
            "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=True
        )

    assert result is False
    verify.assert_called_once_with(
        "pypi", "v0.4.2", "a" * 40, Path("dist"), allow_missing=True
    )
    sleep.assert_not_called()


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

    statement_types_result = pypi_provenance.statement_types(
        statements, "a.whl", "a" * 64
    )

    assert statement_types_result == {
        "https://slsa.dev/provenance/v1",
        "https://docs.pypi.org/attestations/publish/v1",
    }


def multiple_subject_attestation() -> dict[str, object]:
    statement = {"subject": [{"name": "a.whl", "digest": {"sha256": "a" * 64}}] * 2}
    value = attestation("https://slsa.dev/provenance/v1", "a.whl", "a" * 64)
    value["envelope"] = {
        "statement": base64.b64encode(json.dumps(statement).encode()).decode()
    }
    return value


@pytest.mark.parametrize(
    "corrupt",
    [
        pytest.param(
            lambda values: setitem(
                values,
                0,
                attestation("https://slsa.dev/provenance/v1", "a.whl", "b" * 64),
            ),
            id="digest",
        ),
        pytest.param(
            lambda values: setitem(
                values,
                0,
                attestation("https://slsa.dev/provenance/v1", "b.whl", "a" * 64),
            ),
            id="name",
        ),
        pytest.param(
            lambda values: setitem(values, 0, multiple_subject_attestation()),
            id="subjects",
        ),
        pytest.param(lambda values: setitem(values, 1, values[0]), id="duplicate"),
        pytest.param(lambda values: values.pop(), id="missing"),
        pytest.param(
            lambda values: setitem(
                values, 0, attestation("unknown", "a.whl", "a" * 64)
            ),
            id="unknown",
        ),
        pytest.param(
            lambda values: setitem(values[0], "envelope", {"statement": "!"}),
            id="encoding",
        ),
    ],
)
def test_rejects_invalid_index_statement_sets(
    corrupt: Callable[[list[dict[str, object]]], object],
) -> None:
    statements = [
        attestation("https://slsa.dev/provenance/v1", "a.whl", "a" * 64),
        attestation("https://docs.pypi.org/attestations/publish/v1", "a.whl", "a" * 64),
    ]
    corrupt(statements)

    with pytest.raises(ValueError, match=r"subject|attestation|base64"):
        pypi_provenance.statement_types(statements, "a.whl", "a" * 64)


@pytest.mark.parametrize(
    "records",
    [[], ["valid", "valid"], ["yanked"], ["changed"]],
    ids=["missing", "duplicate", "yanked", "digest-mismatch"],
)
def test_index_inventory_rejects_partial_or_changed_publication(
    tmp_path: Path, records: list[str]
) -> None:
    wheel = tmp_path / "a.whl"
    wheel.write_bytes(b"wheel")
    record = {
        "filename": "a.whl",
        "digests": {"sha256": hashlib.sha256(b"wheel").hexdigest()},
        "yanked": False,
    }
    variants = {
        "valid": record,
        "yanked": {**record, "yanked": True},
        "changed": {**record, "digests": {"sha256": "0" * 64}},
    }
    inventory: dict[str, object] = {"urls": [variants[name] for name in records]}

    with pytest.raises(ValueError, match="index"):
        pypi_provenance.check_inventory(inventory, [wheel])


def test_index_inventory_accepts_matching_publication(tmp_path: Path) -> None:
    wheel = tmp_path / "a.whl"
    wheel.write_bytes(b"wheel")
    record = {
        "filename": "a.whl",
        "digests": {"sha256": hashlib.sha256(b"wheel").hexdigest()},
        "yanked": False,
    }

    pypi_provenance.check_inventory({"urls": [record]}, [wheel])


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


@pytest.mark.parametrize("index", ["testpypi", "pypi"])
def test_pypi_upload_uses_protected_oidc_environment(index: str) -> None:
    root = Path(__file__).resolve().parents[1]
    workflow = yaml.safe_load((root / ".github/workflows/release.yml").read_text())

    upload = workflow["jobs"][index + "-publish"]

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


def downloaded_proof_lines() -> list[str]:
    return (
        (ROOT / "tests/fixtures/pypi-provenance-download.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )


def proof_subject_count(line: str) -> int:
    envelope = json.loads(line)["dsseEnvelope"]
    return len(json.loads(base64.b64decode(envelope["payload"]))["subject"])


def test_real_cli_download_contains_aggregate_and_individual_proof() -> None:
    lines = downloaded_proof_lines()

    counts = [proof_subject_count(line) for line in lines]

    assert sorted(counts) == [1, 13]


@pytest.mark.parametrize(
    ("line", "expected_subjects"),
    list(zip(downloaded_proof_lines(), [1, 13], strict=True)),
    ids=["individual-wheel", "aggregate-release"],
)
def test_downloaded_bundle_retains_typed_subjects(
    line: str, expected_subjects: int
) -> None:
    bundle = Bundle.from_json(line)

    value = Attestation.from_bundle(bundle)
    statement = pypi_provenance.statement_of(value.model_dump(mode="json"))

    assert isinstance(statement["subject"], list)
    assert len(statement["subject"]) == expected_subjects


@pytest.mark.parametrize(
    "line",
    [line for line in downloaded_proof_lines() if proof_subject_count(line) == 1],
    ids=["individual-wheel"],
)
@pytest.mark.parametrize(
    "identity",
    [
        GitHubPublisher(repository="HayaoSuzuki/hoimin", workflow="release.yml"),
        policy.OIDCSourceRepositoryDigest("c9f92eb721d8a858a2f55643a5c72723763174dd"),
    ],
    ids=["workflow-publisher", "source-revision"],
)
def test_real_cli_download_contains_individually_verifiable_wheel_proof(
    line: str, identity: GitHubPublisher | policy.OIDCSourceRepositoryDigest
) -> None:
    value = Attestation.from_bundle(Bundle.from_json(line))

    statement = pypi_provenance.statement_of(value.model_dump(mode="json"))
    subjects = statement["subject"]

    assert isinstance(subjects, list)
    subject = object_mapping(subjects[0])
    distribution = Distribution(
        name=str(subject["name"]),
        digest=str(object_mapping(subject["digest"])["sha256"]),
    )
    envelope = object_mapping(json.loads(line)["dsseEnvelope"])
    expected_statement = base64.b64decode(str(envelope["payload"]))

    predicate, _ = value.verify(identity, distribution, offline=True)

    assert predicate == "https://slsa.dev/provenance/v1"
    assert value.envelope.statement == expected_statement


def test_real_signature_rejects_corruption() -> None:
    raw = recorded_bundle()
    envelope = object_mapping(raw["dsseEnvelope"])
    signatures = envelope["signatures"]

    assert isinstance(signatures, list)
    signature = object_mapping(signatures[0])
    signature["sig"] = base64.b64encode(b"invalid signature").decode()
    envelope["signatures"] = [signature]
    raw["dsseEnvelope"] = envelope
    value = Attestation.from_bundle(Bundle.from_json(json.dumps(raw)))

    with pytest.raises(VerificationError, match="Verification failed"):
        value.verify(
            GitHubPublisher(repository="HayaoSuzuki/hoimin", workflow="release.yml"),
            Distribution(name="hoimin-0.3.5-py3-none-win_amd64.whl", digest="a" * 64),
            offline=True,
        )


@pytest.mark.parametrize(
    "publisher",
    [
        pytest.param(
            GitHubPublisher(repository="actions/attest", workflow="release.yml"),
            id="repository",
        ),
        pytest.param(
            GitHubPublisher(
                repository="HayaoSuzuki/hoimin", workflow="publish-pypi.yml"
            ),
            id="workflow",
        ),
        pytest.param(policy.OIDCSourceRepositoryDigest("0" * 40), id="source"),
    ],
)
def test_real_signature_rejects_wrong_identity(
    publisher: GitHubPublisher | policy.OIDCSourceRepositoryDigest,
) -> None:
    value = Attestation.from_bundle(Bundle.from_json(json.dumps(recorded_bundle())))

    with pytest.raises(VerificationError, match="Verification failed"):
        value.verify(
            publisher,
            Distribution(name="hoimin-0.3.5-py3-none-win_amd64.whl", digest="a" * 64),
            offline=True,
        )


@pytest.fixture
def unavailable_index(mocker: MockerFixture) -> Callable[[int], None]:
    original_fetch_json_spec = fetch_json_spec

    def install(status: int) -> None:
        def unavailable(_url: str) -> dict[str, object]:
            url = "https://pypi.org/"
            message = "fixture"
            raise urllib.error.HTTPError(url, status, message, Message(), None)

        mocker.patch.object(
            pypi_provenance,
            "fetch_json",
            create_autospec(original_fetch_json_spec, side_effect=unavailable),
        )

    return install


def test_inspection_404_means_not_published(
    tmp_path: Path, unavailable_index: Callable[[int], None]
) -> None:
    unavailable_index(404)

    result = pypi_provenance.verify_index(
        "pypi", "v1.2.3", "a" * 40, tmp_path, allow_missing=True
    )

    assert result is False


@pytest.mark.parametrize("status", [403, 500])
def test_inspection_propagates_http_errors(
    tmp_path: Path, unavailable_index: Callable[[int], None], status: int
) -> None:
    unavailable_index(status)

    with pytest.raises(urllib.error.HTTPError):
        pypi_provenance.verify_index(
            "pypi", "v1.2.3", "a" * 40, tmp_path, allow_missing=True
        )


@pytest.mark.parametrize("status", [404, 403, 500])
def test_required_index_propagates_http_errors(
    tmp_path: Path, unavailable_index: Callable[[int], None], status: int
) -> None:
    unavailable_index(status)

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


def reject_signature(arguments: list[str]) -> None:
    raise subprocess.CalledProcessError(1, arguments)


def accept_signature(_arguments: list[str]) -> None:
    pass


@pytest.fixture
def stage_environment(
    request: pytest.FixtureRequest, mocker: MockerFixture, tmp_path: Path
) -> tuple[Path, Path, dict[str, bytes]]:
    original_run_gh_spec = pypi_provenance.run_gh
    original_verify_spec = Attestation.verify
    assert isinstance(request.param, str)
    mode = request.param
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

    def verify(arguments: list[str], *, cwd: Path | None = None) -> None:
        assert cwd is None
        wheel = Path(arguments[2])
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
        verified_bundle = Bundle.from_json(
            Path(arguments[arguments.index("--bundle") + 1]).read_text(encoding="utf-8")
        )
        assert (
            Attestation.from_bundle(verified_bundle).envelope.statement
            == source_statements[wheel.name]
        )
        {"signature": reject_signature}.get(mode, accept_signature)(arguments)

    def download_bundle(arguments: list[str], *, cwd: Path | None = None) -> None:
        wheel = Path(arguments[2])
        assert cwd is not None
        raw = copy.deepcopy(recorded_bundle())

        envelope = object_mapping(raw["dsseEnvelope"])

        statement = object_mapping(
            json.loads(base64.b64decode(str(envelope["payload"])))
        )

        statement["subject"] = [
            {
                "name": wheel.name,
                "digest": {
                    "sha256": {"digest": lambda _wheel: "0" * 64}.get(
                        mode, pypi_provenance.sha256
                    )(wheel)
                },
            }
        ]
        payload = json.dumps(statement).encode()
        envelope["payload"] = base64.b64encode(payload).decode()
        raw["dsseEnvelope"] = envelope
        source_statements[wheel.name] = payload
        # gh attestation download writes bare Sigstore bundles, unlike the API.
        line = json.dumps(raw) + "\n"
        prefix = {"aggregate": json.dumps(recorded_bundle()) + "\n"}.get(mode, "")
        (cwd / "download.jsonl").write_text(
            (prefix + line) * {"duplicate": 2}.get(mode, 1)
        )

    def missing_download(_arguments: list[str], *, cwd: Path | None = None) -> None:
        pass

    def github(arguments: list[str], *, cwd: Path | None = None) -> None:
        handlers = {
            "verify": verify,
            "download": {"missing": missing_download}.get(mode, download_bundle),
        }
        handlers[arguments[1]](arguments, cwd=cwd)

    def authenticated(
        _self: Attestation, _publisher: GitHubPublisher, _dist: Distribution
    ) -> tuple[str, None]:
        return "https://slsa.dev/provenance/v1", None

    mocker.patch.object(
        pypi_provenance,
        "run_gh",
        create_autospec(original_run_gh_spec, side_effect=github),
    )
    mocker.patch.object(
        Attestation,
        "verify",
        create_autospec(original_verify_spec, side_effect=authenticated),
    )
    return assets, release_json, source_statements


@pytest.mark.parametrize(
    "stage_environment", ["missing", "digest", "signature"], indirect=True
)
def test_stage_failure_is_atomic(
    tmp_path: Path, stage_environment: tuple[Path, Path, dict[str, bytes]]
) -> None:
    assets, release_json, _source_statements = stage_environment
    output = tmp_path / "dist"

    with pytest.raises(
        (ValueError, subprocess.CalledProcessError), match=r"attestation|non-zero"
    ):
        pypi_provenance.stage("v1.2.3", "a" * 40, assets, release_json, output)

    assert not output.exists()


@pytest.mark.parametrize(
    "stage_environment", ["valid", "duplicate", "aggregate"], indirect=True
)
def test_stage_preserves_signed_statement_bytes(
    tmp_path: Path, stage_environment: tuple[Path, Path, dict[str, bytes]]
) -> None:
    assets, release_json, source_statements = stage_environment
    output = tmp_path / "dist"

    pypi_provenance.stage("v1.2.3", "a" * 40, assets, release_json, output)

    wheels = list(output.glob("*.whl"))
    statements = {
        wheel.name: Attestation.model_validate_json(
            wheel.with_suffix(".whl.slsa.attestation").read_text(encoding="utf-8")
        ).envelope.statement
        for wheel in wheels
    }

    assert len(list(output.iterdir())) == 6
    assert {wheel.name: wheel.read_bytes() for wheel in wheels} == {
        wheel.name: (assets / wheel.name).read_bytes() for wheel in wheels
    }
    assert statements == {wheel.name: source_statements[wheel.name] for wheel in wheels}


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


@pytest.fixture
def verified_index(
    request: pytest.FixtureRequest, mocker: MockerFixture, tmp_path: Path
) -> list[str]:
    original_fetch_json_spec = fetch_json_spec
    original_urlopen_spec = pypi_provenance.urllib.request.urlopen
    original_verify_spec = Attestation.verify
    original_verify_build_spec = pypi_provenance.verify_build
    assert isinstance(request.param, str)
    damage = request.param
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
                    {"host": "http://invalid/"}.get(
                        damage, "https://test-files.pythonhosted.org/packages/"
                    )
                )
                + name,
            }
        )

    def download(_url: str, *, timeout: int) -> io.BytesIO:
        assert timeout > 0
        return io.BytesIO({"bytes": b"modified"}.get(damage, b"wheel"))

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
        {"source": reject_signature}.get(damage, accept_signature)(["gh"])

    mocker.patch.object(
        pypi_provenance,
        "fetch_json",
        create_autospec(
            original_fetch_json_spec,
            side_effect=lambda url: index_data(url, records, template, damage),
        ),
    )
    mocker.patch.object(
        pypi_provenance.urllib.request,
        "urlopen",
        create_autospec(original_urlopen_spec, side_effect=download),
    )
    mocker.patch.object(
        Attestation,
        "verify",
        create_autospec(original_verify_spec, side_effect=signature),
    )
    mocker.patch.object(
        pypi_provenance,
        "verify_build",
        create_autospec(original_verify_build_spec, side_effect=build),
    )
    return verified


@pytest.mark.parametrize("verified_index", ["valid"], indirect=True)
def test_index_verification_checks_downloads_and_both_signatures(
    tmp_path: Path, verified_index: list[str]
) -> None:
    result = pypi_provenance.verify_index("testpypi", "v1.2.3", "a" * 40, tmp_path)

    assert result
    assert len(verified_index) == 6


@pytest.mark.parametrize(
    "verified_index",
    ["bytes", "publisher", "missing", "duplicate", "source", "host"],
    indirect=True,
)
def test_index_verification_rejects_invalid_publication(
    tmp_path: Path, verified_index: list[str]
) -> None:
    assert verified_index == []

    with pytest.raises(
        (ValueError, subprocess.CalledProcessError),
        match=r"differs|publisher|attestation|non-zero|host",
    ):
        pypi_provenance.verify_index("testpypi", "v1.2.3", "a" * 40, tmp_path)


def test_statement_requires_encoded_text() -> None:
    with pytest.raises(TypeError, match="Invalid encoded"):
        pypi_provenance.statement_of({"envelope": {"statement": 1}})


def test_statement_requires_predicate_type() -> None:
    statement = {"subject": [{"name": "wheel", "digest": {"sha256": "digest"}}]}
    value: dict[str, object] = {
        "envelope": {
            "statement": base64.b64encode(json.dumps(statement).encode()).decode()
        }
    }
    with pytest.raises(TypeError, match="Missing predicate"):
        pypi_provenance.statement_types([value], "wheel", "digest")


def test_inventory_requires_file_list() -> None:
    with pytest.raises(TypeError, match="Missing index file inventory"):
        pypi_provenance.check_inventory({}, [])


def test_stage_refuses_existing_output(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Output already exists"):
        pypi_provenance.stage(
            "v1.2.3", "a" * 40, tmp_path, tmp_path / "release", tmp_path
        )


def test_index_requires_stable_tag_and_complete_staging(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_fetch_json_spec = fetch_json_spec
    with pytest.raises(ValueError, match="stable release tag"):
        pypi_provenance.verify_index("pypi", "main", "a" * 40, tmp_path)
    mocker.patch.object(
        pypi_provenance,
        "fetch_json",
        create_autospec(original_fetch_json_spec, side_effect=lambda _: {}),
    )
    with pytest.raises(ValueError, match="staged wheels"):
        pypi_provenance.verify_index("pypi", "v1.2.3", "a" * 40, tmp_path)


def test_fetch_json_reads_index_response(mocker: MockerFixture) -> None:
    original_urlopen_spec = pypi_provenance.urllib.request.urlopen
    mocker.patch.object(
        pypi_provenance.urllib.request,
        "urlopen",
        create_autospec(
            original_urlopen_spec,
            side_effect=lambda _url, **_kwargs: io.BytesIO(b'{"urls": []}'),
        ),
    )
    assert pypi_provenance.fetch_json("https://pypi.org/example") == {"urls": []}


def test_run_gh_requires_cli_and_passes_working_directory(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_which_spec = pypi_provenance.shutil.which
    original_run_spec = pypi_provenance.subprocess.run
    mocker.patch.object(
        pypi_provenance.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: None),
    )
    with pytest.raises(FileNotFoundError, match="GitHub CLI"):
        pypi_provenance.run_gh(["attestation"])
    calls = []
    mocker.patch.object(
        pypi_provenance.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: "/bin/gh"),
    )
    mocker.patch.object(
        pypi_provenance.subprocess,
        "run",
        create_autospec(
            original_run_spec,
            side_effect=lambda args, **kwargs: calls.append((args, kwargs)),
        ),
    )
    pypi_provenance.run_gh(["attestation"], cwd=tmp_path)
    assert calls[0][0] == ["/bin/gh", "attestation"]
    assert calls[0][1]["cwd"] == tmp_path
    assert calls[0][1]["check"] is True


def provenance_arguments(tmp_path: Path, mode: str) -> list[str]:
    return [
        "provenance",
        mode,
        "--tag",
        "v1.2.3",
        "--commit",
        "a" * 40,
        "--directory",
        str(tmp_path),
    ]


def test_provenance_cli_dispatches_staging(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    stage = mocker.patch.object(pypi_provenance, "stage", autospec=True)
    mocker.patch.object(
        sys,
        "argv",
        [
            *provenance_arguments(tmp_path, "stage"),
            "--output",
            str(tmp_path / "dist"),
            "--release-json",
            str(tmp_path / "release.json"),
        ],
    )

    pypi_provenance.main()

    stage.assert_called_once_with(
        "v1.2.3", "a" * 40, tmp_path, tmp_path / "release.json", tmp_path / "dist"
    )


@pytest.mark.parametrize(
    ("mode", "allow_missing", "expected_output"),
    [("inspect", True, "published=true\n"), ("verify", False, "")],
)
def test_provenance_cli_dispatches_verification(
    tmp_path: Path,
    mocker: MockerFixture,
    mode: str,
    expected_output: str,
    *,
    allow_missing: bool,
) -> None:
    output = tmp_path / "github-output"
    output.touch()
    mocker.patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)})
    verify = mocker.patch.object(
        pypi_provenance, "verify_with_propagation", autospec=True, return_value=True
    )
    mocker.patch.object(
        sys, "argv", [*provenance_arguments(tmp_path, mode), "--index", "testpypi"]
    )

    pypi_provenance.main()

    verify.assert_called_once_with(
        "testpypi", "v1.2.3", "a" * 40, tmp_path, allow_missing=allow_missing
    )
    assert output.read_text() == expected_output


@pytest.mark.parametrize("mode", ["stage", "inspect"])
def test_provenance_cli_requires_mode_specific_arguments(
    tmp_path: Path, mocker: MockerFixture, mode: str
) -> None:

    mocker.patch.object(
        sys,
        "argv",
        [
            "provenance",
            mode,
            "--tag",
            "v1.2.3",
            "--commit",
            "a" * 40,
            "--directory",
            str(tmp_path),
        ],
    )
    with pytest.raises(SystemExit) as error:
        pypi_provenance.main()
    assert error.value.code == 2


@pytest.mark.parametrize("damage", ["bundles", "attestations"])
@pytest.mark.parametrize("verified_index", ["valid"], indirect=True)
def test_index_rejects_malformed_provenance_containers(
    tmp_path: Path,
    mocker: MockerFixture,
    verified_index: list[str],
    damage: str,
) -> None:
    original_fetch_json_spec = fetch_json_spec
    original = pypi_provenance.fetch_json

    def fetch(url: str) -> dict[str, object]:
        value = original(url)
        if not url.endswith("/json"):
            if damage == "bundles":
                value["attestation_bundles"] = []
            else:
                bundles = value["attestation_bundles"]
                assert isinstance(bundles, list)
                container = object_mapping(bundles[0])
                container["attestations"] = None
                value["attestation_bundles"] = [container]
        return value

    mocker.patch.object(
        pypi_provenance,
        "fetch_json",
        create_autospec(original_fetch_json_spec, side_effect=fetch),
    )
    with pytest.raises((TypeError, ValueError), match=r"bundle|Missing attestations"):
        pypi_provenance.verify_index("testpypi", "v1.2.3", "a" * 40, tmp_path)
    assert verified_index == []
