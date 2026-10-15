// Actual installed full application extension stack, with detached view. This
// records exact snapshots; it is not native browser/clipboard/print evidence.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import {schema, snapshot, fullEditor, cells} from './word-table-row-pagination-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const tables = await loadTsModule('./ui/word-table-widths.ts');
const [inputFile, outputDirectory, tableArg = '0', cellArg = '0', patchArg = '{"cellFillRgb":"FFFFFF"}'] = process.argv.slice(2);
assert(inputFile && outputDirectory, 'Usage: node tests/ui/edit-table-cell-no-fill.mjs INPUT OUTPUT_DIR [TABLE_INDEX] [CELL_INDEX] [PATCH_JSON]');
const tableIndex = Number(tableArg), cellIndex = Number(cellArg), patch = JSON.parse(patchArg);
assert(Number.isInteger(tableIndex) && tableIndex >= 0 && Number.isInteger(cellIndex) && cellIndex >= 0);
const bytes = readFileSync(inputFile), envelope = JSON.parse(bytes), raw = envelope.document ?? envelope;
tables.validateTableWidthsDocument(raw);
const parsed = schema.nodeFromJSON(raw); parsed.check();
const plain = value => Array.isArray(value) ? value.map(plain) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key, child]) => [key, plain(child)])) : value;
// Refuse silent schema attr/mark/text loss before any edits. Schema defaults
// are recorded in schema-json/before, never stripped from comparisons.
function retained(source, restored) {
  assert.equal(restored.type, source.type);
  if ('text' in source) assert.equal(restored.text, source.text);
  for (const [key, value] of Object.entries(source.attrs ?? {})) assert.deepEqual(restored.attrs?.[key], value, `Lost ${source.type}.${key}`);
  for (const mark of source.marks ?? []) {
    const found = restored.marks?.find(item => item.type === mark.type); assert(found, `Lost mark ${mark.type}`);
    for (const [key, value] of Object.entries(mark.attrs ?? {})) assert.deepEqual(found.attrs?.[key], value, `Lost ${mark.type}.${key}`);
  }
  assert.equal(source.content?.length ?? 0, restored.content?.length ?? 0);
  source.content?.forEach((child, index) => retained(child, restored.content[index]));
}
retained(raw, plain(parsed.toJSON()));
const {editor, errors} = fullEditor(parsed);
try {
  const targets = []; editor.state.doc.forEach((node, pos) => {if (node.type.name === 'table') targets.push({node, pos});});
  assert(targets[tableIndex], 'Missing target table'); const target = targets[tableIndex], positions = [];
  target.node.descendants((node, offset) => {if (['tableCell', 'tableHeader'].includes(node.type.name)) positions.push(target.pos + 1 + offset);});
  assert(positions[cellIndex] !== undefined, 'Missing target cell'); editor.commands.setTextSelection(positions[cellIndex] + 2);
  const snap = () => plain(snapshot(editor.state.doc)), before = snap();
  // Independent expected patch calculation, frozen contract v2. It is not
  // imported from the implementation under test.
  function expectedPatch(document, request) {
    const expected = structuredClone(document), cell = expected.content.filter(node => node.type === 'table')[tableIndex].content.flatMap(row => row.content)[cellIndex];
    const attrs = {...cell.attrs};
    if (Object.hasOwn(request, 'cellFillRgb') || Object.hasOwn(request, 'cellFillMode')) {
      delete attrs.cellFillRgb; delete attrs.cellFillMode;
      if (request.cellFillRgb != null) attrs.cellFillRgb = request.cellFillRgb;
      if (request.cellFillMode != null) attrs.cellFillMode = request.cellFillMode;
    }
    if (Object.hasOwn(request, 'cellTextColorMode')) {delete attrs.cellTextColorMode; if (request.cellTextColorMode != null) attrs.cellTextColorMode = request.cellTextColorMode;}
    if (Object.keys(attrs).length) cell.attrs = attrs; else delete cell.attrs;
    return expected;
  }
  assert(editor.commands.setTableCellColors(patch), 'Patch rejected or no-op'); const edited = snap(); assert.deepEqual(edited, expectedPatch(before, patch));
  assert(editor.commands.undo()); const undone = snap(); assert.deepEqual(undone, before);
  assert(editor.commands.redo()); const redone = snap(); assert.deepEqual(redone, edited);
  const outcomes = {'source': raw, 'schema-json': plain(parsed.toJSON()), before, edited, undo: undone, redo: redone};
  for (const [name, request] of [['reset-fill', {cellFillRgb: null}], ['reset-mode', {cellTextColorMode: null}], ['reset-all', {cellFillRgb: null, cellFillMode: null, cellTextColorMode: null}]]) {
    const expected = expectedPatch(edited, request), changed = editor.commands.setTableCellColors(request);
    outcomes[name] = snap(); assert.deepEqual(outcomes[name], expected);
    if (changed) {assert(editor.commands.undo()); assert.deepEqual(snap(), edited);}
  }
  // Reopen exact draft JSON through the complete stack, without comparator
  // field filtering or candidate-derived expected attribute changes.
  const reopened = fullEditor(schema.nodeFromJSON(edited));
  try {outcomes.reopened = plain(snapshot(reopened.editor.state.doc)); assert.deepEqual(outcomes.reopened, edited); assert.deepEqual(reopened.errors, []);} finally {reopened.editor.destroy();}
  assert.deepEqual(errors, []);
  const out = resolve(outputDirectory); mkdirSync(out, {recursive: false});
  for (const [name, value] of Object.entries(outcomes)) writeFileSync(join(out, `${name}.json`), JSON.stringify(value, null, 2) + '\n', {flag: 'wx'});
  const report = {input: resolve(inputFile), inputSha256: createHash('sha256').update(bytes).digest('hex'), tableIndex, cellIndex, patch,
    files: Object.keys(outcomes), scope: 'Full installed application extension stack, detached view; exact editor/undo/redo/reset/draft reopen snapshots, no native browser or print claim.'};
  writeFileSync(join(out, 'report.json'), JSON.stringify(report, null, 2) + '\n', {flag: 'wx'}); process.stdout.write(JSON.stringify(report) + '\n');
} finally {editor.destroy();}
