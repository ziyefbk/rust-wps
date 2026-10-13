"""Windows release CLI measurements, not desktop startup or cold-cache results."""
import ctypes
from datetime import datetime, timezone
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def document_text(node):
    return node.get("text", "") + "".join(document_text(child) for child in node.get("content", []))


class MemoryCounters(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in (
            "peak_working_set", "working_set", "peak_paged_pool", "paged_pool",
            "peak_nonpaged_pool", "nonpaged_pool", "commit", "peak_commit",
        )
    ]


def main():
    if os.name != "nt":
        raise SystemExit("This measurement uses Windows process memory counters")
    executable = ROOT / "target/release/rust-wps.exe"
    if not executable.exists():
        raise SystemExit("Run cargo build --release --locked first")
    get_memory = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
    get_memory.restype = wintypes.BOOL
    destination = ROOT / "tmp/benchmarks"
    destination.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="core-", dir=destination))
    inputs = []
    manifest = json.loads((ROOT / "scripts/corpus.json").read_text(encoding="utf-8"))
    for item in manifest["fixtures"]:
        source = ROOT / "tmp/corpus" / item["name"]
        data = source.read_bytes()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise RuntimeError(f"Corpus hash mismatch: {source}")
        inputs.append((source, ["inspect", "import-docx" if source.suffix == ".docx" else "import-doc"], []))
    for paragraphs in (1000, 4000):
        model = directory / f"paragraphs-{paragraphs}.json"
        source = model.with_suffix(".docx")
        model.write_text(json.dumps({"type": "doc", "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": f"段落 {i}: Rust office 中文长文档读取与结构转换性能测试。"}]}
            for i in range(paragraphs)
        ]}, ensure_ascii=False), encoding="utf-8")
        subprocess.run([str(executable), "create-docx", str(model), str(source)], check=True, capture_output=True, timeout=30)
        inputs.append((source, ["inspect", "import-docx"], ["段落 0:", f"段落 {paragraphs - 1}:"]))
    from pypdf import PdfWriter
    from pypdf.generic import DictionaryObject, NameObject, DecodedStreamObject
    writer = PdfWriter()
    font = writer._add_object(DictionaryObject({NameObject("/Type"): NameObject("/Font"), NameObject("/Subtype"): NameObject("/Type1"), NameObject("/BaseFont"): NameObject("/Helvetica")}))
    for index in range(100):
        page = writer.add_blank_page(width=595.28, height=841.89)
        page[NameObject("/Resources")] = DictionaryObject({NameObject("/Font"): DictionaryObject({NameObject("/F1"): font})})
        content = DecodedStreamObject()
        content.set_data(f"BT /F1 12 Tf 60 780 Td (Performance page {index + 1}) Tj ET".encode("ascii"))
        page[NameObject("/Contents")] = writer._add_object(content)
    pdf = directory / "text-100-pages.pdf"
    writer.write(pdf)
    inputs.append((pdf, ["inspect"], ["Performance page 1", "Performance page 100"]))
    results = []
    for source, operations, markers in inputs:
        original = source.read_bytes()
        for operation in operations:
            runs = []
            for _ in range(5):
                started = time.perf_counter()
                process = subprocess.Popen([str(executable), operation, str(source)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=0x08000000)
                try:
                    stdout, stderr = process.communicate(timeout=30)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.communicate()
                    raise RuntimeError(f"Measurement timed out: {source.name} {operation}")
                wall_ms = (time.perf_counter() - started) * 1000
                if process.returncode:
                    raise RuntimeError(stderr.decode("utf-8", errors="replace"))
                counters = MemoryCounters()
                counters.cb = ctypes.sizeof(counters)
                # Popen retains its Windows process handle after communicate.
                if not get_memory(int(process._handle), ctypes.byref(counters), counters.cb):
                    raise ctypes.WinError(ctypes.get_last_error())
                if not counters.peak_working_set or not counters.peak_commit:
                    raise RuntimeError("Unavailable peak memory counters")
                parsed = json.loads(stdout)
                if operation == "inspect" and not parsed["text"].strip():
                    raise RuntimeError("Empty text in performance input")
                if operation != "inspect" and parsed["document"]["type"] != "doc":
                    raise RuntimeError("Invalid import response")
                recovered = parsed["text"] if operation == "inspect" else document_text(parsed["document"])
                if any(marker not in recovered for marker in markers):
                    raise RuntimeError(f"Missing generated-input content marker: {source.name} {operation}; recovered prefix={recovered[:100]!r}")
                runs.append({"wall_ms": round(wall_ms, 3), "inspection_ms": parsed.get("elapsed_ms"), "peak_working_set_bytes": counters.peak_working_set, "peak_commit_bytes": counters.peak_commit, "stdout_bytes": len(stdout)})
            if source.read_bytes() != original:
                raise RuntimeError(f"Source changed: {source}")
            results.append({"input": str(source), "bytes": len(original), "sha256": hashlib.sha256(original).hexdigest(), "operation": operation, "first_process_ms": runs[0]["wall_ms"], "repeat_median_ms": statistics.median(run["wall_ms"] for run in runs[1:]), "max_peak_working_set_bytes": max(run["peak_working_set_bytes"] for run in runs), "runs": runs})
    report = {"measured_at_utc": datetime.now(timezone.utc).isoformat(), "scope": "release CLI only; fresh process per run; OS cache uncontrolled, no cold-start claim; synthetic DOCX/PDF are simple structures, not complex Office fidelity", "platform": platform.platform(), "logical_cpus": os.cpu_count(), "compiler": subprocess.check_output(["rustc", "--version"], text=True).strip(), "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(), "cargo_lock_sha256": hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(), "memory_reference": "https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters", "results": results}
    output = directory / "report.json"
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"report": str(output), "cases": len(results), "summary": [{"input": Path(r["input"]).name, "operation": r["operation"], "repeat_median_ms": r["repeat_median_ms"], "max_peak_working_set_mib": round(r["max_peak_working_set_bytes"] / 1048576, 2)} for r in results]}, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
