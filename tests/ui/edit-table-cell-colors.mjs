// Browser-free evidence helper using the product's actual table candidates and
// history. Does not claim browser, native picker, clipboard or rendering QA.
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {Extension, Mark, Node, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {closeHistory, history, undo, redo} from '@tiptap/pm/history';
import {tableEditing} from '@tiptap/pm/tables';
import {loadTsModule} from './load-ts.mjs';

const [inputFile, outputDirectory, tableIndexArg = '0', cellIndexArg = '0', patchArg = '{"cellFillRgb":"000000","cellTextColorMode":"auto"}'] = process.argv.slice(2);
if (!inputFile || !outputDirectory) throw new Error('Usage: node tests/ui/edit-table-cell-colors.mjs INPUT_JSON OUTPUT_DIR [TABLE_INDEX=0] [CELL_INDEX=0] [PATCH_JSON]');
const tableIndex = Number(tableIndexArg), cellIndex = Number(cellIndexArg), patch = JSON.parse(patchArg);
assert(Number.isInteger(tableIndex) && tableIndex >= 0 && Number.isInteger(cellIndex) && cellIndex >= 0);
const bytes = readFileSync(inputFile), input = JSON.parse(bytes), raw = input.document ?? input;
const tables = await loadTsModule('./ui/word-table-widths.ts');
const spacing = await loadTsModule('./ui/word-paragraph-spacing.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const image = await loadTsModule('./ui/word-image-dimensions.ts');
const profile = await loadTsModule('./ui/word-style-profile.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const {FontSize} = await loadTsModule('./ui/word-font-size.ts');
const {TextColor} = await loadTsModule('./ui/word-text-color.ts');
const {FontFamily} = await loadTsModule('./ui/word-font-family.ts');
const metadata = Extension.create({name: 'evidenceMetadata', addGlobalAttributes: () => [
  {types: ['doc'], attributes: {styleProfile: {default: null, rendered: false}}},
  {types: ['paragraph', 'heading'], attributes: {textAlign: {default: null, rendered: false}}},
]});
const Superscript = Mark.create({name: 'superscript', excludes: 'subscript', renderHTML: () => ['sup', 0]});
const Subscript = Mark.create({name: 'subscript', excludes: 'superscript', renderHTML: () => ['sub', 0]});
const PageBreak = Node.create({name: 'pageBreak', group: 'block', atom: true, renderHTML: () => ['div', {'data-page-break': ''}]});
const schema = getSchema([StarterKit.configure({heading: {levels: [1, 2, 3]}, blockquote: false, horizontalRule: false, codeBlock: false}),
  metadata, FontFamily, FontSize, TextColor, Superscript, Subscript, empty.EmptyParagraphFont, page.WordPageGeometry,
  spacing.ParagraphSpacing, indent.ParagraphIndent, PageBreak, TableKit.configure({table: false}), tables.WordTable, image.WordImage]);
profile.documentStyleProfile(raw); tables.validateTableWidthsDocument(raw); empty.validateEmptyParagraphFontDocument(raw);
spacing.validateParagraphSpacingDocument(raw); indent.validateParagraphIndentDocument(raw); page.validatePageGeometryDocument(raw); image.validateImageDimensionsDocument(raw);
const document = schema.nodeFromJSON(raw); document.check();
// Refuse input with information the evidence schema would silently discard.
function retained(source, restored) {
  assert.equal(restored.type, source.type); if (source.text !== undefined) assert.equal(restored.text, source.text);
  for (const [key, value] of Object.entries(source.attrs ?? {})) assert.deepEqual(restored.attrs?.[key], value, `Lost ${source.type}.${key}`);
  for (const mark of source.marks ?? []) {
    const found = restored.marks?.find(value => value.type === mark.type); assert(found, `Lost ${mark.type}`);
    for (const [key, value] of Object.entries(mark.attrs ?? {})) assert.deepEqual(found.attrs?.[key], value, `Lost mark ${mark.type}.${key}`);
  }
  assert.equal(restored.content?.length ?? 0, source.content?.length ?? 0);
  source.content?.forEach((child, index) => retained(child, restored.content[index]));
}
retained(raw, document.toJSON());
const targets = []; document.forEach((node, pos) => {if (node.type.name === 'table') targets.push({node, pos});});
assert(targets[tableIndex], 'Missing top-level table');
const target = targets[tableIndex], cells = []; target.node.descendants((node, offset) => {if (['tableCell', 'tableHeader'].includes(node.type.name)) cells.push(target.pos + 1 + offset);});
assert(cells[cellIndex] !== undefined, 'Missing cell');
let state = EditorState.create({doc: document, selection: TextSelection.near(document.resolve(cells[cellIndex] + 2)),
  plugins: [history(), tableEditing(), tables.tableWidthGuard(), page.pageGeometryGuard()]});
// ProseMirror mark attrs can have a null prototype. Normalize prototypes only,
// recursively retaining every own enumerable JSON field and its value. Do not
// stringify for comparison: that could silently discard undefined evidence.
const plainData = value => Array.isArray(value) ? value.map(plainData)
  : value !== null && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key, child]) => [key, plainData(child)])) : value;
