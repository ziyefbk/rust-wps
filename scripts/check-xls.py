"""Self-generated BIFF8 corpus; independent xlrd comparison, never a calculator."""
from datetime import datetime
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import olefile
import xlrd
import xlwt

root = Path(__file__).resolve().parents[1]
def make(path, date1904=False):
    book = xlwt.Workbook()
    book.dates_1904 = date1904
    sheet = book.add_sheet('Data')
    sheet.write(0, 0, 12.5, xlwt.easyxf(num_format_str='0.00'))
    sheet.write(0, 1, xlwt.Formula('A1*2'))
    sheet.write(0, 2, True)
    sheet.write(0, 3, '001')
    sheet.row(0).set_cell_error(4, '#DIV/0!')
    sheet.write(1, 0, datetime(2024, 1, 1), xlwt.easyxf(num_format_str='yyyy-mm-dd'))
    sheet.write(1, 2, xlwt.Formula('1+1'))
    sheet.write(1, 3, '中文')
    sheet.write_merge(2, 2, 0, 1, 'Merged')
    sheet.write(2, 3, 9.25, xlwt.easyxf(num_format_str='0.00"元"'))
    sheet.write(65535, 255, 'Far')
    book.add_sheet('Hidden').visibility = 2
    book.save(str(path))
    # A controlled cached result proves the reader flags it as a formula cache.
    with olefile.OleFileIO(path, write_mode=True) as container:
        data = bytearray(container.openstream('Workbook').read())
        position = 0
        while position + 4 <= len(data):
            kind, length = struct.unpack_from('<HH', data, position)
            if kind == 6 and struct.unpack_from('<HH', data, position + 4) == (0, 1):
                data[position + 10:position + 18] = struct.pack('<d', 25)
                break
            position += 4 + length
        else:
            raise AssertionError('formula record missing')
        container.write_stream('Workbook', bytes(data))

if '--write-fixture' in sys.argv:
    make(root / 'tests/fixtures/basic-biff8.xls')
    raise SystemExit()
directory = Path(tempfile.mkdtemp(prefix='xls-', dir=root / 'tmp/spreadsheets'))
results = []
for date1904 in [False, True]:
    source = directory / f'epoch-{1904 if date1904 else 1900}.xls'
    make(source, date1904)
    before = hashlib.sha256(source.read_bytes()).hexdigest()
    report = json.loads(subprocess.check_output([str(root/'target/debug/rust-wps.exe'), 'inspect-xls', str(source)], encoding='utf-8'))
    independent = xlrd.open_workbook(source, formatting_info=True)
    assert report['date1904'] == bool(independent.datemode) == date1904
    cells = {cell['address']: cell for cell in report['sheets'][0]['cells']}
    sheet = independent.sheet_by_index(0)
    for address, row, column in [('A1',0,0),('B1',0,1),('D1',0,3),('A2',1,0),('D2',1,3),('D3',2,3),('IV65536',65535,255)]:
        assert cells[address]['value']['value'] == sheet.cell_value(row, column), address
        xf = independent.xf_list[sheet.cell_xf_index(row, column)]
        expected_format = independent.format_map[xf.format_key].format_str
        assert cells[address]['number_format'] == expected_format, (address, expected_format)
    assert cells['C1']['value'] == {'type':'boolean','value':True}
    assert cells['E1']['value'] == {'type':'error','value':'#DIV/0!'}
    assert cells['B1']['formula_unavailable'] and cells['B1']['formula'] is None and cells['B1']['formula_cache'] == 'present'
    assert cells['C2']['formula_unavailable'] and cells['C2']['formula_cache'] == 'absent'
    assert report['sheets'][0]['merged_ranges'] == ['A3:B3']
    assert report['sheets'][1]['visibility'] == 'VeryHidden'
    assert independent.sheet_by_index(1).visibility == 2
    assert xlrd.xldate_as_datetime(cells['A2']['value']['value'], independent.datemode) == datetime(2024,1,1)
    assert hashlib.sha256(source.read_bytes()).hexdigest() == before
    result = {'source':str(source), 'sha256':before, 'date1904':date1904, 'independentXlrd':True, 'sourceUnchanged':True, 'formulaCacheOnly':True}
    results.append(result)
(directory/'report.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
print(json.dumps({'systems':results}))
