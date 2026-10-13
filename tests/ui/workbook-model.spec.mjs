import { test, expect } from '@playwright/test';
import { loadTsModule } from './load-ts.mjs';
import { readFileSync, mkdtempSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
const { WorkbookEditor, parseCellInput } = await loadTsModule('ui/workbook-model.ts');
const initial = () => ({ version: 1, sheets: [{ name: '中文', cells: [{ row: 0, column: 0, content: { type: 'number', value: 12.5 }, number_format: '0.00' }] }] });

test('typed edits stay sparse, preserve formats and undo across saved revisions', () => {
  const source = initial(), editor = new WorkbookEditor(source);
  editor.setCell(0, 0, 0, 'number', '99.5');
  const saved = editor.revision; editor.markSaved(saved); expect(editor.dirty).toBe(false);
  editor.setCell(0, 1048575, 16383, 'text', '001');
  expect(editor.snapshot().sheets[0].cells).toHaveLength(2);
  expect(editor.dirty).toBe(true); editor.undo(); expect(editor.dirty).toBe(false);
  editor.undo(); expect(editor.dirty).toBe(true);
  editor.redo(); expect(editor.dirty).toBe(false);
  editor.redo(); expect(editor.getCell(0, 1048575, 16383).content).toEqual({ type: 'text', value: '001' });
  expect(editor.getCell(0, 0, 0).number_format).toBe('0.00');
  expect(source).toEqual(initial());
  const snapshot = editor.snapshot(); snapshot.sheets[0].cells[0].content.value = 0;
  expect(editor.getCell(0, 0, 0).content.value).toBe(99.5);
  editor.undo(); editor.setCell(0, 0, 1, 'boolean', 'false'); expect(editor.canRedo).toBe(false);
});

test('invalid inputs do not change content or history; empty edits preserve styled cells', () => {
  const editor = new WorkbookEditor(initial());
  for (const [type, text] of [['number', ''], ['number', 'Infinity'], ['number', '0x10'], ['boolean', 'yes'], ['formula', '='], ['formula', '==A1'], ['text', '\0']]) expect(() => editor.setCell(0, 0, 0, type, text)).toThrow();
  expect(editor.dirty).toBe(false); expect(editor.canUndo).toBe(false);
  editor.setCell(0, 0, 0, 'empty', ''); expect(editor.getCell(0, 0, 0).number_format).toBe('0.00');
  editor.setCell(0, 0, 1, 'formula', '=A1*2'); expect(editor.getCell(0, 0, 1).content.value).toBe('A1*2');
  editor.setCell(0, 0, 1, 'empty', ''); expect(editor.getCell(0, 0, 1)).toBeUndefined();
  expect(parseCellInput('text', '001')).toEqual({ type: 'text', value: '001' });
  expect(() => editor.setCell(0, 1048576, 0, 'number', '1')).toThrow();
});

test('editor snapshot exports through real Rust and reopens with the same typed content', () => {
  const editor = new WorkbookEditor(initial());
  editor.setCell(0, 0, 0, 'number', '24.75');
  editor.setCell(0, 0, 1, 'formula', '=A1*2');
  editor.setCell(0, 1048575, 16383, 'text', '最远单元格');
  mkdirSync('tmp/spreadsheets', { recursive: true });
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/editor-'));
  const input = path.join(directory, 'model.json'), output = path.join(directory, 'edited.xlsx');
  writeFileSync(input, JSON.stringify(editor.snapshot()));
  const exe = path.resolve('target/debug/rust-wps.exe');
  const report = JSON.parse(execFileSync(exe, ['create-xlsx', input, output], { encoding: 'utf8', windowsHide: true }));
  expect(report.recalculated).toBe(false);
  expect(report.sheets[0].cells.find(cell => cell.address === 'B1').formula_cache).toBe('absent');
  const restored = JSON.parse(execFileSync(exe, ['read-xlsx-model', output], { encoding: 'utf8', windowsHide: true }));
  expect(restored).toEqual(editor.snapshot());
});
