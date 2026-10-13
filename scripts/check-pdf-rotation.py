"""Independently check the bounded PDF rotation export using an unchanged paper.

Requires pypdf, Pillow and Poppler. Reads the pinned tracemonkey.pdf original,
never rewrites it, and requires a new evidence directory. Invokes the compiled
CLI and an optional standalone transport through the actual production worker.
Each page is rendered by a fresh Poppler process, at bounded DPI with antialiasing
disabled so exact integer-pixel clockwise transformation is an explicit oracle.
No native/browser acceptance is claimed. Any failure and its artifacts remain.
The default strict source-transpose oracle is retained. The explicit alternative
--visual-oracle independent-reference compares against separately pypdf-rotated
references with both Poppler Splash and Cairo, retaining source-transpose failures
as diagnostics rather than representing them as passes.
"""
import argparse
import hashlib
import io
import json
import platform
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import PIL
import pypdf
from PIL import Image, ImageChops
from pypdf.generic import (
    ArrayObject, BooleanObject, ByteStringObject, DictionaryObject,
    FloatObject, IndirectObject, NameObject, NullObject, NumberObject,
    StreamObject, TextStringObject,
)

ROOT = Path(__file__).resolve().parent.parent


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def identity(path):
    h = hashlib.sha256()
    size = path.stat().st_size
    git = hashlib.sha1(b"blob " + str(size).encode() + b"\0")
    read = 0
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
            git.update(block)
            read += len(block)
    require(size == read, f"File changed while hashing: {path}")
    return {"bytes": size, "sha256": h.hexdigest(), "git_blob_sha": git.hexdigest()}


def flat_pixels(image):
    return image.get_flattened_data() if hasattr(image, "get_flattened_data") else image.getdata()


def write_json(path, value):
    with path.open("x") as f:
        json.dump(value, f, indent=2, ensure_ascii=True)
        f.write("\n")


def canonical(obj):
    """Typed direct-object representation; references retain IDs, never recurse."""
    if isinstance(obj, IndirectObject):
        return ["reference", obj.idnum, obj.generation]
    if isinstance(obj, StreamObject):
        return ["stream", canonical_dict(obj), {"encoded_bytes": len(obj._data),
            "encoded_sha256": sha(obj._data), "decoded_bytes": len(obj.get_data()),
            "decoded_sha256": sha(obj.get_data())}]
    if isinstance(obj, DictionaryObject):
        return ["dictionary", canonical_dict(obj)]
    if isinstance(obj, ArrayObject):
        return ["array", [canonical(value) for value in obj]]
    if isinstance(obj, NameObject):
        return ["name", str(obj)]
    if isinstance(obj, (TextStringObject, ByteStringObject)):
        return ["pdf_string_bytes", bytes(obj.original_bytes).hex()]
    if isinstance(obj, BooleanObject):
        return ["boolean", obj.value]
    if isinstance(obj, NullObject):
        return ["null"]
    if isinstance(obj, (NumberObject, FloatObject)):
        return ["number", str(obj)]
    raise TypeError(f"Unsupported pypdf value: {type(obj).__name__}")


def canonical_dict(obj):
    return {str(key): canonical(obj.raw_get(key)) for key in sorted(obj)}


def snapshot(path):
    reader = pypdf.PdfReader(io.BytesIO(path.read_bytes()), strict=True)
    require(not reader.is_encrypted, "Fixture unexpectedly encrypted")
    require(not reader.xref_objStm, "This evidence profile requires classic xrefs")
    # Capture unflattened dictionaries before reader.pages adds inherited values.
    objects, xrefs = {}, {}
    for generation, entries in sorted(reader.xref.items()):
        for object_id, offset in sorted(entries.items()):
            if object_id == 0:
                continue
            key = f"{object_id}:{generation}"
            objects[key] = canonical(reader.get_object(IndirectObject(object_id, generation, reader)))
            xrefs[key] = offset
    trailer = canonical_dict(reader.trailer)
    pages = list(reader.pages)
    text = [page.extract_text() or "" for page in pages]
    selections = []
    effective = []
    for index, page in enumerate(pages):
        ref = page.indirect_reference
        selections.append({"page": index + 1, "pages": len(pages), "object_id": ref.idnum,
            "generation": ref.generation, "rotation": page.rotation % 360})
        effective.append({"rotation": page.rotation % 360,
            "media_box": [str(v) for v in page.mediabox],
            "crop_box": [str(v) for v in page.cropbox],
            "resources": canonical(page.raw_get("/Resources")) if "/Resources" in page else None,
            "contents": canonical(page.raw_get("/Contents")) if "/Contents" in page else None})
    return {"objects": objects, "xrefs": xrefs, "trailer": trailer, "selections": selections,
        "text": text, "effective": effective, "startxref": reader._startxref}


def run(prefix, command, timeout):
    result = subprocess.run([str(arg) for arg in command], capture_output=True, timeout=timeout)
    prefix.with_suffix(".stdout").write_bytes(result.stdout)
    prefix.with_suffix(".stderr.txt").write_bytes(result.stderr)
    return result, {"command": [str(arg) for arg in command], "returncode": result.returncode,
        "stdout_file": str(prefix.with_suffix(".stdout")), "stdout_sha256": sha(result.stdout),
        "stderr_file": str(prefix.with_suffix(".stderr.txt")), "stderr_sha256": sha(result.stderr)}


