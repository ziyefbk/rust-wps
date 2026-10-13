"""Check unchanged, pinned PDF originals with two CLIs and independent readers.

Requires installed pypdf and Poppler's pdftotext. Never downloads, re-saves, or
renders source PDFs. A new output directory is required for every run. Omitting
--candidate records baseline/independent evidence only, not candidate acceptance.
Exact CLI JSON comparison excludes only top-level elapsed_ms; text is not
normalized. Whitespace folding is used solely for explicitly listed evidence
anchors across independent consumers. Known unsupported-encoding panics remain
reported failures of PDF text support, not positive extraction coverage.
"""
import argparse
import hashlib
import io
import json
import logging
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import pypdf


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MANIFEST = ROOT / "scripts/pdf-snapshot-corpus.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def write_json(path, data):
    path.write_text(json.dumps(data, ensure_ascii=True, indent=2) + "\n")


def identity(path):
    data = path.read_bytes()
    return {
        "bytes": len(data),
        "sha256": digest(data),
        "git_blob_sha": hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest(),
    }


def artifact(path, data):
    path.write_bytes(data)
    return {"path": str(path), "bytes": len(data), "sha256": digest(data)}


def run_cli(cli, source, prefix, timeout):
    result = subprocess.run([str(cli), "inspect", str(source)], capture_output=True, timeout=timeout)
    evidence = {
        "returncode": result.returncode,
        "stdout": artifact(prefix.with_suffix(".stdout.json"), result.stdout),
        "stderr": artifact(prefix.with_suffix(".stderr.txt"), result.stderr),
    }
    if result.returncode == 0:
        payload = json.loads(result.stdout)
        evidence["text"] = artifact(prefix.with_suffix(".text.txt"), payload["text"].encode("utf-8"))
        evidence["text_characters"] = len(payload["text"])
    else:
        payload = None
    return evidence, payload, result.stderr.decode("utf-8", errors="replace")


def independent_extract(source, output, timeout):
    log = io.StringIO()
    handler = logging.StreamHandler(log)
    logger = logging.getLogger("pypdf")
    logger.addHandler(handler)
    try:
        reader = pypdf.PdfReader(io.BytesIO(source.read_bytes()))
        pages = [page.extract_text() or "" for page in reader.pages]
    finally:
        logger.removeHandler(handler)
    pypdf_text = "\n".join(pages)
    write_json(output / "pypdf-pages.json", pages)
    result = subprocess.run(
        ["pdftotext", "-enc", "UTF-8", str(source), "-"], capture_output=True, timeout=timeout
    )
    consumers = {
        "pypdf": {
            "pages": len(pages),
            "text": artifact(output / "pypdf.txt", pypdf_text.encode("utf-8")),
            "warnings": artifact(output / "pypdf-warnings.txt", log.getvalue().encode("utf-8")),
            "text_characters": len(pypdf_text),
        },
        "poppler": {
            "returncode": result.returncode,
            "text": artifact(output / "poppler.txt", result.stdout),
            "stderr": artifact(output / "poppler-stderr.txt", result.stderr),
            "text_characters": len(result.stdout.decode("utf-8")),
        },
    }
    check(result.returncode == 0, source.name + ": Poppler extraction failed")
    return consumers, {"pypdf": pypdf_text, "poppler": result.stdout.decode("utf-8")}


