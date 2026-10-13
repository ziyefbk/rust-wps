"""Independent, bounded evidence for reopenable Jianwen rotation chains.

This is a successor to the frozen check-pdf-rotation.py, whose rejection of every
prior revision describes the first release only. It never edits that checker or
uses product parsing to validate a PDF. The genuine pinned original stays intact.
Every saved intermediate is retained, independently opened with pypdf, compared
object-by-object, and rendered by fresh Poppler processes. Independently generated
pypdf references start from the original and apply the cumulative rotations.
Splash and Cairo are two Poppler paths, not unrelated PDF engines. Direct source
raster transpose is a diagnostic, never silently promoted to the acceptance rule.
No browser, Tauri command, native dialog, WebView or broad PDF support is proved.
"""
import argparse
import hashlib
import importlib.util
import json
import platform
import re
import shutil
import struct
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import PIL
import pypdf
from PIL import Image
from pypdf.generic import (
    ArrayObject, ByteStringObject, DictionaryObject, NameObject, NumberObject,
    TextStringObject,
)

ROOT = Path(__file__).resolve().parent.parent
HELPER = ROOT / "scripts/check-pdf-rotation.py"
FROZEN_HELPER_SHA = "d847a3bc0dd3f0386c4fe2b64fe1434f3193ae070f06379d0c9f22a790999161"
LEGACY_SHA = "45d2ae6d9df6aaa85d325e20ec03c138e03bea341caf6246e31bdb934134c7de"
LEGACY_REPORT_SHA = "3c5fb35cfdab34b3e382e0f788036d49b2aa300283f59314f70026095fd7681d"
HISTORICAL_TRANSPOSE_REPORT_SHA = "12cbfa2426c5f4962e849b01fcc34e736d9f0ed1b0fada6ded2e8ef5ed9d4a9c"
spec = importlib.util.spec_from_file_location("frozen_rotation_evidence", HELPER)
frozen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frozen)
# Reuse only independent consumer/filesystem/rendering utilities, never main(),
# its first-release revision rejection, or anything from the product parser.
require, sha, identity = frozen.require, frozen.sha, frozen.identity
write_json, snapshot, run = frozen.write_json, frozen.snapshot, frozen.run
render_pages, compare_pixels = frozen.render_pages, frozen.compare_pixels
image_comparison, renderer_provenance = frozen.image_comparison, frozen.renderer_provenance