def compare_structure(source, output, original, destination, selection, directory):
    data = output.read_bytes()
    require(data.startswith(source), "Output did not preserve the complete original byte prefix")
    suffix = data[len(source):]
    (directory / "incremental-suffix.bin").write_bytes(suffix)
    key = f"{selection['object_id']}:{selection['generation']}"
    require(original["objects"].keys() == destination["objects"].keys(), "Object inventory changed")
    differences = [k for k in original["objects"] if original["objects"][k] != destination["objects"][k]]
    require(differences == [key], f"Changed objects are {differences}, expected only {key}")
    expected = json.loads(json.dumps(original["objects"][key]))
    require(expected[0] == "dictionary", "Selected page is not a direct dictionary")
    expected[1]["/Rotate"] = ["number", str((selection["rotation"] + 90) % 360)]
    require(destination["objects"][key] == expected, "Selected page changed outside /Rotate")
    require(destination["xrefs"].keys() == original["xrefs"].keys(), "Xref inventory changed")
    xref_changes = [k for k in original["xrefs"] if original["xrefs"][k] != destination["xrefs"][k]]
    require(xref_changes == [key], f"Unexpected xref offset changes: {xref_changes}")
    require(destination["xrefs"][key] >= len(source), "Selected replacement object points into old bytes")
    require(destination["startxref"] >= len(source), "New startxref does not point to appended bytes")
    expected_trailer = dict(original["trailer"])
    expected_trailer["/Prev"] = ["number", str(original["startxref"])]
    id_revision = None
    if "/ID" in original["trailer"]:
        old_id = original["trailer"]["/ID"]
        new_id = destination["trailer"].get("/ID")
        require(old_id[0] == "array" and len(old_id[1]) == 2, "Original document ID shape unexpected")
        require(new_id and new_id[0] == "array" and len(new_id[1]) == 2, "Output document ID shape invalid")
        require(all(value[0] == "pdf_string_bytes" and len(value[1]) == 32 for value in old_id[1] + new_id[1]), "Document IDs must remain 16-byte strings")
        require(new_id[1][0] == old_id[1][0], "Permanent document ID changed")
        require(new_id[1][1] != old_id[1][1], "Incremental revision document ID was not updated")
        raw_ids = re.findall(rb"/ID\s*\[\s*<([0-9A-Fa-f]{32})>\s*<([0-9A-Fa-f]{32})>\s*\]", suffix)
        require(raw_ids == [(new_id[1][0][1].encode(), new_id[1][1][1].encode())] or
                [(a.lower(), b.lower()) for a, b in raw_ids] == [(new_id[1][0][1].encode(), new_id[1][1][1].encode())],
                "Appended trailer IDs must be exactly two 16-byte hexadecimal strings")
        expected_trailer["/ID"] = new_id
        id_revision = {"permanent_id_preserved": True, "old_revision_id": old_id[1][1][1], "new_revision_id": new_id[1][1][1]}
    require(destination["trailer"] == expected_trailer, "Trailer changed outside /Prev and revision ID[1]")
    declarations = re.findall(rb"(?m)^\s*(\d+)\s+(\d+)\s+obj\b", suffix)
    require(declarations == [(str(selection["object_id"]).encode(), str(selection["generation"]).encode())],
            f"Unexpected appended object declarations: {declarations}")
    require(destination["text"] == original["text"], "pypdf page text changed")
    require(len(destination["selections"]) == selection["pages"], "Page count changed")
    for index, (old, new) in enumerate(zip(original["effective"], destination["effective"])):
        expected_page = dict(old)
        if index + 1 == selection["page"]:
            expected_page["rotation"] = (old["rotation"] + 90) % 360
        require(new == expected_page, f"Effective page properties changed unexpectedly: page {index + 1}")
    streams = sum(value[0] == "stream" for value in original["objects"].values())
    return {"exact_source_prefix": True, "prefix_sha256": sha(data[:len(source)]),
        "suffix_bytes": len(suffix), "suffix_sha256": sha(suffix), "objects_compared": len(original["objects"]),
        "unchanged_objects": len(original["objects"]) - 1, "changed_objects": differences,
        "streams_compared_encoded_and_decoded": streams, "changed_xref_offsets": xref_changes,
        "trailer_only_prev_and_revision_id_changed": True, "document_id_revision": id_revision, "all_page_text_exact": True,
        "all_effective_boxes_resources_contents_preserved": True}


def render_pages(source, directory, count, dpi, timeout):
    directory.mkdir()
    evidence = []
    for page in range(1, count + 1):
        prefix = directory / f"page-{page:02d}"
        # Separate process for every page, never use another renderer as baseline.
        result, command = run(directory / f"render-{page:02d}", ["pdftoppm", "-f", page, "-l", page,
            "-r", dpi, "-aa", "no", "-aaVector", "no", "-singlefile", "-png", source, prefix], timeout)
        require(result.returncode == 0, f"Poppler render failed: {source}, page {page}")
        path = prefix.with_suffix(".png")
        with Image.open(path) as image:
            image.load()
            evidence.append({"page": page, "file": str(path), "dimensions": list(image.size),
                "mode": image.mode, "pixel_sha256": sha(image.tobytes()), "png_sha256": sha(path.read_bytes()),
                "fresh_process": True, "process": command})
    return evidence