const snapshot = state => plainData(indent.omitDefaultParagraphIndent(image.omitDefaultImageDimensions(tables.omitDefaultTableWidths(spacing.omitDefaultParagraphSpacing(profile.snapshotDocument({getJSON: () => state.doc.toJSON()}))))));
const before = snapshot(state), transaction = tables.createTableCellColorsTransaction(state, patch);
assert(transaction, 'Requested patch is a no-op; choose another patch or target');
state = state.apply(closeHistory(transaction)); state = state.apply(closeHistory(state.tr));
const editedState = state, edited = snapshot(state);
assert.equal(state.doc.textContent, document.textContent); assert(transaction.selection.eq(state.selection));
assert(undo(state, tr => {state = state.apply(tr);})); const undone = snapshot(state); assert.deepEqual(undone, before);
assert(redo(state, tr => {state = state.apply(tr);})); const redone = snapshot(state); assert.deepEqual(redone, edited);
const reset = patch => {const tr = tables.createTableCellColorsTransaction(editedState, patch); return snapshot(tr ? editedState.apply(tr) : editedState);};
const outcomes = {before, edited, undo: undone, redo: redone, 'reset-fill': reset({cellFillRgb: null}), 'reset-mode': reset({cellTextColorMode: null}), 'reset-all': reset({cellFillRgb: null, cellTextColorMode: null})};
// Compare each non-target node and every text mark against the normalized
// before document; the actual product operation may only change selected attrs.
const expected = structuredClone(before);
const expectedTable = expected.content.filter(node => node.type === 'table')[tableIndex];
const expectedCell = expectedTable.content.flatMap(row => row.content)[cellIndex];
expectedCell.attrs = {...expectedCell.attrs, ...patch};
for (const field of ['cellFillRgb', 'cellTextColorMode']) if (expectedCell.attrs[field] === null) delete expectedCell.attrs[field];
if (!Object.keys(expectedCell.attrs).length) delete expectedCell.attrs;
assert.deepEqual(edited, expected);
const dir = resolve(outputDirectory); mkdirSync(dir, {recursive: true});
for (const [name, model] of Object.entries(outcomes)) {
  tables.validateTableWidthsDocument(model);
  writeFileSync(join(dir, `${name}.json`), `${JSON.stringify(input.document ? {...input, document: model} : model, null, 2)}\n`, {flag: 'wx'});
}
const report = {input: resolve(inputFile), inputSha256: createHash('sha256').update(bytes).digest('hex'), tableIndex, cellIndex,
  cellPosition: cells[cellIndex], patch, transactionSteps: transaction.steps.length, selection: transaction.selection.toJSON(),
  notes: 'Actual browser-free table candidate/history; snapshots include normal editor defaults. No visual or native verification.', files: Object.keys(outcomes)};
writeFileSync(join(dir, 'report.json'), `${JSON.stringify(report, null, 2)}\n`, {flag: 'wx'});
process.stdout.write(`${JSON.stringify(report)}\n`);
