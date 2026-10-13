"""Rebuild the local office_oxide DOCX patch from the verified crate archive."""

import difflib
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor" / "office_oxide"
MANIFEST = ROOT / "vendor" / "office_oxide-upstream-files.json"
OUTPUT = ROOT / "vendor" / "office_oxide-docx-typography.patch"
FILES = (
    "src/convert_docx.rs",
    "src/core/theme.rs",
    "src/docx/formatting.rs",
    "src/docx/mod.rs",
    "src/docx/styles.rs",
)


def main() -> None:
    candidates = list((Path.home() / ".cargo" / "registry" / "src").glob("*/office_oxide-0.1.13"))
    if len(candidates) != 1:
        raise RuntimeError(f"expected one office_oxide 0.1.13 crate archive, found {len(candidates)}")
    upstream = candidates[0]
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8-sig"))
    for entry in manifest:
        original = upstream / entry["path"]
        if hashlib.sha256(original.read_bytes()).hexdigest() != entry["sha256"]:
            raise RuntimeError(f"upstream crate does not match manifest: {entry['path']}")

    patches = []
    for relative in FILES:
        original = (upstream / relative).read_text(encoding="utf-8").splitlines(keepends=True)
        changed = (VENDOR / relative).read_text(encoding="utf-8").splitlines(keepends=True)
        patches.extend(difflib.unified_diff(
            original, changed, fromfile=f"a/{relative}", tofile=f"b/{relative}", lineterm="\n",
        ))
    with OUTPUT.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write("".join(patches))
    print(f"verified {len(manifest)} upstream files; wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
