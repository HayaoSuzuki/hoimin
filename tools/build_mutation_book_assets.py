"""Build the reproducible download used by the Japanese mutation-testing book."""

from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / "docs/book/examples/shipping"
ARCHIVE = ROOT / "docs/book/assets/shipping-workshop.zip"
FILES = (
    ".gitignore",
    "SPEC.md",
    "pytest.ini",
    "requirements.txt",
    "shipping.py",
    "tests/test_shipping.py",
    "solutions/test_shipping_improved.py",
)


def main() -> None:
    ARCHIVE.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(ARCHIVE, "w", compression=ZIP_DEFLATED) as archive:
        for relative in FILES:
            info = ZipInfo(f"shipping-workshop/{relative}", (2026, 10, 28, 0, 0, 0))
            info.create_system = 3
            info.compress_type = ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, (EXAMPLE / relative).read_bytes())
    print(ARCHIVE)  # noqa: T201 -- CLI reports its generated artifact.


if __name__ == "__main__":
    main()
