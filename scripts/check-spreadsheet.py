"""Independent openpyxl-generated XLSX -> real Rust CLI sparse inventory."""
from datetime import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from openpyxl import Workbook
from openpyxl.utils.datetime import CALENDAR_MAC_1904, to_excel

root = Path(__file__).resolve().parent.parent
destination = root / "tmp/spreadsheets"
destination.mkdir(parents=True, exist_ok=True)
directory = Path(tempfile.mkdtemp(prefix="openpyxl-", dir=destination))
source = directory / "中文工作簿.xlsx"
workbook = Workbook()
workbook.epoch = CALENDAR_MAC_1904
sheet = workbook.active
sheet.title = "数据"
sheet["A1"] = "中文表格"
sheet["B1"] = 12.5
sheet["C1"] = True
sheet["D1"] = "#DIV/0!"
sheet["E1"] = "=B1*2"
date = datetime(2026, 10, 11, 12, 30)
sheet["F1"] = date
sheet["XFD1048576"] = "远端单元格"
hidden = workbook.create_sheet("隐藏数据")
hidden.sheet_state = "hidden"
hidden["A1"] = "隐藏内容仍须读取"
workbook.save(source)
before = hashlib.sha256(source.read_bytes()).hexdigest()
result = subprocess.run([str(root / "target/debug/rust-wps.exe"), "inspect-xlsx", str(source)], check=True, capture_output=True, timeout=30)
report = json.loads(result.stdout)
assert report["date1904"] is True and report["recalculated"] is False
assert len(report["sheets"]) == 2
cells = {cell["address"]: cell for cell in report["sheets"][0]["cells"]}
assert len(cells) == 7
assert cells["A1"]["value"] == {"type": "text", "value": "中文表格"}
assert cells["B1"]["value"] == {"type": "number", "value": 12.5}
assert cells["C1"]["value"] == {"type": "boolean", "value": True}
assert cells["D1"]["value"] == {"type": "error", "value": "#DIV/0!"}
assert cells["E1"]["formula"] == "B1*2" and cells["E1"]["formula_cache"] == "empty_or_absent"
assert any(warning.startswith("Empty formula caches") for warning in report["warnings"])
assert cells["F1"]["value"]["type"] == "number"
assert abs(cells["F1"]["value"]["value"] - to_excel(date, CALENDAR_MAC_1904)) < 1e-8
assert cells["F1"]["number_format"] is not None
assert cells["XFD1048576"]["row"] == 1048575
assert cells["XFD1048576"]["column"] == 16383
assert report["sheets"][1]["visibility"] == "Hidden"
assert before == hashlib.sha256(source.read_bytes()).hexdigest()
output = directory / "report.json"
output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps({"independentXlsxWriter": "openpyxl", "typedCells": True, "formulaWithoutCache": True, "date1904SerialAndFormat": True, "hiddenSheet": True, "sparseFarCell": True, "sourcePreserved": True, "report": str(output)}, ensure_ascii=True, indent=2))