def compare_pixels(original, rotated, selected):
    evidence = []
    for old, new in zip(original, rotated):
        with Image.open(old["file"]) as baseline, Image.open(new["file"]) as output:
            baseline.load(); output.load()
            expected = baseline.transpose(Image.Transpose.ROTATE_270) if old["page"] == selected else baseline
            dimensions_equal = expected.size == output.size and expected.mode == output.mode
            equal = dimensions_equal and expected.tobytes() == output.tobytes()
            bbox = None
            different_pixels = 0 if equal else None
            maximum_channel_delta = 0 if equal else None
            if dimensions_equal and not equal:
                difference = ImageChops.difference(expected, output)
                bbox = difference.getbbox()
                pixels = difference.get_flattened_data() if hasattr(difference, "get_flattened_data") else difference.getdata()
                different_pixels = sum(any(value) if isinstance(value, tuple) else bool(value) for value in pixels)
                maximum_channel_delta = max(extreme[1] for extreme in difference.convert("RGB").getextrema())
            evidence.append({"page": old["page"], "expected": "exact_clockwise_90_transpose" if old["page"] == selected else "exact_unchanged_pixels",
                "equal": equal, "expected_dimensions": list(expected.size), "actual_dimensions": list(output.size),
                "expected_pixel_sha256": sha(expected.tobytes()), "actual_pixel_sha256": sha(output.tobytes()),
                "difference_bbox": bbox, "different_pixels": different_pixels,
                "total_pixels": expected.width * expected.height, "maximum_channel_delta": maximum_channel_delta})
    return evidence


def inverse_viewport_diagnostic(source, output, selected, directory, dpi, aa=0):
    """A separate diagnostic, never substituted for the strict Poppler oracle."""
    import fitz
    protected_before = {str(path): identity(path) for path in (source, output)}
    fitz.TOOLS.set_aa_level(aa)
    source_document = fitz.open(source)
    output_document = fitz.open(output)
    scale = dpi / 72
    source_page = source_document[selected - 1]
    output_page = output_document[selected - 1]
    original = source_page.get_pixmap(matrix=fitz.Matrix(scale, scale), colorspace=fitz.csRGB, alpha=False)
    inverse = output_page.get_pixmap(matrix=fitz.Matrix(scale, scale).prerotate(-90), colorspace=fitz.csRGB, alpha=False)
    native = output_page.get_pixmap(matrix=fitz.Matrix(scale, scale), colorspace=fitz.csRGB, alpha=False)
    directory.mkdir()
    original.save(directory / "source.png")
    inverse.save(directory / "output-inverse-viewport.png")
    native.save(directory / "output-native.png")
    before = Image.frombytes("RGB", (original.width, original.height), original.samples)
    after = Image.frombytes("RGB", (inverse.width, inverse.height), inverse.samples)
    require(before.size == after.size, "Inverse viewport has different dimensions")
    delta = ImageChops.difference(before, after)
    pixels = delta.get_flattened_data() if hasattr(delta, "get_flattened_data") else delta.getdata()
    result = {"role": "diagnostic_only_not_strict_source_transpose_acceptance", "renderer": fitz.__doc__,
        "source_path": str(source), "candidate_path": str(output), "selected_page": selected,
        "matrix_method": "source.get_pixmap(Matrix(dpi/72,dpi/72)); candidate.get_pixmap(Matrix(dpi/72,dpi/72).prerotate(-90))",
        "mode": "RGB", "alpha": False, "protected_before": protected_before,
        "dpi": dpi, "antialiasing": fitz.TOOLS.show_aa_level(), "output_viewport_degrees": -90,
        "source_rotation": source_page.rotation, "output_rotation": output_page.rotation,
        "source_dimensions": [original.width, original.height], "inverse_dimensions": [inverse.width, inverse.height],
        "native_output_dimensions": [native.width, native.height],
        "exact_equal": original.samples == inverse.samples,
        "different_pixels": sum(any(value) for value in pixels), "difference_bbox": delta.getbbox(),
        "maximum_channel_delta": max(extreme[1] for extreme in delta.getextrema()),
        "source_pixel_sha256": sha(original.samples), "inverse_pixel_sha256": sha(inverse.samples),
        "source_nonwhite_pixels": sum(any(v != 255 for v in value) for value in flat_pixels(before)),
        "pdf_rewritten": False}
    result["protected_after"] = {str(path): identity(path) for path in (source, output)}
    result["protected_unchanged"] = result["protected_after"] == protected_before
    require(result["protected_unchanged"], "Inverse viewport diagnostic changed a PDF")
    write_json(directory / "report.json", result)
    return result



