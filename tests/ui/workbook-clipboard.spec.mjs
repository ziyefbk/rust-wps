import { test, expect } from '@playwright/test';
import { loadTsModule as module } from './load-ts.mjs';
const { decodeTsv, encodeTsv, decodeTypedCells } = await module('ui/workbook-clipboard.ts');
const { WorkbookEditor } = await module('ui/workbook-model.ts');

test('TSV preserves quoted tabs, multiline text, quotes, empty cells and leading zeroes', () => {
  const matrix = [['001', '中文\t两列', '两行\r\n正文', '"引号"'], ['', '=SUM(A1:A2)', '', '尾部']];
  expect(decodeTsv(encodeTsv(matrix))).toEqual(matrix);
  expect(decodeTsv('a\tb\r\nc\td\r\n')).toEqual([['a', 'b'], ['c', 'd']]);
  expect(decodeTsv('a\tb\nc')).toEqual([['a', 'b'], ['c', '']]);
  expect(decodeTsv('""')).toEqual([['']]);
  expect(() => decodeTsv('"unterminated')).toThrow();
  expect(() => decodeTsv('"closed"bad')).toThrow();
  expect(() => decodeTsv('a\t'.repeat(10001))).toThrow();
});

test('batch edits are atomic and undo as one transaction while preserving formats', () => {
  const editor = new WorkbookEditor({ version: 1, sheets: [{ name: '表', cells: [{ row: 0, column: 0, content: { type: 'number', value: 10 }, number_format: '0.00' }] }] });
  const initial = editor.snapshot();
  expect(() => editor.setCells(0, [{ row: 0, column: 0, type: 'number', input: '20' }, { row: 1, column: 0, type: 'number', input: 'bad' }])).toThrow();
  expect(editor.snapshot()).toEqual(initial); expect(editor.canUndo).toBe(false);
  expect(() => editor.setCells(0, [{ row: 0, column: 0, type: 'text', input: 'changed' }, { row: 1048576, column: 0, type: 'text', input: 'outside' }])).toThrow();
  expect(editor.snapshot()).toEqual(initial);
  editor.setCells(0, [{ row: 0, column: 0, type: 'text', input: '001' }, { row: 1, column: 0, type: 'formula', input: '=A1*2' }]);
  expect(editor.getCell(0, 0, 0).number_format).toBe('0.00');
  const edited = editor.snapshot(); editor.undo(); expect(editor.snapshot()).toEqual(initial);
  expect(editor.canUndo).toBe(false); editor.redo(); expect(editor.snapshot()).toEqual(edited);
  const revision = editor.revision;
  expect(() => editor.setCells(0, [{ row: 2, column: 0, type: 'text', input: 'x' }, { row: 2, column: 0, type: 'text', input: 'y' }])).toThrow();
  expect(editor.revision).toBe(revision); expect(editor.snapshot()).toEqual(edited);
});

test('typed clipboard validates shapes, types and bounds before creating inputs', () => {
  const rows = [[{ type: 'number', value: 1 }, { type: 'formula', value: 'A1*2' }], [{ type: 'text', value: '001' }, { type: 'boolean', value: false }]];
  expect(decodeTypedCells(JSON.stringify({ version: 1, rows }))).toEqual(rows);
  for (const payload of [{ version: 3, rows }, { version: 2, origin: { row: -1, column: 0 }, rows }, { version: 1, rows: [rows[0], []] }, { version: 1, rows: [[{ type: 'number', value: '1' }]] }, { version: 1, rows: [[{ type: 'unknown' }]] }]) expect(() => decodeTypedCells(JSON.stringify(payload))).toThrow();
});

test('text budget remains consistent across batch clear, undo and rejected growth', () => {
  const cells = Array.from({ length: 128 }, (_, row) => ({ row, column: 0, content: { type: 'text', value: 'x'.repeat(32767) } }));
  const editor = new WorkbookEditor({ version: 1, sheets: [{ name: '表', cells }] });
  const before = editor.snapshot();
  expect(() => editor.setCell(0, 200, 0, 'text', 'extra'.repeat(100))).toThrow();
  expect(editor.dirty).toBe(false);
  editor.setCells(0, [{ row: 0, column: 0, type: 'empty', input: '' }, { row: 200, column: 0, type: 'text', input: 'x'.repeat(32767) }]);
  editor.undo(); expect(editor.snapshot()).toEqual(before);
  expect(() => editor.setCell(0, 200, 0, 'text', 'extra'.repeat(100))).toThrow();
  expect(editor.snapshot()).toEqual(before);
});
