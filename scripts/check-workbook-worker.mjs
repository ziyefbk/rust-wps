import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const python = 'C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
const fixture = JSON.parse(execFileSync(python, ['scripts/check-spreadsheet.py'], { encoding: 'utf8', windowsHide: true }));
const source = path.join(path.dirname(fixture.report), '中文工作簿.xlsx');
const before = readFileSync(source);
const exe = path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
const response = JSON.parse(execFileSync(exe, ['--document-worker'], {
  input: JSON.stringify({ operation: 'open', path: source }), encoding: 'utf8',
  timeout: 35000, maxBuffer: 32 * 1024 * 1024, windowsHide: true,
}));
assert.equal(response.result, 'opened');
assert.equal(response.document.inspection.format, 'xlsx');
assert.deepEqual(response.document.workbook, JSON.parse(readFileSync(fixture.report, 'utf8')));
assert.deepEqual(readFileSync(source), before);
const exported = JSON.parse(execFileSync(python, ['scripts/check-workbook-export.py'], { encoding: 'utf8', windowsHide: true }));
const open = file => JSON.parse(execFileSync(exe, ['--document-worker'], {
  input: JSON.stringify({ operation: 'open', path: file }), encoding: 'utf8',
  timeout: 35000, maxBuffer: 32 * 1024 * 1024, windowsHide: true,
}));
const authored = open(exported.output);
assert.equal(authored.result, 'opened');
const expectedModel = JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'), ['read-xlsx-model', exported.output], { encoding: 'utf8', windowsHide: true }));
assert.deepEqual(authored.document.workbook_model, expectedModel);
for (const name of ['stale_model', 'stale_sheet']) {
  const file = exported.variants[name], bytes = readFileSync(file), result = open(file);
  assert.equal(result.result, 'opened');
  assert.equal(result.document.workbook_model, undefined);
  assert.ok(result.document.inspection.warnings.some(warning => warning.startsWith('Editable workbook model unavailable:')));
  assert.deepEqual(readFileSync(file), bytes);
}
const external = open(exported.variants.external);
assert.equal(external.result, 'opened');
assert.equal(external.document.workbook_model, undefined);
assert.ok(!external.document.inspection.warnings.some(warning => warning.startsWith('Editable workbook model unavailable:')));
console.log(JSON.stringify({ realWorkerXlsx: true, matchesIndependentFixture: true, sourcePreserved: true, authoredModelRestored: true, staleModelFallsBackToReadOnly: true, externalWorkbookReadOnly: true }));
