"""Verbatim retry function projection; see ../evidence/provenance.json."""
import http
import logging
from pathlib import Path
import time
import urllib.error
LOGGER = logging.getLogger(__name__)
def verify_index(*args, **kwargs):
    raise AssertionError("external verification must be replaced by the test")
PROPAGATION_ATTEMPTS = 61
PROPAGATION_SECONDS = 600
PROPAGATION_INTERVAL = 10
PROPAGATION_MAX_INTERVAL = 60

def verify_with_propagation(
    index: str, tag: str, commit: str, directory: Path, *, allow_missing: bool
) -> bool:
    """Wait only for post-upload 404s; retain every publication verification."""
    deadline = time.monotonic() + PROPAGATION_SECONDS
    attempt = 0
    while True:
        attempt += 1
        try:
            return verify_index(
                index, tag, commit, directory, allow_missing=allow_missing
            )
        except urllib.error.HTTPError as error:
            remaining = deadline - time.monotonic()
            if (
                allow_missing
                or error.code != http.HTTPStatus.NOT_FOUND
                or attempt >= PROPAGATION_ATTEMPTS
                or remaining <= 0
            ):
                raise
            delay = min(
                PROPAGATION_INTERVAL * 2 ** (attempt - 1),
                PROPAGATION_MAX_INTERVAL,
                remaining,
            )
            LOGGER.warning(
                "%s %s publication not yet visible (HTTP 404); "
                "retry %d/%d in %.1fs (%.1fs remaining)",
                index,
                tag,
                attempt,
                PROPAGATION_ATTEMPTS - 1,
                delay,
                remaining,
            )
            time.sleep(delay)
            if time.monotonic() >= deadline:
                raise