def image_comparison(expected_path, actual_path):
    with Image.open(expected_path) as expected, Image.open(actual_path) as actual:
        expected.load(); actual.load()
        require(expected.size == actual.size and expected.mode == actual.mode, "Reference/candidate image dimensions or modes differ")
        difference = ImageChops.difference(expected, actual)
        pixels = difference.get_flattened_data() if hasattr(difference, "get_flattened_data") else difference.getdata()
        return {"exact_equal": expected.tobytes() == actual.tobytes(), "dimensions": list(expected.size), "mode": expected.mode,
            "different_pixels": sum(any(value) if isinstance(value, tuple) else bool(value) for value in pixels),
            "maximum_channel_delta": max(value[1] for value in difference.convert("RGB").getextrema()),
            "difference_bbox": difference.getbbox(), "expected_file": str(expected_path), "actual_file": str(actual_path),
            "expected_pixel_sha256": sha(expected.tobytes()), "actual_pixel_sha256": sha(actual.tobytes()),
            "expected_nonwhite_pixels": sum(any(channel != 255 for channel in value) for value in flat_pixels(expected.convert("RGB")))}


def renderer_provenance(program):
    launcher = Path(shutil.which(program)).resolve()
    result = {"launcher_path": str(launcher), "launcher_identity": identity(launcher)}
    if launcher.stat().st_size < 4096:
        script = launcher.read_text(errors="replace")
        match = re.search(r'exec "\$\{SCRIPT_DIR\}/([^"\n]+)" "\$@"', script)
        if match:
            executable = (launcher.parent / match.group(1)).resolve(strict=True)
            result["underlying_executable_path"] = str(executable)
            result["underlying_executable_identity"] = identity(executable)
    return result