def independent_transition(source_path, destination_path, before, after, selected, directory):
    """Independently check one actual predecessor -> saved-output transition."""
    source, destination = source_path.read_bytes(), destination_path.read_bytes()
    require(destination.startswith(source), "Complete predecessor byte prefix changed")
    suffix = destination[len(source):]
    (directory / "incremental-suffix.bin").write_bytes(suffix)
    selection = before["selections"][selected - 1]
    key = f"{selection['object_id']}:{selection['generation']}"
    target = (selection["rotation"] + 90) % 360
    require(after["objects"].keys() == before["objects"].keys(), "Object inventory changed")
    changed = [key_ for key_ in before["objects"] if before["objects"][key_] != after["objects"][key_]]
    require(changed == [key], f"Expected one selected page object change, got {changed}")
    expected_object = json.loads(json.dumps(before["objects"][key]))
    require(expected_object[0] == "dictionary", "Selected page object is not a dictionary")
    expected_object[1]["/Rotate"] = ["number", str(target)]
    require(after["objects"][key] == expected_object, "Selected page changed outside Rotate")
    require(before["text"] == after["text"], "Per-page pypdf text changed")
    expected_selections = json.loads(json.dumps(before["selections"]))
    expected_selections[selected - 1]["rotation"] = target
    require(after["selections"] == expected_selections, "Page identity, order, count or rotation changed")
    expected_effective = json.loads(json.dumps(before["effective"]))
    expected_effective[selected - 1]["rotation"] = target
    require(after["effective"] == expected_effective, "Page geometry/resources/contents changed")
    require(before["xrefs"].keys() == after["xrefs"].keys(), "Active xref inventory changed")
    changed_offsets = [key_ for key_ in before["xrefs"] if before["xrefs"][key_] != after["xrefs"][key_]]
    require(changed_offsets == [key], f"Unexpected active xref offset changes: {changed_offsets}")
    object_offset = after["xrefs"][key]
    require(len(source) <= object_offset < after["startxref"] < len(destination), "Appended offsets out of bounds")
    require(re.fullmatch(rb"(?:[ \t\r\n]|%[^\r\n]*(?:\r\n|\r|\n))*", destination[len(source):object_offset]),
            "Unaccounted append prefix before replacement object")
    page_span = destination[object_offset:after["startxref"]]
    header = f"{selection['object_id']} 0 obj".encode()
    require(page_span.startswith(header), "Replacement xref does not address selected object header")
    require(re.fullmatch(re.escape(header) + rb"\s*<<.*>>\s*endobj\s*", page_span, re.S), "Unexpected replacement framing")
    require(re.findall(rb"(?m)^\s*(\d+)\s+(\d+)\s+obj\b", suffix) ==
            [(str(selection["object_id"]).encode(), b"0")], "Append contains additional object declarations")
    # Independent exact classic-xref check. This intentionally recognizes only
    # the demonstrated writer's one-page append profile; no repair or scans for
    # alternate xref positions are used to accept a candidate.
    raw_tail = destination[after["startxref"]:]
    xref = re.fullmatch(rb"xref\n0 1\n0000000000 65535 f \n(\d+) 1\n(\d{10}) 00000 n \ntrailer\n(<<.*>>)\nstartxref\n(\d+)\n%%EOF\n?", raw_tail, re.S)
    require(xref is not None, "Appended classic xref/trailer/EOF framing differs from bounded profile")
    require(int(xref[1]) == selection["object_id"] and int(xref[2]) == object_offset,
            "Raw appended xref selected ID/offset differs from independent active map")
    require(int(xref[4]) == after["startxref"], "Raw terminal startxref differs from actual xref location")
    prevs = re.findall(rb"/Prev\s+(\d+)\b", xref[3])
    require(prevs == [str(before["startxref"]).encode()], "Raw Prev must point to immediate predecessor")
    expected_trailer = dict(before["trailer"])
    expected_trailer["/Prev"] = ["number", str(before["startxref"])]
    ids = after["trailer"].get("/ID")
    require(ids and ids[0] == "array" and len(ids[1]) == 2 and
            all(v[0] == "pdf_string_bytes" and len(v[1]) == 32 for v in ids[1]), "Invalid output ID shape")
    old_ids = before["trailer"].get("/ID")
    first_id = old_ids[1][0][1] if old_ids else sha(source)[:32]
    expected_second = hashlib.sha256(b"jianwen-pdf-rotation-id-v1\0" + hashlib.sha256(source).digest() +
                                    struct.pack(">I", selection["object_id"]) + struct.pack(">i", target)).digest()[:16].hex()
    require(ids[1][0][1] == first_id, "Permanent document ID changed")
    require(ids[1][1][1] == expected_second, "Revision ID does not bind exact predecessor/object/rotation")
    require(not old_ids or old_ids[1][1] != ids[1][1], "Revision ID was not changed")
    raw_ids = re.findall(rb"/ID\s*\[\s*<([0-9A-Fa-f]{32})>\s*<([0-9A-Fa-f]{32})>\s*\]", xref[3])
    require([(a.decode().lower(), b.decode().lower()) for a, b in raw_ids] == [(first_id, expected_second)],
            "Raw trailer IDs differ from independently expected values")
    expected_trailer["/ID"] = ids
    require(after["trailer"] == expected_trailer, "Trailer changed outside Prev and revision ID")
    return {"status": "passed", "exact_predecessor_prefix": True, "prefix_bytes": len(source), "prefix_sha256": sha(source),
            "suffix_bytes": len(suffix), "suffix_sha256": sha(suffix), "objects_compared": len(before["objects"]),
            "unchanged_objects": len(before["objects"]) - 1, "changed_objects": changed,
            "streams_encoded_and_decoded_compared": sum(obj[0] == "stream" for obj in before["objects"].values()),
            "changed_active_offsets": changed_offsets, "raw_classic_xref_exact": True, "previous_startxref": before["startxref"],
            "new_startxref": after["startxref"], "page_order_refs_geometry_resources_contents_preserved": True,
            "all_page_pypdf_text_exact": True, "permanent_id": first_id, "revision_id": expected_second,
            "revision_id_bound_to_exact_predecessor": True, "target_rotation": target}


def cumulative_reference(original_path, candidate_path, original, observed, touched, directory, dpi, timeout, splash):
    """Derive each complete reference from the genuine original, not the output."""
    directory.mkdir()
    reference = directory / "original-derived-pypdf-reference.pdf"
    writer = pypdf.PdfWriter(clone_from=original_path)
    operations = []
    for page, (old, current) in enumerate(zip(original["selections"], observed["selections"]), 1):
        delta = (current["rotation"] - old["rotation"]) % 360
        if delta:
            writer.pages[page - 1].rotate(delta)
            operations.append({"page": page, "clockwise_degrees": delta})
    with reference.open("xb") as output:
        writer.write(output)
    report = {"classification": "independent pypdf-rewritten cumulative reference; not genuine original or product output",
              "reference": {"path": str(reference), **identity(reference)}, "generator": {"library": "pypdf", "version": pypdf.__version__,
              "method": "PdfWriter(clone_from=original); rotate each page by independently observed cumulative delta; write(new_file)",
              "operations": operations}, "touched_pages": sorted(touched), "comparisons": []}
    for name, program, flags in (("splash", "pdftoppm", ["-aa", "no", "-aaVector", "no"]), ("cairo", "pdftocairo", [])):
        render_dir = directory / name
        render_dir.mkdir()
        for page in sorted(touched):
            paths, processes = {}, {}
            for label, pdf in (("reference", reference), ("candidate", candidate_path)):
                if name == "splash" and label == "candidate":
                    paths[label] = Path(splash[page - 1]["file"])
                    continue
                prefix = render_dir / f"{label}-page-{page:02d}"
                result, process = run(render_dir / f"{label}-page-{page:02d}-process",
                                      [program, "-r", dpi, *flags, "-f", page, "-l", page, "-singlefile", "-png", pdf, prefix], timeout)
                require(result.returncode == 0, f"{name} reference render failed for page {page}")
                paths[label], processes[label] = prefix.with_suffix(".png"), process
            comparison = image_comparison(paths["reference"], paths["candidate"])
            record = {"consumer": name, "page": page, "fresh_process_per_render": True, "processes": processes, **comparison}
            if name == "splash":
                record["candidate_reused_exact_render"] = str(paths["candidate"])
            report["comparisons"].append(record)
            require(comparison["expected_nonwhite_pixels"] > 1000, f"{name} page {page} unexpectedly blank/sparse")
            require(comparison["exact_equal"], f"{name} page {page} differs from independent cumulative reference")
    reference_text, report["reference_text_process"] = run(directory / "reference-text", ["pdftotext", "-enc", "UTF-8", reference, "-"], timeout)
    actual_text, report["candidate_text_process"] = run(directory / "candidate-text", ["pdftotext", "-enc", "UTF-8", candidate_path, "-"], timeout)
    require(reference_text.returncode == actual_text.returncode == 0, "Cumulative reference text extraction failed")
    report["reference_candidate_poppler_text_exact"] = reference_text.stdout == actual_text.stdout
    require(report["reference_candidate_poppler_text_exact"], "Output text differs from cumulative reference in Poppler")
    report["status"] = "passed"
    write_json(directory / "report.json", report)
    return report