def inspect_expected(name, expected, evidence, payload, stderr):
    if expected["status"] == "success":
        check(evidence["returncode"] == 0 and payload is not None, name + ": CLI extraction unexpectedly failed")
        check(payload["format"] == "pdf", name + ": incorrect format")
        check(payload["backend"] == "pdf-extract 0.12.1", name + ": unexpected backend")
        check(isinstance(payload["elapsed_ms"], int), name + ": elapsed_ms missing or not an integer")
    else:
        check(evidence["returncode"] == expected["returncode"], name + ": known encoding failure exit changed")
        check(payload is None and evidence["stdout"]["bytes"] == 0, name + ": known encoding failure unexpectedly returned data")
        check(expected["stderr_contains"] in stderr, name + ": known encoding failure signature changed")
        evidence["known_unsupported_encoding"] = expected["stderr_contains"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, default=ROOT / "tmp/cell-colors-final-cli")
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--corpus-dir", type=Path, default=ROOT / "tmp/pdf-gap-investigation")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, required=True, help="New evidence directory; existing directories are rejected")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    manifest_path, corpus, output = args.manifest.resolve(), args.corpus_dir.resolve(), args.output.resolve()
    manifest = json.loads(manifest_path.read_text())
    baseline = args.baseline.resolve(strict=True)
    candidate = args.candidate.resolve(strict=True) if args.candidate else None
    baseline_hash = digest(baseline.read_bytes())
    check(baseline_hash == manifest["baseline_cli_sha256"], "Frozen baseline CLI identity changed")
    candidate_hash = digest(candidate.read_bytes()) if candidate else None
    check(candidate is None or candidate_hash != baseline_hash, "Candidate and baseline have identical bytes; not a change-validation run")
    check(shutil.which("pdftotext"), "Poppler pdftotext is not installed")
    output.mkdir(parents=True, exist_ok=False)
    report_path = output / "report.json"
    report = {
        "status": "running", "started_utc": datetime.now(timezone.utc).isoformat(),
        "mode": "candidate-comparison" if candidate else "baseline-and-independent-only",
        "method": "Read unchanged pinned originals with frozen CLI, candidate CLI (when supplied), pypdf and pdftotext; no source rewriting or generated PDFs",
        "manifest": {"path": str(manifest_path), "sha256": digest(manifest_path.read_bytes())},
        "script_sha256": digest(Path(__file__).read_bytes()),
        "baseline_cli": {"path": str(baseline), "sha256": baseline_hash},
        "candidate_cli": {"path": str(candidate), "sha256": candidate_hash} if candidate else None,
        "consumers": {"python": platform.python_version(), "pypdf": pypdf.__version__,
            "poppler": subprocess.run(["pdftotext", "-v"], capture_output=True, text=True, timeout=args.timeout).stderr.splitlines()[0]},
        "limits": manifest["limits"], "cases": {}, "source_hashes_unchanged": False,
    }
    shutil.copyfile(__file__, output / Path(__file__).name)
    shutil.copyfile(manifest_path, output / manifest_path.name)
    before = {}
    try:
        for name in ("report.json", "source-snapshot-race.json"):
            source_report = corpus / name
            if source_report.is_file():
                report.setdefault("prior_evidence", {})[name] = {"path": str(source_report), "sha256": digest(source_report.read_bytes())}
        report["product_sources"] = {
            str(path.relative_to(ROOT)): digest(path.read_bytes())
            for path in (ROOT / "src/lib.rs", ROOT / "src/pdf_snapshot.rs", ROOT / "Cargo.toml", ROOT / "Cargo.lock")
            if path.is_file()
        }
        for fixture in manifest["fixtures"]:
            source = corpus / fixture["name"]
            before[fixture["name"]] = identity(source)
            check(before[fixture["name"]] == {key: fixture[key] for key in ("bytes", "sha256", "git_blob_sha")}, fixture["name"] + ": source identity differs from pinned upstream original")
        for fixture in manifest["fixtures"]:
            name, expected = fixture["name"], fixture["expected_cli"]
            source = corpus / name
            case_dir = output / source.stem
            case_dir.mkdir()
            case = report["cases"][name] = {"source": {"path": str(source), **before[name]}, "source_url": fixture["source_url"], "known_limits": fixture.get("known_limits", [])}
            case["baseline"], old, old_stderr = run_cli(baseline, source, case_dir / "baseline", args.timeout)
            inspect_expected(name, expected, case["baseline"], old, old_stderr)
            check(old is None or old["bytes"] == fixture["bytes"], name + ": baseline byte count incorrect")
            texts = {}
            if old is not None:
                texts["baseline"] = old["text"]
            if candidate:
                case["candidate"], new, new_stderr = run_cli(candidate, source, case_dir / "candidate", args.timeout)
                inspect_expected(name, expected, case["candidate"], new, new_stderr)
                if old is not None:
                    check(new["bytes"] == fixture["bytes"], name + ": candidate byte count incorrect")
                    old_comparable = {key: value for key, value in old.items() if key != "elapsed_ms"}
                    new_comparable = {key: value for key, value in new.items() if key != "elapsed_ms"}
                    case["exact_json_equal_excluding_elapsed_ms"] = old_comparable == new_comparable
                    case["exact_text_equal"] = old["text"] == new["text"]
                    check(case["exact_json_equal_excluding_elapsed_ms"], name + ": CLI JSON changed outside elapsed_ms")
                    check(case["exact_text_equal"], name + ": literal CLI text changed")
                    texts["candidate"] = new["text"]
                else:
                    case["known_failure_unchanged"] = True
                    case["failure_comparison_scope"] = "Same nonzero exit, empty stdout and encoding signature; raw stderr is retained but not equated because process identifiers can vary"
            case["independent"], independent_texts = independent_extract(source, case_dir, args.timeout)
            check(case["independent"]["pypdf"]["pages"] == fixture["pages"], name + ": independent page count changed")
            texts.update(independent_texts)
            case["anchors"] = []
            for anchor in fixture["anchors"]:
                counts = {}
                for consumer in anchor["consumers"]:
                    if consumer == "candidate" and not candidate:
                        continue
                    # This is evidence-only whitespace folding, never the CLI parity comparison.
                    text = " ".join(texts[consumer].split()) if anchor.get("whitespace_fold", True) else texts[consumer]
                    counts[consumer] = text.count(anchor["text"])
                    check(counts[consumer] >= 1, name + ": independent anchor missing from " + consumer + ": " + repr(anchor["text"]))
                case["anchors"].append({"text": anchor["text"], "whitespace_fold": anchor.get("whitespace_fold", True), "counts": counts})
            case["source_unchanged_after_consumers"] = identity(source) == before[name]
            check(case["source_unchanged_after_consumers"], name + ": source changed during inspection")
        report["summary"] = {
            "originals_checked": len(report["cases"]),
            "successful_cli_extractions": sum(f["expected_cli"]["status"] == "success" for f in manifest["fixtures"]),
            "known_baseline_encoding_failures": sum(f["expected_cli"]["status"] != "success" for f in manifest["fixtures"]),
            "unchanged_known_encoding_failures": sum(c.get("known_failure_unchanged", False) for c in report["cases"].values()),
            "exact_json_and_text_parity_cases": sum(c.get("exact_json_equal_excluding_elapsed_ms", False) and c.get("exact_text_equal", False) for c in report["cases"].values()),
            "independent_consumers": ["pypdf", "Poppler pdftotext"],
        }
        report["status"] = "passed" if candidate else "baseline-recorded"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        try:
            report["sources_after"] = {name: identity(corpus / name) for name in before}
            report["source_hashes_unchanged"] = report["sources_after"] == before
            check(report["source_hashes_unchanged"], "Original PDF bytes changed")
            check(digest(baseline.read_bytes()) == baseline_hash, "Baseline binary changed during checks")
            check(candidate is None or digest(candidate.read_bytes()) == candidate_hash, "Candidate binary changed during checks")
        except Exception as error:
            report["status"] = "failed"
            report["final_identity_error"] = f"{type(error).__name__}: {error}"
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        write_json(report_path, report)
    print(json.dumps({"status": report["status"], "report": str(report_path), "sha256": digest(report_path.read_bytes()), "summary": report.get("summary"), "error": report.get("error"), "source_hashes_unchanged": report["source_hashes_unchanged"]}, indent=2))
    return 1 if report["status"] == "failed" else 0


if __name__ == "__main__":
    sys.exit(main())