def reference_acceptance(source, candidate, selected, directory, dpi, timeout, existing_splash_render):
    directory.mkdir()
    protected_before = {str(path): identity(path) for path in (source, candidate)}
    reference = directory / "pypdf-rotated-reference.pdf"
    writer = pypdf.PdfWriter(clone_from=source)
    writer.pages[selected - 1].rotate(90)
    with reference.open("xb") as destination:
        writer.write(destination)
    report = {"role": "independently_generated_reference_not_genuine_source_or_product_output", "reference": {"path": str(reference), **identity(reference)},
        "source": str(source), "candidate": str(candidate), "selected_page": selected,
        "generator": {"library": "pypdf", "version": pypdf.__version__,
            "method": "PdfWriter(clone_from=source); writer.pages[selected - 1].rotate(90); writer.write(new_reference_file)"},
        "consumers": {}}
    for name, program, antialias in (("splash", "pdftoppm", ["-aa", "no", "-aaVector", "no"]), ("cairo", "pdftocairo", [])):
        render_dir = directory / name; render_dir.mkdir()
        paths = {}
        consumer = {"version": subprocess.run([program, "-v"], capture_output=True, text=True).stderr.splitlines()[0],
            "program_path": shutil.which(program), "program_identity": identity(Path(shutil.which(program)).resolve()),
            "executable_provenance": renderer_provenance(program),
            "dpi": dpi, "antialias": "text/vector disabled" if name == "splash" else "Cairo default", "processes": {}}
        for label, path in (("reference", reference), ("candidate", candidate)):
            if name == "splash" and label == "candidate":
                paths[label] = existing_splash_render
                consumer["candidate_reused_exact_render_file"] = str(existing_splash_render)
                continue
            prefix = render_dir / label
            result, evidence = run(render_dir / f"{label}-process", [program, "-r", dpi, *antialias, "-f", selected,
                "-l", selected, "-singlefile", "-png", path, prefix], timeout)
            require(result.returncode == 0, f"Independent reference {program} failed")
            consumer["processes"][label] = evidence
            paths[label] = prefix.with_suffix(".png")
        consumer["comparison"] = image_comparison(paths["reference"], paths["candidate"])
        require(consumer["comparison"]["expected_nonwhite_pixels"] > 1000, "Reference render is blank or unexpectedly sparse")
        require(consumer["comparison"]["exact_equal"], f"Candidate differs from independent rotated reference in {name}")
        report["consumers"][name] = consumer
    reference_text, text_evidence = run(directory / "reference-text", ["pdftotext", "-enc", "UTF-8", reference, "-"], timeout)
    candidate_text, candidate_evidence = run(directory / "candidate-text", ["pdftotext", "-enc", "UTF-8", candidate, "-"], timeout)
    require(reference_text.returncode == candidate_text.returncode == 0, "Reference text extraction failed")
    report["poppler_reference_text_exact"] = reference_text.stdout == candidate_text.stdout
    report["poppler_reference_text"] = text_evidence; report["poppler_candidate_text"] = candidate_evidence
    require(report["poppler_reference_text_exact"], "Candidate Poppler text differs from independently rotated reference")
    report["protected_before"] = protected_before
    report["protected_after"] = {str(path): identity(path) for path in (source, candidate)}
    report["source_and_candidate_unchanged"] = report["protected_after"] == protected_before
    require(report["source_and_candidate_unchanged"], "Reference generation/rendering changed source or candidate")
    report["status"] = "passed"
    write_json(directory / "report.json", report)
    return report

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--library", type=Path)
    parser.add_argument("--provenance", type=Path)
    parser.add_argument("--source", type=Path, default=ROOT / "tmp/pdf-gap-investigation/tracemonkey.pdf")
    parser.add_argument("--manifest", type=Path, default=ROOT / "scripts/pdf-snapshot-corpus.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--visual-oracle", choices=["strict-transpose", "independent-reference"], default="strict-transpose")
    parser.add_argument("--dpi", type=int, default=72)
    parser.add_argument("--diagnose-inverse-viewport", action="store_true", help="Also report optional installed MuPDF inverse-viewport diagnostic; never replaces strict Poppler oracle")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    require(18 <= args.dpi <= 144, "DPI must be bounded to 18..144")
    require(shutil.which("pdftoppm") and shutil.which("pdftotext"), "Poppler is required")
    if args.visual_oracle == "independent-reference":
        require(shutil.which("pdftocairo"), "The reference criterion requires Poppler Cairo")
    output = args.output.resolve(); output.mkdir(parents=True, exist_ok=False)
    source_path = args.source.resolve(strict=True)
    candidate = args.candidate.resolve(strict=True)
    probe = args.probe.resolve(strict=True) if args.probe else None
    manifest_path = args.manifest.resolve(strict=True)
    manifest = json.loads(manifest_path.read_text())
    fixture = next(f for f in manifest["fixtures"] if f["name"] == "tracemonkey.pdf")
    source_identity = identity(source_path)
    protected = {str(path): identity(path) for path in (source_path, candidate, manifest_path,
        Path(__file__).resolve(), *([probe] if probe else []), *([args.library.resolve(strict=True)] if args.library else []))}
    report = {"status": "running", "started_utc": datetime.now(timezone.utc).isoformat(),
        "checker_command": [sys.executable, *sys.argv], "checker_cwd": str(Path.cwd()),
        "source": {"path": str(source_path), **source_identity, "source_url": fixture["source_url"],
            "classification": fixture["classification"], "generated_or_resaved": False},
        "candidate": {"path": str(candidate), **identity(candidate)},
        "probe": {"path": str(probe), **identity(probe)} if probe else None,
        "consumer_versions": {"python": platform.python_version(), "pypdf": pypdf.__version__, "Pillow": PIL.__version__,
            "poppler": subprocess.run(["pdftoppm", "-v"], capture_output=True, text=True).stderr.splitlines()[0]},
        "visual_acceptance_criterion": args.visual_oracle,
        "criterion_description": ("Exact candidate/reference pixels in both Poppler Splash and Cairo for all 3 selected pages; 39 untouched pages exact; independent object/stream/prefix/text/session controls. Direct source-transpose remains a separately failed diagnostic." if args.visual_oracle == "independent-reference" else "Every output page equals untouched source or exact clockwise source pixel transpose"),
        "render_profile": {"dpi": args.dpi, "antialias_text": False, "antialias_vector": False, "fresh_process_per_page": True},
        "limits": ["One unchanged pinned 14-page published paper; not whole PDF format coverage",
            "This genuine paper has zero effective source rotation; inherited/nonzero-rotation edge cases need separate coverage",
            "Rendering at the reported bounded DPI and antialias settings does not establish all-renderer/all-resolution fidelity",
            "Standalone production-worker transport is not Tauri command, save-dialog, IPC, browser or native WebView acceptance",
            "Malformed-selection and altered-source controls use isolated copies or request metadata, never alter pinned originals",
            "No OCR, PDF content editing, annotations, encryption/signature support, or whole DOC/DOCX/PDF phase completion is established"],
        "cases": [], "controls": [], "protected_before": protected, "failures": []}
    shutil.copyfile(__file__, output / Path(__file__).name)
    shutil.copyfile(manifest_path, output / manifest_path.name)
    if args.provenance:
        report["build_provenance"] = {"path": str(args.provenance.resolve(strict=True)), **identity(args.provenance.resolve(strict=True))}
    def fail(label, error):
        report["failures"].append({"label": label, "error": f"{type(error).__name__}: {error}"})
    try:
        require(source_identity == {key: fixture[key] for key in ("bytes", "sha256", "git_blob_sha")}, "Source differs from pinned genuine original")
        source = source_path.read_bytes()
        original = snapshot(source_path)
        require(len(original["selections"]) == fixture["pages"] == 14, "Unexpected original page count")
        write_json(output / "original-pypdf-snapshot.json", original)
        original_render = render_pages(source_path, output / "original-render", 14, args.dpi, args.timeout)
        write_json(output / "original-render.json", original_render)
        poppler_text, poppler_text_evidence = run(output / "original-text", ["pdftotext", "-enc", "UTF-8", source_path, "-"], args.timeout)
        require(poppler_text.returncode == 0, "Original Poppler text failed")
        report["original"] = {"objects": len(original["objects"]), "stream_count": sum(v[0] == "stream" for v in original["objects"].values()),
            "pages": 14, "effective_source_rotations": [p["rotation"] for p in original["selections"]],
            "renders": original_render, "poppler_text": poppler_text_evidence}
        saved_outputs = []
        for page in (1, 7, 14):
            case_dir = output / f"rotate-page-{page:02d}"; case_dir.mkdir()
            destination = case_dir / "rotated.pdf"
            case = {"page": page, "selection": original["selections"][page-1]}; report["cases"].append(case)
            try:
                result, case["cli"] = run(case_dir / "cli", [candidate, "rotate-pdf", source_path, destination, page], args.timeout)
                require(result.returncode == 0, f"CLI rotation failed: {result.stderr.decode(errors='replace')}")
                require(destination.is_file(), "CLI did not publish output")
                saved_outputs.append(destination)
                case["output"] = {"path": str(destination), **identity(destination)}
                failures_before = len(report["failures"])
                try:
                    observed = snapshot(destination)
                    write_json(case_dir / "pypdf-snapshot.json", observed)
                    case["structure"] = compare_structure(source, destination, original, observed, case["selection"], case_dir)
                except Exception as error:
                    fail(f"rotate-page-{page}-structure", error)
                try:
                    text, case["poppler_text"] = run(case_dir / "text", ["pdftotext", "-enc", "UTF-8", destination, "-"], args.timeout)
                    require(text.returncode == 0, "Rotated output Poppler text failed")
                    case["poppler_text_exact"] = text.stdout == poppler_text.stdout
                    case["poppler_source_text_status"] = "exact" if case["poppler_text_exact"] else "changed_after_rotation"
                    if args.visual_oracle == "strict-transpose":
                        require(case["poppler_text_exact"], "Poppler text changed")
                except Exception as error:
                    fail(f"rotate-page-{page}-text", error)
                try:
                    renders = render_pages(destination, case_dir / "renders", 14, args.dpi, args.timeout)
                    write_json(case_dir / "renders.json", renders)
                    case["pixel_comparisons"] = compare_pixels(original_render, renders, page)
                    write_json(case_dir / "pixel-comparisons.json", case["pixel_comparisons"])
                    case["direct_source_transpose_status"] = "passed" if case["pixel_comparisons"][page - 1]["equal"] else "failed"
                    require(all(p["equal"] for p in case["pixel_comparisons"] if p["page"] != page), "Untouched-page exact Poppler pixels changed")
                    if args.visual_oracle == "strict-transpose":
                        require(all(p["equal"] for p in case["pixel_comparisons"]), "Exact Poppler pixel oracle failed")
                except Exception as error:
                    fail(f"rotate-page-{page}-pixels", error)
                if args.visual_oracle == "independent-reference":
                    try:
                        case["reference_acceptance"] = reference_acceptance(source_path, destination, page, case_dir / "independent-reference", args.dpi, args.timeout, case_dir / "renders" / f"page-{page:02d}.png")
                    except Exception as error:
                        fail(f"rotate-page-{page}-reference", error)
                if args.diagnose_inverse_viewport:
                    try:
                        case["inverse_viewport_diagnostic"] = {
                            "no_antialias": inverse_viewport_diagnostic(source_path, destination, page, case_dir / "inverse-viewport-no-aa", args.dpi, 0),
                            "default_antialias": inverse_viewport_diagnostic(source_path, destination, page, case_dir / "inverse-viewport-default-aa", args.dpi, 8)}
                    except Exception as error:
                        case["inverse_viewport_diagnostic"] = {"status": "failed", "error": str(error)}
                case["status"] = "passed" if len(report["failures"]) == failures_before else "failed"
            except Exception as error:
                case["status"] = "failed"; fail(f"rotate-page-{page}", error)
        control_dir = output / "controls"; control_dir.mkdir()
        def rejected(label, command, destination=None, preserved=None):
            result, evidence = run(control_dir / label, command, args.timeout)
            record = {"label": label, "process": evidence}; report["controls"].append(record)
            require(result.returncode != 0, f"{label}: unexpected success")
            if destination is not None:
                require(not destination.exists(), f"{label}: destination created on failure")
            if preserved is not None:
                require(preserved[0].read_bytes() == preserved[1], f"{label}: existing bytes changed")
            record["status"] = "passed"
        sentinel = control_dir / "existing.pdf"; sentinel.write_bytes(b"existing destination: preserve exactly\n")
        rejected("existing-destination", [candidate, "rotate-pdf", source_path, sentinel, 1], preserved=(sentinel, sentinel.read_bytes()))
        rejected("same-source-destination", [candidate, "rotate-pdf", source_path, source_path, 1], preserved=(source_path, source))
        for page in (0, 15):
            dest = control_dir / f"invalid-page-{page}.pdf"
            rejected(f"invalid-page-{page}", [candidate, "rotate-pdf", source_path, dest, page], destination=dest)
        if saved_outputs:
            dest = control_dir / "prior-revision-output.pdf"
            rejected("prior-revision", [candidate, "rotate-pdf", saved_outputs[0], dest, 1], destination=dest)
        report["rejection_sources"] = []
        for fixture_name in ("basicapi.pdf", "rotation.pdf"):
            known = next(item for item in manifest["fixtures"] if item["name"] == fixture_name)
            reject_source = source_path.parent / fixture_name
            known_identity = identity(reject_source)
            require(known_identity == {key: known[key] for key in ("bytes", "sha256", "git_blob_sha")}, f"Rejection original {fixture_name} is not pinned")
            protected[str(reject_source)] = known_identity
            dest = control_dir / f"rejected-{fixture_name}"
            rejected(f"unchanged-original-{reject_source.stem}", [candidate, "rotate-pdf", reject_source, dest, 1], destination=dest)
            report["rejection_sources"].append({"path": str(reject_source), **known_identity,
                "classification": known["classification"], "source_url": known["source_url"], "generated_or_resaved": False})
        for label, password in (("encrypted-empty-password", ""), ("encrypted-nonempty-password", "rotation-test-password")):
            controlled = control_dir / f"{label}.pdf"
            writer = pypdf.PdfWriter(clone_from=source_path)
            writer.encrypt(user_password=password, owner_password="rotation-test-owner")
            with controlled.open("xb") as encrypted_output:
                writer.write(encrypted_output)
            require(pypdf.PdfReader(controlled).is_encrypted, "Controlled encryption fixture is not encrypted")
            controlled_identity = identity(controlled)
            dest = control_dir / f"{label}-output.pdf"
            rejected(label, [candidate, "rotate-pdf", controlled, dest, 1], destination=dest)
            require(identity(controlled) == controlled_identity, "Rejected encrypted source changed")
            report["rejection_sources"].append({"path": str(controlled), **controlled_identity,
                "classification": "pypdf-rewritten encrypted rejection-only derivative of pinned paper", "generated_or_resaved": True,
                "method": "PdfWriter(clone_from=source); encrypt(user_password=empty_or_test_password, owner_password=test_owner); write(new_file)",
                "password_kind": "empty" if not password else "nonempty_test_password", "source_unchanged": True})
        for label in ("annotation", "form", "action", "signature-dictionary"):
            controlled = control_dir / f"controlled-{label}.pdf"
            writer = pypdf.PdfWriter(clone_from=source_path)
            if label == "annotation":
                annotation = DictionaryObject({NameObject("/Type"): NameObject("/Annot"), NameObject("/Subtype"): NameObject("/Text"),
                    NameObject("/Rect"): ArrayObject([NumberObject(value) for value in (0, 0, 10, 10)]), NameObject("/Contents"): TextStringObject("rejection test")})
                writer.pages[0][NameObject("/Annots")] = ArrayObject([writer._add_object(annotation)])
            elif label == "form":
                writer._root_object[NameObject("/AcroForm")] = writer._add_object(DictionaryObject({NameObject("/Fields"): ArrayObject()}))
            elif label == "action":
                writer._root_object[NameObject("/OpenAction")] = ArrayObject([writer.pages[0].indirect_reference, NameObject("/Fit")])
            else:
                signature = DictionaryObject({NameObject("/Type"): NameObject("/Sig"), NameObject("/ByteRange"): ArrayObject([NumberObject(value) for value in (0, 1, 2, 3)]),
                    NameObject("/Contents"): ByteStringObject(b"not-a-cryptographic-signature")})
                field = DictionaryObject({NameObject("/FT"): NameObject("/Sig"), NameObject("/T"): TextStringObject("rejection test"), NameObject("/V"): writer._add_object(signature)})
                writer._root_object[NameObject("/AcroForm")] = writer._add_object(DictionaryObject({NameObject("/Fields"): ArrayObject([writer._add_object(field)])}))
            with controlled.open("xb") as controlled_output:
                writer.write(controlled_output)
            controlled_identity = identity(controlled)
            dest = control_dir / f"controlled-{label}-output.pdf"
            rejected(f"controlled-{label}", [candidate, "rotate-pdf", controlled, dest, 1], destination=dest)
            require(identity(controlled) == controlled_identity, "Controlled rejected source changed")
            report["rejection_sources"].append({"path": str(controlled), **controlled_identity,
                "classification": "controlled pypdf-rewritten rejection-only derivative of pinned paper", "generated_or_resaved": True,
                "injected_feature": label, "cryptographic_signature_claim": False, "source_unchanged": True,
                "limit": "May contain other unsupported features from reference rewriting; rejection is not an isolated causal proof"})
        if probe:
            selection = original["selections"][0]
            for field, bad in (("page",2),("pages",15),("object_id",3),("generation",1),("rotation",90)):
                wrong = {**selection, field: bad}
                dest = control_dir / f"wrong-{field}.pdf"
                rejected(f"wrong-{field}", [probe, "rotate", source_path, dest, json.dumps(wrong)], destination=dest)
            pdf_identity = {"bytes": len(source), "sha256": list(hashlib.sha256(source).digest())}
            worker_dir = output / "worker"; worker_dir.mkdir()
            stage = worker_dir / "stage"; stage.mkdir()
            request = {"operation":"rotate_pdf", "path": str(source_path), "identity":pdf_identity,
                "selection":selection, "staging_directory":str(stage)}
            result, evidence = run(worker_dir / "rotate", [probe,"request",json.dumps(request)],args.timeout)
            report["worker"] = {"process":evidence}
            require(result.returncode == 0, "Production worker rotation failed")
            response = json.loads(result.stdout); report["worker"]["response"] = response
            require(response.get("result") == "pdf_rotated", "Worker did not return pdf_rotated")
            staged = response["candidate"]
            require(staged["file_name"] == "rotated.pdf", "Worker candidate filename violated fixed contract")
            staged_path = stage / staged["file_name"]
            require(staged_path.parent == stage and staged_path.is_file(), "Worker candidate path escaped stage or missing")
            staged_identity = identity(staged_path)
            require(staged["bytes"] == staged_identity["bytes"], "Worker candidate byte count wrong")
            worker_sha = bytes(staged["sha256"]).hex() if isinstance(staged["sha256"], list) else staged["sha256"]
            require(worker_sha == staged_identity["sha256"], "Worker candidate SHA-256 wrong")
            require(staged["page"] == 1 and staged["pages"] == 14 and staged["rotation"] == 90, "Worker rotation metadata wrong")
            require(saved_outputs and staged_path.read_bytes() == saved_outputs[0].read_bytes(), "Worker/core CLI bytes differ")
            report["worker"].update({"candidate": {"path":str(staged_path), **staged_identity}, "exact_cli_output_bytes":True, "status":"passed"})
            publish_dir = output / "production-publish"; publish_dir.mkdir()
            published = publish_dir / "published.pdf"
            result, publication = run(publish_dir / "publish", [probe,"publish",source_path,published,json.dumps(selection)],args.timeout)
            require(result.returncode == 0,"Production session/worker/publication failed")
            publication["response"] = json.loads(result.stdout)
            require(publication["response"]["session_source_unchanged"],"Publication changed current source session")
            require(publication["response"]["saved"] == {"path":str(published),"page":1,"pages":14,"rotation":90},"Publication metadata wrong")
            require(published.read_bytes() == saved_outputs[0].read_bytes(),"Production publication bytes differ from CLI")
            require(not list(publish_dir.glob(".rust-wps-rotation-*")),"Successful publication leaked stage")
            publication["exact_cli_output_bytes"] = True
            publication["status"] = "passed"
            report["production_publication"] = publication
            result, cancellation = run(publish_dir / "cancel",[probe,"cancel",source_path,json.dumps(selection)],args.timeout)
            require(result.returncode == 0,"Cancel operation failed")
            cancelled = json.loads(result.stdout)
            require(cancelled["saved"] is None and cancelled["session_source_unchanged"],"Cancel changed session or saved output")
            report["controls"].append({"label":"production-cancel","status":"passed","process":cancellation})
            rejected("production-existing-destination",[probe,"publish",source_path,published,json.dumps(selection)],preserved=(published,published.read_bytes()))
            rejected("production-source-destination",[probe,"publish",source_path,source_path,json.dumps(selection)],preserved=(source_path,source))
            stale_output = publish_dir / "stale.pdf"
            rejected("production-stale-session",[probe,"publish-stale",source_path,stale_output,json.dumps(selection)],destination=stale_output)
            require(not list(publish_dir.glob(".rust-wps-rotation-*")),"Rejected/cancelled publication leaked stage")
            for label, changed in (("worker-wrong-identity",{"identity": {**pdf_identity,"bytes":len(source)+1}}),
                                   ("worker-wrong-selection",{"selection":{**selection,"object_id":3}})):
                rejected_stage = worker_dir / label; rejected_stage.mkdir()
                req = {**request, **changed,"staging_directory":str(rejected_stage)}
                rejected(label,[probe,"request",json.dumps(req)])
                require(not list(rejected_stage.iterdir()),f"{label}: failed worker left staged output")
            altered = control_dir / "altered-source.pdf"
            altered.write_bytes(source[:-1] + (b" " if source[-1:] != b" " else b"\n"))
            altered_before = identity(altered)
            rejected_stage = worker_dir / "changed-source-stage"; rejected_stage.mkdir()
            req = {**request,"path":str(altered),"staging_directory":str(rejected_stage)}
            rejected("worker-source-changed",[probe,"request",json.dumps(req)])
            require(not list(rejected_stage.iterdir()),"Changed source worker left staged output")
            require(identity(altered) == altered_before,"Worker changed rejected source")
        else:
            report["worker"] = {"status":"not-run", "reason":"No compiled production-worker probe supplied"}
    except Exception as error:
        fail("checker",error)
    finally:
        report["protected_after"] = {path: identity(Path(path)) for path in protected}
        report["protected_unchanged"] = report["protected_after"] == protected
        report["source_hashes_unchanged"] = report["protected_after"][str(source_path)] == source_identity
        if not report["protected_unchanged"]:
            fail("identity",AssertionError("Original/protected artifact changed during validation"))
        report["status"] = "failed" if report["failures"] else "passed"
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        report["summary"] = {"genuine_originals":1,"synthetic_pdf_success_cases":0,
            "generated_independent_reference_pdfs":sum("reference_acceptance" in c for c in report["cases"]),
            "direct_source_transpose_failures_retained":sum(c.get("direct_source_transpose_status")=="failed" for c in report["cases"]),
            "successful_rotation_cases":sum(c.get("status")=="passed" for c in report["cases"]),
            "controls_passed":sum(c.get("status")=="passed" for c in report["controls"]),
            "all_page_exact_pixel_cases":sum(p["equal"] for c in report["cases"] for p in c.get("pixel_comparisons",[])),
            "untouched_pages_exact":sum(p["equal"] and p["page"] != c["page"] for c in report["cases"] for p in c.get("pixel_comparisons",[])),
            "selected_reference_pixel_matches":sum(reader["comparison"]["exact_equal"] for c in report["cases"] for reader in c.get("reference_acceptance",{}).get("consumers",{}).values()),
            "unchanged_rejection_originals":sum(not item["generated_or_resaved"] for item in report.get("rejection_sources",[])),
            "controlled_rejection_derivatives":sum(item["generated_or_resaved"] for item in report.get("rejection_sources",[])),
            "worker":report.get("worker",{}).get("status","not-run")}
        write_json(output / "report.json", report)
    print(json.dumps({"status":report["status"],"report":str(output / "report.json"),
        "report_sha256":sha((output / "report.json").read_bytes()),"summary":report["summary"],"failures":report["failures"]},indent=2))
    return int(report["status"] != "passed")


if __name__ == "__main__":
    sys.exit(main())