def rejection_derivative(source, path, kind):
    writer = pypdf.PdfWriter(clone_from=source)
    if kind.startswith("encrypted-"):
        writer.encrypt(user_password="" if kind == "encrypted-empty" else "chain-test-password", owner_password="chain-test-owner")
    elif kind == "annotation":
        annotation = DictionaryObject({NameObject("/Type"): NameObject("/Annot"), NameObject("/Subtype"): NameObject("/Text"),
                                       NameObject("/Rect"): ArrayObject([NumberObject(v) for v in (0, 0, 10, 10)]),
                                       NameObject("/Contents"): TextStringObject("rejection-only chain control")})
        writer.pages[0][NameObject("/Annots")] = ArrayObject([writer._add_object(annotation)])
    elif kind == "form":
        writer._root_object[NameObject("/AcroForm")] = writer._add_object(DictionaryObject({NameObject("/Fields"): ArrayObject()}))
    elif kind == "action":
        writer._root_object[NameObject("/OpenAction")] = ArrayObject([writer.pages[0].indirect_reference, NameObject("/Fit")])
    elif kind == "signature-dictionary":
        signature = DictionaryObject({NameObject("/Type"): NameObject("/Sig"),
                                      NameObject("/ByteRange"): ArrayObject([NumberObject(v) for v in (0, 1, 2, 3)]),
                                      NameObject("/Contents"): ByteStringObject(b"not-a-cryptographic-signature")})
        field = DictionaryObject({NameObject("/FT"): NameObject("/Sig"), NameObject("/V"): writer._add_object(signature)})
        writer._root_object[NameObject("/AcroForm")] = writer._add_object(DictionaryObject({NameObject("/Fields"): ArrayObject([writer._add_object(field)])}))
    else:
        raise AssertionError(kind)
    with path.open("xb") as output:
        writer.write(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=ROOT / "tmp/pdf-gap-investigation/tracemonkey.pdf")
    parser.add_argument("--manifest", type=Path, default=ROOT / "scripts/pdf-snapshot-corpus.json")
    parser.add_argument("--legacy-output", type=Path, default=ROOT / "tmp/pdf-rotation-independent-reference-final/rotate-page-01/rotated.pdf")
    parser.add_argument("--legacy-report", type=Path, default=ROOT / "tmp/pdf-rotation-independent-reference-final/report.json")
    parser.add_argument("--historical-transpose-report", type=Path, default=ROOT / "tmp/pdf-rotation-independent-first/report.json")
    parser.add_argument("--provenance", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dpi", type=int, default=72)
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    require(18 <= args.dpi <= 144, "DPI must be within 18..144")
    require(all(shutil.which(p) for p in ("pdftoppm", "pdftocairo", "pdftotext")), "Both Poppler paths and pdftotext required")
    require(sha(HELPER.read_bytes()) == FROZEN_HELPER_SHA, "Frozen predecessor checker changed")
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    original_path, candidate, probe, manifest_path, legacy = [p.resolve(strict=True) for p in
            (args.source, args.candidate, args.probe, args.manifest, args.legacy_output)]
    manifest = json.loads(manifest_path.read_text())
    fixture = next(f for f in manifest["fixtures"] if f["name"] == "tracemonkey.pdf")
    legacy_report_path = args.legacy_report.resolve(strict=True)
    historical_report_path = args.historical_transpose_report.resolve(strict=True)
    protected_paths = [original_path, candidate, probe, manifest_path, legacy, HELPER, Path(__file__).resolve(), legacy_report_path, historical_report_path]
    if args.provenance:
        protected_paths.append(args.provenance.resolve(strict=True))
    protected = {str(path): identity(path) for path in protected_paths}
    report = {"status": "running", "started_utc": datetime.now(timezone.utc).isoformat(), "command": [sys.executable, *sys.argv],
              "cwd": str(Path.cwd()), "source": {"path": str(original_path), **identity(original_path), "source_url": fixture["source_url"],
              "classification": fixture["classification"], "generated_or_resaved": False},
              "candidate": {"path": str(candidate), **identity(candidate)}, "production_probe": {"path": str(probe), **identity(probe)},
              "frozen_predecessor_checker": {"path": str(HELPER), **identity(HELPER), "prior_revision_rejection_superseded_only_for_demonstrated_profile": True},
              "versions": {"python": platform.python_version(), "pypdf": pypdf.__version__, "Pillow": PIL.__version__},
              "render_profile": {"dpi": args.dpi, "splash_text_vector_antialias": False, "cairo_antialias": "default", "fresh_process_per_page": True},
              "renderers": {p: {**renderer_provenance(p), "version": subprocess.run([p, "-v"], capture_output=True, text=True).stderr.splitlines()[0]}
                            for p in ("pdftoppm", "pdftocairo", "pdftotext")},
              "acceptance": "Every intermediate: exact predecessor prefix and independent structural checks; all 13 unselected Splash pages exact to predecessor; all cumulatively touched pages exact to original-derived pypdf reference in Splash and Cairo",
              "limits": ["One genuine unchanged pinned 14-page paper, plus two unchanged rejection originals; not general PDF support",
                         "The paper begins with zero effective page rotations; this evidence does not establish inherited/nonzero-source rotation coverage",
                         "Independent pypdf references are generated derivatives, never counted as genuine originals",
                         "Splash and Cairo share Poppler and are not unrelated PDF engines; exactness only at recorded settings",
                         "Direct predecessor raster transpose is retained as a separate diagnostic and may fail; no threshold tolerance or normalization is used",
                         "Poppler source/output text may reorder or join words after rotation; reference/output comparison and pypdf per-page text are exact",
                         "Standalone production-worker/session/publication probe is not Tauri command, native save dialog, browser, WebView2 or Windows Job acceptance",
                         "Controlled malformed/unsupported derivatives are negative tests; not cryptographic signature validation or proof of every rejection cause",
                         "No OCR, PDF content editing, arbitrary revision compatibility, or whole DOC/DOCX/PDF stage completion"],
              "chains": [], "controls": [], "rejection_sources": [], "protected_before": protected, "failures": []}
    if args.provenance:
        report["build_provenance"] = {"path": str(args.provenance.resolve()), **identity(args.provenance.resolve())}
    for path in (Path(__file__).resolve(), HELPER, manifest_path):
        shutil.copyfile(path, directory / path.name)

    def fail(label, error):
        report["failures"].append({"label": label, "error": f"{type(error).__name__}: {error}"})

    try:
        require(identity(original_path) == {k: fixture[k] for k in ("bytes", "sha256", "git_blob_sha")}, "Pinned original differs from manifest")
        require(identity(legacy)["sha256"] == LEGACY_SHA, "Legacy output differs from frozen published first-release evidence")
        require(identity(legacy_report_path)["sha256"] == LEGACY_REPORT_SHA, "Frozen first-release acceptance report changed")
        require(identity(historical_report_path)["sha256"] == HISTORICAL_TRANSPOSE_REPORT_SHA, "Historical failed strict-transpose report changed")
        legacy_report = json.loads(legacy_report_path.read_text())
        require(legacy_report["status"] == "passed" and legacy_report["cases"][0]["output"]["sha256"] == LEGACY_SHA,
                "Legacy output is not tied to accepted first-release report")
        historical_report = json.loads(historical_report_path.read_text())
        require(historical_report["status"] == "failed", "Historical strict-transpose failure was relabeled")
        historical_diagnostics = []
        for case in historical_report["cases"]:
            page = case["page"]
            expected = case["pixel_comparisons"][page - 1]
            source_render = historical_report["original"]["renders"][page - 1]
            candidate_render_path = Path(case["output"]["path"]).parent / "renders" / f"page-{page:02d}.png"
            recomputed = compare_pixels([source_render], [{"page": page, "file": str(candidate_render_path)}], page)[0]
            require(not expected["equal"] and not recomputed["equal"], "Historical direct transpose failure no longer reproduced")
            require(all(expected[k] == recomputed[k] for k in ("expected_pixel_sha256", "actual_pixel_sha256", "expected_dimensions", "actual_dimensions")),
                    "Historical strict-transpose images differ from recorded failed diagnostics")
            for artifact in (Path(source_render["file"]), candidate_render_path):
                protected[str(artifact)] = identity(artifact)
            historical_diagnostics.append(recomputed)
        report["historical_evidence_verified"] = {
            "accepted_first_release_report": {"path": str(legacy_report_path), **identity(legacy_report_path)},
            "failed_direct_transpose_report": {"path": str(historical_report_path), **identity(historical_report_path)},
            "failed_transpose_diagnostics_recomputed_from_retained_images": historical_diagnostics,
            "rerendered": False, "failure_not_relabeled_as_pass": True}
        original = snapshot(original_path)
        require(len(original["selections"]) == 14, "Genuine paper page count changed")
        write_json(directory / "original-pypdf-snapshot.json", original)
        original_render = render_pages(original_path, directory / "original-splash", 14, args.dpi, args.timeout)
        write_json(directory / "original-splash.json", original_render)
        original_text, report["original_poppler_text"] = run(directory / "original-text", ["pdftotext", "-enc", "UTF-8", original_path, "-"], args.timeout)
        require(original_text.returncode == 0, "Original text extraction failed")
        report["original_objects"] = len(original["objects"])
        report["original_streams"] = sum(obj[0] == "stream" for obj in original["objects"].values())
        legacy_dir = directory / "legacy-input-validation"; legacy_dir.mkdir()
        legacy_snapshot = snapshot(legacy)
        report["legacy_input"] = {"path": str(legacy), **identity(legacy),
                "classification": "actual frozen first-release saved output, not newly generated fixture",
                "historical_report_path": str(legacy_report_path),
                "historical_report_sha256": identity(legacy_report_path)["sha256"],
                "independent_transition": independent_transition(original_path, legacy, original, legacy_snapshot, 1, legacy_dir)}
        # A shared rendering cache avoids re-rendering byte-identical PDFs. The
        # provenance records the actual PDF path of every reused render.
        renders_cache = {identity(original_path)["sha256"]: original_render}
        for name, initial_path, pages in (("same-page-wraparound", original_path, [1, 1, 1, 1]),
                                          ("mixed-pages-revisit-earlier", original_path, [1, 7, 14, 1]),
                                          ("first-release-export-reopen", legacy, [1])):
            chain_dir = directory / name; chain_dir.mkdir()
            chain = {"name": name, "selected_pages": pages, "input": {"path": str(initial_path), **identity(initial_path)}, "steps": []}
            report["chains"].append(chain)
            source_path, previous = initial_path, snapshot(initial_path)
            touched = {p["page"] for p, old in zip(previous["selections"], original["selections"]) if p["rotation"] != old["rotation"]}
            source_hash = identity(source_path)["sha256"]
            if source_hash not in renders_cache:
                renders_cache[source_hash] = render_pages(source_path, chain_dir / "initial-splash", 14, args.dpi, args.timeout)
            previous_renders = renders_cache[source_hash]
            for step_number, selected in enumerate(pages, 1):
                step_dir = chain_dir / f"step-{step_number:02d}-page-{selected:02d}"; step_dir.mkdir()
                destination = step_dir / "saved.pdf"
                step = {"step": step_number, "selected_page": selected, "source": {"path": str(source_path), **identity(source_path)},
                        "selection": previous["selections"][selected - 1]}
                chain["steps"].append(step)
                result, step["cli_process"] = run(step_dir / "cli", [candidate, "rotate-pdf", source_path, destination, selected], args.timeout)
                require(result.returncode == 0 and destination.is_file(), f"{name}/{step_number} save failed: {result.stderr.decode(errors='replace')}")
                step["cli_response"] = json.loads(result.stdout)
                observed = snapshot(destination)
                write_json(step_dir / "pypdf-snapshot.json", observed)
                step["output"] = {"path": str(destination), **identity(destination)}
                protected[str(destination)] = identity(destination)
                step["structure"] = independent_transition(source_path, destination, previous, observed, selected, step_dir)
                require(observed["text"] == original["text"], "Cumulative pypdf per-page text differs from original")
                require(destination.read_bytes().startswith(original_path.read_bytes()), "Cumulative original prefix changed")
                step["exact_original_prefix"] = True
                expected_metadata = {"page": selected, "pages": 14, "rotation": observed["selections"][selected - 1]["rotation"]}
                require(all(step["cli_response"].get(k) == v for k, v in expected_metadata.items()), "CLI metadata differs from independent pypdf selection")
                output_hash = step["output"]["sha256"]
                if output_hash in renders_cache:
                    current_renders = renders_cache[output_hash]
                    step["splash_renders_reused_identical_pdf_sha256"] = output_hash
                else:
                    current_renders = render_pages(destination, step_dir / "splash", 14, args.dpi, args.timeout)
                    renders_cache[output_hash] = current_renders
                write_json(step_dir / "splash-renders.json", current_renders)
                comparisons = compare_pixels(previous_renders, current_renders, selected)
                step["predecessor_pixel_comparisons"] = comparisons
                write_json(step_dir / "predecessor-pixel-comparisons.json", comparisons)
                require(all(c["equal"] for c in comparisons if c["page"] != selected), "Unselected predecessor page pixels changed")
                step["direct_predecessor_transpose_diagnostic"] = "passed" if comparisons[selected - 1]["equal"] else "failed"
                touched.add(selected)
                step["cumulative_reference"] = cumulative_reference(original_path, destination, original, observed, touched,
                        step_dir / "independent-cumulative-reference", args.dpi, args.timeout, current_renders)
                reference_path = Path(step["cumulative_reference"]["reference"]["path"])
                protected[str(reference_path)] = identity(reference_path)
                step["never_selected_original_pixels_exact"] = []
                for page in sorted(set(range(1, 15)) - touched):
                    comparison = image_comparison(original_render[page - 1]["file"], current_renders[page - 1]["file"])
                    require(comparison["exact_equal"], "Never-selected page differs from original")
                    step["never_selected_original_pixels_exact"].append(page)
                candidate_text = Path(step["cumulative_reference"]["candidate_text_process"]["stdout_file"]).read_bytes()
                step["poppler_original_text_exact"] = candidate_text == original_text.stdout
                # Reopen through the actual production Open worker, create a new
                # session, rotate through production worker + parent publication.
                # Byte identity lets the exact same independent structural/pixel
                # evidence apply without duplicating all image renders.
                published = step_dir / "production-published.pdf"
                publish, step["publication_process"] = run(step_dir / "production-publish", [probe, "publish", source_path,
                        published, json.dumps(step["selection"])], args.timeout)
                require(publish.returncode == 0, f"Production publish failed: {publish.stderr.decode(errors='replace')}")
                response = json.loads(publish.stdout)
                require(response["session_source_unchanged"], "Save changed current source session")
                require(response["saved"] == {"path": str(published), **expected_metadata}, "Production publication metadata mismatch")
                require(published.read_bytes() == destination.read_bytes(), "Production publication differs from CLI")
                require(not list(step_dir.glob(".rust-wps-rotation-*")), "Publication leaked temporary stage")
                protected[str(published)] = identity(published)
                step["production_publication"] = {"response": response, "output": {"path": str(published), **identity(published)},
                    "exact_cli_bytes": True, "real_worker_open_rotate_and_parent_publish": True, "source_session_unchanged": True}
                step["status"] = "passed"
                # The next operation actually reopens the saved bytes; no
                # in-memory PDF model or original selection is reused.
                source_path, previous, previous_renders = published, snapshot(published), current_renders
            chain["final_rotations"] = [p["rotation"] for p in previous["selections"]]
            if name == "same-page-wraparound":
                require(chain["final_rotations"] == [p["rotation"] for p in original["selections"]], "Four turns did not restore original orientation")
                chain["full_turn_original_pixel_comparisons"] = []
                for page in range(1, 15):
                    comp = image_comparison(original_render[page - 1]["file"], previous_renders[page - 1]["file"])
                    require(comp["exact_equal"], f"Four-turn image differs from original page {page}")
                    chain["full_turn_original_pixel_comparisons"].append({"page": page, **comp})
                chain["four_turn_visual_roundtrip_exact"] = True
            chain["status"] = "passed"

        controls_dir = directory / "controls"; controls_dir.mkdir()
        chain_source = Path(report["chains"][0]["steps"][1]["output"]["path"])
        chain_bytes = chain_source.read_bytes()
        chain_snapshot = snapshot(chain_source)
        selection = chain_snapshot["selections"][0]

        def rejected(label, command, destination=None, preserve=()):
            saved = {str(path): identity(path) for path in preserve}
            process, evidence = run(controls_dir / label, command, args.timeout)
            entry = {"label": label, "process": evidence}
            report["controls"].append(entry)
            require(process.returncode != 0, f"{label} unexpectedly accepted")
            require(destination is None or not destination.exists(), f"{label} published output on failure")
            require({str(path): identity(path) for path in preserve} == saved, f"{label} changed protected source/destination")
            entry.update({"status": "passed", "protected_files": saved, "no_output_on_failure": destination is not None})

        sentinel = controls_dir / "existing.pdf"; sentinel.write_bytes(b"preserve-existing-chain-destination\n")
        rejected("existing-destination", [candidate, "rotate-pdf", chain_source, sentinel, 1], preserve=(chain_source, sentinel))
        rejected("same-source-destination", [candidate, "rotate-pdf", chain_source, chain_source, 1], preserve=(chain_source,))
        for link_kind in ("hardlink", "symlink"):
            link = controls_dir / f"{link_kind}.pdf"
            link.hardlink_to(chain_source) if link_kind == "hardlink" else link.symlink_to(chain_source)
            rejected(f"{link_kind}-source-destination", [candidate, "rotate-pdf", chain_source, link, 1], preserve=(chain_source, link))
        for page in (0, 15):
            destination = controls_dir / f"invalid-page-{page}.pdf"
            rejected(f"invalid-page-{page}", [candidate, "rotate-pdf", chain_source, destination, page], destination, (chain_source,))
        for name in ("basicapi.pdf", "rotation.pdf"):
            fixture_ = next(f for f in manifest["fixtures"] if f["name"] == name)
            path = original_path.parent / name
            expected = {k: fixture_[k] for k in ("bytes", "sha256", "git_blob_sha")}
            require(identity(path) == expected, f"Unsupported genuine original {name} changed")
            protected[str(path)] = expected
            destination = controls_dir / f"rejected-{name}"
            rejected(f"genuine-unsupported-{name}", [candidate, "rotate-pdf", path, destination, 1], destination, (path,))
            report["rejection_sources"].append({"path": str(path), **expected, "source_url": fixture_["source_url"],
                    "classification": fixture_["classification"], "generated_or_resaved": False})
        for kind in ("encrypted-empty", "encrypted-nonempty", "annotation", "form", "action", "signature-dictionary"):
            path = controls_dir / f"controlled-{kind}.pdf"
            rejection_derivative(original_path, path, kind)
            destination = controls_dir / f"rejected-controlled-{kind}.pdf"
            rejected(f"controlled-{kind}", [candidate, "rotate-pdf", path, destination, 1], destination, (path,))
            report["rejection_sources"].append({"path": str(path), **identity(path), "generated_or_resaved": True,
                    "classification": "pypdf-rewritten rejection-only derivative", "feature": kind,
                    "limit": "May include other unsupported rewrite features; no isolated causal or valid cryptographic-signature claim"})

        # Same-length mutations keep later object and xref offsets intact. They
        # test the bounded chain proof rather than merely making a corrupt tail.
        second_step = report["chains"][0]["steps"][1]
        predecessor_start = second_step["structure"]["prefix_bytes"]
        before_tail, tail = chain_bytes[:predecessor_start], chain_bytes[predecessor_start:]
        def tail_replace(pattern, replacement):
            mutated, count = re.subn(pattern, replacement, tail, count=1)
            require(count == 1, f"Mutation pattern not unique/found: {pattern}")
            require(len(mutated) == len(tail), "Intended same-length mutation changed offsets")
            return before_tail + mutated
        def padded_prev(value):
            return lambda match: match[1] + str(value).zfill(len(match[2])).encode()
        mutations = {
            "prev-self-cycle": tail_replace(rb"(/Prev\s+)(\d+)", padded_prev(chain_snapshot["startxref"])),
            "prev-skip-predecessor": tail_replace(rb"(/Prev\s+)(\d+)", padded_prev(original["startxref"])),
            "prev-zero": tail_replace(rb"(/Prev\s+)(\d+)", padded_prev(0)),
            "rotate-not-next-quarter-turn": tail_replace(rb"/Rotate 180", b"/Rotate 270"),
            "geometry-changed": tail_replace(rb"/MediaBox\[0 0 612 792\]", b"/MediaBox[0 0 613 792]"),
            "revision-id-tampered": tail_replace(rb"(/ID\[<[0-9A-Fa-f]{32}><)([0-9A-Fa-f])", lambda m: m[1] + (b"0" if m[2] != b"0" else b"1")),
            "permanent-id-tampered": tail_replace(rb"(/ID\[<)([0-9A-Fa-f])", lambda m: m[1] + (b"0" if m[2] != b"0" else b"1")),
            "truncated-final-eof": chain_bytes[:-4],
            "trailing-nonwhitespace": chain_bytes + b"unproved-tail",
        }
        # Alter an earlier revision's Rotate without changing any offsets: a
        # latest-only validator would miss history tampering that this profile
        # must reject even if the final active page has a plausible orientation.
        historical = chain_bytes[:predecessor_start]
        marker = historical.rfind(b"/Rotate 90")
        require(marker >= len(original_path.read_bytes()), "Prior appended rotation not found")
        mutations["earlier-revision-rotate-tampered"] = chain_bytes[:marker] + b"/Rotate 00" + chain_bytes[marker + len(b"/Rotate 90"):]
        for label, data in mutations.items():
            path = controls_dir / f"tampered-{label}.pdf"; path.write_bytes(data)
            destination = controls_dir / f"rejected-tampered-{label}.pdf"
            rejected(f"tampered-{label}", [candidate, "rotate-pdf", path, destination, 1], destination, (path,))
            report["controls"][-1]["derivation"] = {"source": str(chain_source), "source_sha256": sha(chain_bytes),
                    "mutant": {"path": str(path), **identity(path)}, "same_length": len(data) == len(chain_bytes), "generated_negative_control": True}
        for field, bad in (("page", 2), ("pages", 15), ("object_id", 3), ("generation", 1), ("rotation", 0)):
            destination = controls_dir / f"wrong-{field}.pdf"
            wrong = {**selection, field: bad}
            rejected(f"wrong-{field}-on-reopened-chain", [probe, "rotate", chain_source, destination, json.dumps(wrong)], destination, (chain_source,))
        rejected("production-existing-destination", [probe, "publish", chain_source, sentinel, json.dumps(selection)], preserve=(chain_source, sentinel))
        rejected("production-source-destination", [probe, "publish", chain_source, chain_source, json.dumps(selection)], preserve=(chain_source,))
        stale = controls_dir / "stale-publication.pdf"
        rejected("production-stale-session", [probe, "publish-stale", chain_source, stale, json.dumps(selection)], stale, (chain_source,))
        cancelled, evidence = run(controls_dir / "production-cancel", [probe, "cancel", chain_source, json.dumps(selection)], args.timeout)
        require(cancelled.returncode == 0, "Production cancellation failed")
        cancelled_json = json.loads(cancelled.stdout)
        require(cancelled_json["saved"] is None and cancelled_json["session_source_unchanged"], "Production cancellation changed session or saved output")
        report["controls"].append({"label": "production-cancel", "status": "passed", "process": evidence})
        require(not list(controls_dir.glob(".rust-wps-rotation-*")), "Rejected/cancelled publication leaked stages")
        worker_dir = directory / "raw-worker-reopened-chain"; worker_dir.mkdir()
        stage = worker_dir / "stage"; stage.mkdir()
        pdf_identity = {"bytes": len(chain_bytes), "sha256": list(hashlib.sha256(chain_bytes).digest())}
        request = {"operation": "rotate_pdf", "path": str(chain_source), "identity": pdf_identity,
                   "selection": selection, "staging_directory": str(stage)}
        worker, evidence = run(worker_dir / "worker", [probe, "request", json.dumps(request)], args.timeout)
        require(worker.returncode == 0, "Raw production worker rejected reopened chain")
        response = json.loads(worker.stdout)
        require(response.get("result") == "pdf_rotated", "Raw worker result mismatch")
        metadata = response["candidate"]
        require(metadata["file_name"] == "rotated.pdf", "Raw worker candidate filename violated contract")
        staged = stage / metadata["file_name"]
        expected_cli = Path(report["chains"][0]["steps"][2]["output"]["path"])
        require(staged.read_bytes() == expected_cli.read_bytes(), "Raw worker output differs from independently checked third turn")
        require(metadata["bytes"] == staged.stat().st_size and bytes(metadata["sha256"]).hex() == identity(staged)["sha256"], "Raw worker candidate identity mismatch")
        require((metadata["page"], metadata["pages"], metadata["rotation"]) == (1, 14, 270), "Raw worker candidate selection mismatch")
        report["raw_worker"] = {"status": "passed", "process": evidence, "response": response,
                               "candidate": {"path": str(staged), **identity(staged)}, "exact_independently_checked_cli_bytes": True}
        for label, override in (("worker-wrong-identity", {"identity": {**pdf_identity, "bytes": len(chain_bytes) + 1}}),
                                ("worker-stale-selection", {"selection": {**selection, "rotation": 90}})):
            stage_ = worker_dir / label; stage_.mkdir()
            rejected(label, [probe, "request", json.dumps({**request, **override, "staging_directory": str(stage_)})], preserve=(chain_source,))
            require(not list(stage_.iterdir()), "Rejected worker left candidate")
        altered = controls_dir / "worker-altered-chain-source.pdf"
        altered.write_bytes(chain_bytes[:-1] + (b" " if chain_bytes[-1:] != b" " else b"\n"))
        altered_stage = worker_dir / "altered-source-stage"; altered_stage.mkdir()
        rejected("worker-source-identity-changed", [probe, "request", json.dumps({**request, "path": str(altered),
                 "staging_directory": str(altered_stage)})], preserve=(altered,))
        require(not list(altered_stage.iterdir()), "Changed-identity worker left candidate")
    except Exception as error:
        fail("checker", error)
    finally:
        report["protected_after"] = {path: identity(Path(path)) for path in protected}
        report["protected_unchanged"] = report["protected_after"] == protected
        if not report["protected_unchanged"]:
            fail("protected-identities", AssertionError("Protected original, binary, manifest or checker changed"))
        steps = [step for chain in report["chains"] for step in chain["steps"]]
        report["summary"] = {"genuine_success_originals": 1, "legacy_first_release_outputs_reopened": sum(c["name"] == "first-release-export-reopen" and c.get("status") == "passed" for c in report["chains"]),
                "chains_passed": sum(c.get("status") == "passed" for c in report["chains"]), "intermediate_saves_passed": sum(s.get("status") == "passed" for s in steps),
                "production_publications_exact_cli_bytes": sum(s.get("production_publication", {}).get("exact_cli_bytes", False) for s in steps),
                "unselected_predecessor_pages_exact": sum(c["equal"] and c["page"] != s["selected_page"] for s in steps for c in s.get("predecessor_pixel_comparisons", [])),
                "never_selected_original_pages_exact": sum(len(s.get("never_selected_original_pixels_exact", [])) for s in steps),
                "touched_page_reference_matches": sum(c["exact_equal"] for s in steps for c in s.get("cumulative_reference", {}).get("comparisons", [])),
                "generated_cumulative_reference_pdfs": sum("cumulative_reference" in s for s in steps),
                "direct_predecessor_transpose_failures_retained": sum(s.get("direct_predecessor_transpose_diagnostic") == "failed" for s in steps),
                "full_turn_all_page_pixel_matches": sum(len(c.get("full_turn_original_pixel_comparisons", [])) for c in report["chains"]),
                "controls_passed": sum(c.get("status") == "passed" for c in report["controls"]),
                "genuine_unsupported_originals": sum(not s["generated_or_resaved"] for s in report["rejection_sources"]),
                "raw_worker": report.get("raw_worker", {}).get("status", "not-run")}
        report["status"] = "failed" if report["failures"] else "passed"
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        report["evidence_bytes"] = sum(path.stat().st_size for path in directory.rglob("*") if path.is_file())
        write_json(directory / "report.json", report)
    print(json.dumps({"status": report["status"], "report": str(directory / "report.json"),
                      "report_sha256": sha((directory / "report.json").read_bytes()), "summary": report["summary"], "failures": report["failures"]}, indent=2))
    return int(report["status"] != "passed")


if __name__ == "__main__":
    sys.exit(main())
