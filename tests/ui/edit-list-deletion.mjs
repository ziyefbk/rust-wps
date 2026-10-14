// Actual source-model ProseMirror deletion/history/draft evidence. These are
// registered-app candidate functions, not browser/native keyboard acceptance.
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {getSchema} from '@tiptap/core';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {history, undo, redo, undoDepth} from '@tiptap/pm/history';
import {extensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const [inputFile, outputDirectory] = process.argv.slice(2);
if (!inputFile || !outputDirectory) throw new Error('Usage: node tests/ui/edit-list-deletion.mjs IMPORT_JSON OUTPUT_DIR');
const bytes = readFileSync(inputFile), raw = JSON.parse(bytes).document ?? JSON.parse(bytes);
const list = await loadTsModule('./ui/word-list-geometry.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const schema = getSchema([...extensions, list.PreservedListGeometry, empty.EmptyParagraphFont]);
list.validateListGeometryDocument(raw); page.validatePageGeometryDocument(raw); indent.validateParagraphIndentDocument(raw);
const document = schema.nodeFromJSON(raw); document.check();
assert.equal(document.childCount, 1); assert.equal(document.firstChild.type.name, 'orderedList'); assert.equal(document.firstChild.childCount, 2);
const errors = [];
const stateFor = (doc, selection = TextSelection.atStart(doc)) => EditorState.create({doc, selection,
  plugins: [history(), list.listGeometryGuard(message => errors.push(message)), page.pageGeometryGuard(), indent.paragraphIndentGuard()]});
const snapshot = state => list.omitDefaultListGeometry(empty.omitDefaultEmptyParagraphFont(baseSnapshot(state.doc)));
function retained(source, restored) {
  assert.equal(source.type, restored.type); if (source.text !== undefined) assert.equal(source.text, restored.text);
  for (const [key, value] of Object.entries(source.attrs ?? {})) assert.deepEqual(restored.attrs?.[key], value);
  for (const mark of source.marks ?? []) {const match = restored.marks?.find(candidate => candidate.type === mark.type); assert(match); for (const [key, value] of Object.entries(mark.attrs ?? {})) assert.deepEqual(match.attrs?.[key], value);}
  assert.equal(source.content?.length ?? 0, restored.content?.length ?? 0); source.content?.forEach((child, index) => retained(child, restored.content[index]));
}
retained(raw, document.toJSON());
const dir = resolve(outputDirectory); mkdirSync(dir, {recursive: true});
const snapshots = new Map(), operations = [];
function save(name, state) {
  const value = snapshot(state); list.validateListGeometryDocument(value); page.validatePageGeometryDocument(value); indent.validateParagraphIndentDocument(value);
  const filename = `${name}.json`; const wrapped = {version: 2, document: value}; snapshots.set(filename, wrapped);
  writeFileSync(join(dir, filename), JSON.stringify(wrapped, null, 2) + '\n', {flag: 'wx'}); return filename;
}
const initial = stateFor(document); save('initial', initial);
const itemPosition = (doc, index) => 1 + Array.from({length: index}, (_, i) => doc.firstChild.child(i).nodeSize).reduce((sum, size) => sum + size, 0);
function exercise(name, beforeName, document, selection, kind, indices, description) {
  let state = stateFor(document, selection); const before = state.doc;
  const tr = list.createPreservedListDeleteTransaction(state); assert(tr);
  const applied = state.applyTransaction(tr); assert.equal(applied.transactions.length, 1); state = applied.state; assert.equal(undoDepth(state), 1);
  const edited = state.doc, after = save(name, state);
  const draft = JSON.parse(JSON.stringify({version: 2, document: snapshot(state)})); const restored = stateFor(schema.nodeFromJSON(draft.document)); assert(restored.doc.eq(edited));
  const draftPath = save(`${name}-draft`, restored);
  assert(undo(state, transaction => {state = state.apply(transaction);})); assert(state.doc.eq(before)); assert(state.selection.eq(selection)); const undoPath = save(`${name}-undo`, state);
  assert(redo(state, transaction => {state = state.apply(transaction);})); assert(state.doc.eq(edited)); const redoPath = save(`${name}-redo`, state);
  operations.push({name, kind, before: `${beforeName}.json`, after, undo: undoPath, redo: redoPath, draft: draftPath,
    ...(kind === 'items' ? {blockIndex: 0, itemIndices: indices} : kind === 'blocks' ? {blockIndices: indices} : {}), selection: selection.toJSON(), description});
  return edited;
}
const firstDeleted = exercise('first-node', 'initial', document, NodeSelection.create(document, 1), 'items', [0], 'Node-selected first source item deleted; source start unchanged and remaining original second item becomes first');
exercise('last-node', 'initial', document, NodeSelection.create(document, itemPosition(document, 1)), 'items', [1], 'Node-selected last source item deleted; first item unchanged');
exercise('first-text', 'initial', document, TextSelection.create(document, 3, itemPosition(document, 1) + 2), 'items', [0], 'Text selection from first source item start to second source item start removes the first shell');
exercise('whole-list-node', 'initial', document, NodeSelection.create(document, 0), 'blocks', [0], 'Whole preserved list removed, keeping page attrs and one neutral paragraph');
exercise('whole-document', 'initial', document, new AllSelection(document), 'document', [], 'AllSelection clears document content but retains exact root/page attrs');
exercise('sole-item-node', 'first-node', firstDeleted, NodeSelection.create(firstDeleted, 1), 'items', [0], 'Delete sole remaining source item and its list shell, leaving a neutral paragraph');
const splitState = stateFor(document, TextSelection.create(document, 183));
const split = splitState.applyTransaction(list.createPreservedListSplitTransaction(splitState)); assert.equal(split.transactions.length, 1); assert.equal(split.state.doc.firstChild.childCount, 3);
save('split-before', split.state);
exercise('middle-node', 'split-before', split.state.doc, NodeSelection.create(split.state.doc, itemPosition(split.state.doc, 1)), 'items', [1], 'After actual app Enter at first source item character 180, delete the new middle item and preserve both exact survivors');
const withOutside = initial.applyTransaction(initial.tr.insert(document.content.size, schema.nodes.paragraph.create(null, schema.text('After the numbered list.'))));
assert.equal(withOutside.transactions.length, 1); save('outside-before', withOutside.state);
const outsideDoc = withOutside.state.doc, outsideStart = outsideDoc.firstChild.nodeSize + 1;
exercise('last-text', 'outside-before', outsideDoc, TextSelection.create(outsideDoc, itemPosition(outsideDoc, 1) + 2, outsideStart), 'items', [1], 'Text selection crosses last item boundary to immediate outside paragraph start; outside paragraph exact');
exercise('whole-list-text', 'outside-before', outsideDoc, TextSelection.create(outsideDoc, 3, outsideStart), 'blocks', [0], 'First item text start to immediate outside paragraph text start deletes whole list and preserves outside paragraph');
// Separate authored control, not another genuine source: duplicate equal items
// share the same immutable node, non-one start is explicit, and a survivor gets
// actual inline transactions before deletion to expose formatting loss.
const secondText = itemPosition(document, 1) + 2;
let styled = stateFor(document, TextSelection.create(document, secondText, secondText + document.firstChild.child(1).firstChild.content.size));
styled = styled.apply(styled.tr.insertText('Styled survivor'));
let styling = styled.tr;
for (const [name, attrs] of [['fontFamily', {name: 'Times New Roman'}], ['fontSize', {halfPoints: 26}], ['bold', null], ['italic', null], ['underline', null], ['strike', null], ['textColor', {rgb: '205090'}]]) {
  styling.addMark(secondText, secondText + 'Styled survivor'.length, schema.mark(name, attrs));
}
styled = styled.apply(styling);
const sourceFirst = document.firstChild.firstChild;
const synthetic = schema.nodes.doc.create(document.attrs, schema.nodes.orderedList.create({...document.firstChild.attrs, start: 7}, [sourceFirst, sourceFirst, styled.doc.firstChild.child(1)]));
save('authored-before', stateFor(synthetic));
exercise('authored-first', 'authored-before', synthetic, NodeSelection.create(synthetic, 1), 'items', [0], 'Explicit authored start 7 and duplicate equal items; first deletion keeps start 7 and styled survivor exact');
exercise('authored-middle', 'authored-before', synthetic, TextSelection.create(synthetic, itemPosition(synthetic, 1) + 2, itemPosition(synthetic, 2) + 2), 'items', [1], 'Explicit authored duplicate-equal middle boundary deletion retains all styles and unchanged start 7');
exercise('authored-last', 'authored-before', synthetic, NodeSelection.create(synthetic, itemPosition(synthetic, 2)), 'items', [2], 'Explicit authored start 7 last deletion keeps both equal source-derived items distinct');
assert.deepEqual(errors, []); assert(readFileSync(inputFile).equals(bytes));
const manifest = {input: resolve(inputFile), inputSha256: createHash('sha256').update(bytes).digest('hex'), evidence: 'Actual app-owned deletion/Enter candidates, ProseMirror guards and history, JSON draft restore; no browser/native keyboard or rendering claim',
  operations, files: [...snapshots.keys()], sourceFieldsRetained: true, rejected: 0,
  productFiles: Object.fromEntries(['ui/word-list-geometry.ts', 'ui/word-list-geometry-model.ts', 'tests/ui/edit-list-deletion.mjs'].map(path => [path, createHash('sha256').update(readFileSync(path)).digest('hex')]))};
writeFileSync(join(dir, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', {flag: 'wx'}); console.log(JSON.stringify(manifest, null, 2));
