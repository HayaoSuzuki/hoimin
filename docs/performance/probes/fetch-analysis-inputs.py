"""Fetch two fixed source files with gh; never import or execute their code."""
import base64
import hashlib
import json
import subprocess
from pathlib import Path

destination = Path(".superpowers/runtime-improvements/analysis-inputs")
sources = [
    ("proofreadtools", "HayaoSuzuki/proofreadtools", "409ed955c0695ec04976842ba9fd22c64c03c33a", "src/proofreadtools/checks/_variant_candidates.py"),
    ("django", "django/django", "dab0a5c47f159eb58472110dcbc10bd4a9b01adb", "django/db/models/base.py"),
]
inventory = []
for name, repository, revision, filename in sources:
    response = subprocess.run(
        ["gh", "api", f"repos/{repository}/contents/{filename}?ref={revision}"],
        capture_output=True, check=True, timeout=60,
    )
    data = json.loads(response.stdout)
    assert data["encoding"] == "base64" and data["size"] < 1_048_576
    source = base64.b64decode(data["content"])
    assert len(source) == data["size"]
    directory = destination / name
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "source.py").write_bytes(source)
    inventory.append(dict(name=name, repository=repository, revision=revision,
                          path=filename, bytes=len(source), sha256=hashlib.sha256(source).hexdigest()))
generated = "\n".join(f"def f_{i}(x):\n    return x + {i} if x > {i} else x - {i}\n" for i in range(1_000))
directory = destination / "generated"
directory.mkdir(parents=True, exist_ok=True)
source = generated.encode()
(directory / "source.py").write_bytes(source)
inventory.append(dict(name="generated", bytes=len(source), sha256=hashlib.sha256(source).hexdigest()))
Path("docs/performance/runtime-2026-10-12/analysis-inputs.json").write_text(json.dumps(inventory,indent=2), encoding="utf-8")
print(json.dumps(inventory,indent=2))
