"""Independent Decimal arithmetic checks against the real bounded Rust CLI.

This is a numerical corpus check, not an Excel/WPS compatibility oracle.
"""
import decimal
import hashlib
import json
import math
import pathlib
import random
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
exe = root / "target/debug/rust-wps.exe"
directory = pathlib.Path(tempfile.mkdtemp(prefix="calculation-", dir=root / "tmp"))
rng = random.Random(1701)
cells, expected = [], []
for row in range(200):
    a, b, c = [rng.randint(-1000, 1000) for _ in range(3)]
    d = rng.randint(1, 1000)
    formula = f"({a}+{b})*({c})/{d}"
    cells.append({"row": row, "column": 1, "content": {"type": "formula", "value": formula}})
    expected.append(float((decimal.Decimal(a) + b) * c / d))
model = {"version": 1, "sheets": [{"name": "Numbers", "cells": cells}]}
source = directory / "model.json"
source.write_text(json.dumps(model), encoding="utf-8")
original = hashlib.sha256(source.read_bytes()).hexdigest()
result = subprocess.run([str(exe), "calculate-workbook", str(source)], check=True, capture_output=True, encoding="utf-8")
report = json.loads(result.stdout)
assert report["complete"] is True
assert len(report["cells"]) == len(expected)
for cell, value in zip(report["cells"], expected):
    assert "failure" not in cell, cell
    assert math.isclose(cell["value"]["value"], value, rel_tol=1e-12, abs_tol=1e-12), cell
assert hashlib.sha256(source.read_bytes()).hexdigest() == original
(directory / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps({"passed": len(expected), "operations": report["operations"], "input_unchanged": True, "report": str(directory / "report.json")}))
