// Actual application controls, commands and registered key handlers with independently
// frozen full-model oracles. DOM doubles do not establish browser/layout acceptance.
// Usage: node tests/ui/edit-minimum-line-height.mjs IMPORT_JSON BASE_ORACLE EXPECTED_STATES NEW_OUTPUT_DIR
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {TextSelection} from '@tiptap/pm/state';
import {undo, redo, undoDepth} from '@tiptap/pm/history';
import {schema, snapshot, fullEditor, key, controls, spacing, block} from './word-minimum-line-height-harness.mjs';
import {Element} from './word-minimum-line-height-dom.mjs';
const [inputFile, baseFile, expectedFile, outputDirectory] = process.argv.slice(2);
assert(inputFile && baseFile && expectedFile && outputDirectory, 'Four arguments required');
const read = file => JSON.parse(readFileSync(file, 'utf8'));
const imported = read(inputFile), base = read(baseFile), expected = read(expectedFile);
assert.deepEqual(imported, base, 'Full genuine import differs from independent oracle');
spacing.validateParagraphSpacingDocument(imported.document);
const source = schema.nodeFromJSON(imported.document); source.check();
const names = ['retained', 'minimum360', 'minimum360-undo', 'minimum360-redo', 'multiple360', 'default-line', 'gap-before100', 'heading2', 'split-first8', 'join-first-two', 'list-refusal-unchanged', 'partial-delete-refusal-unchanged'];
assert.deepEqual(Object.keys(expected).sort(), [...names].sort());
const outcomes = {}, receipts = [];
function record(name, editor) {
  const model = JSON.parse(JSON.stringify(snapshot(editor.state.doc)));
  assert.deepEqual(model, expected[name], `${name}: independent full model`);
  spacing.validateParagraphSpacingDocument(model);
  const draft = JSON.parse(JSON.stringify({version: 2, document: model}));
  const restored = editor.schema.nodeFromJSON(draft.document); restored.check();
  assert.deepEqual(JSON.parse(JSON.stringify(snapshot(restored))), expected[name], `${name}: draft restore`);
  assert(restored.eq(editor.state.doc), `${name}: draft retains full editor document`);
  outcomes[name] = {model, draft};
}
function scenario(run) {
  const {editor, errors} = fullEditor(source);
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 1)));
  try {run(editor, errors);} finally {editor.destroy();}
}
function withControls(editor, run) {
  const oldDocument = globalThis.document, oldElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id);};
  const owner = {get state() {return editor.state;}, isEditable: true,
    view: {composing: false, focus() {}, dispatch: tr => editor.view.dispatch(tr)}};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  try {
    const control = new controls.WordParagraphSpacingControls({editor: () => owner, locked: () => false, notice: message => notices.push(message), syncSelection() {}});
    control.update();
    run({type(id, value) {element(id).emit('focus'); element(id).value = value; element(id).emit('input');},
      mode(value) {element('edit-line-mode').emit('focus'); element('edit-line-mode').value = value; element('edit-line-mode').emit('change');},
      apply() {element('edit-paragraph-spacing-apply').emit('click');}, reset() {element('edit-line-spacing-reset').emit('click');}});
    assert.deepEqual(notices, [], 'Valid control action emitted error');
  } finally {globalThis.document = oldDocument; globalThis.HTMLElement = oldElement;}
}
scenario((editor, errors) => {
  record('retained', editor);
  withControls(editor, control => {control.type('edit-minimum-line-height', '18'); control.apply();});
  record('minimum360', editor); assert.equal(undoDepth(editor.state), 1);
  assert(undo(editor.state, tr => editor.view.dispatch(tr))); record('minimum360-undo', editor);
  assert(redo(editor.state, tr => editor.view.dispatch(tr))); record('minimum360-redo', editor);
  assert.deepEqual(errors, []);
});
for (const [name, action] of [
  ['multiple360', control => {control.mode('multiple'); control.type('edit-line-spacing', '1.5'); control.apply();}],
  ['default-line', control => control.reset()],
  ['gap-before100', control => {control.type('edit-space-before', '5'); control.apply();}],
]) scenario((editor, errors) => {withControls(editor, action); record(name, editor); assert.deepEqual(errors, []);});
scenario((editor, errors) => {
  const tr = block.createWordBlockTypeTransaction(editor.state, 2); assert(tr); editor.view.dispatch(tr);
  record('heading2', editor); assert.deepEqual(errors, []);
});
scenario((editor, errors) => {
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 9)));
  assert(key(editor, 'Enter')); record('split-first8', editor); assert.deepEqual(errors, []);
});
scenario((editor, errors) => {
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, editor.state.doc.firstChild.nodeSize + 1)));
  assert(key(editor, 'Backspace')); record('join-first-two', editor); assert.deepEqual(errors, []);
});
scenario((editor, errors) => {
  const before = editor.state.doc, depth = undoDepth(editor.state);
  editor.commands.toggleBulletList(); assert(editor.state.doc.eq(before)); assert.equal(undoDepth(editor.state), depth);
  assert(errors.length > 0); record('list-refusal-unchanged', editor);
  receipts.push({operation: 'list conversion refused', errors: [...errors]});
});
scenario((editor, errors) => {
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 2, editor.state.doc.firstChild.nodeSize + 2)));
  const before = editor.state.doc, depth = undoDepth(editor.state);
  editor.view.dispatch(editor.state.tr.deleteSelection()); assert(editor.state.doc.eq(before)); assert.equal(undoDepth(editor.state), depth);
  assert(errors.length > 0); record('partial-delete-refusal-unchanged', editor);
  receipts.push({operation: 'partial cross-paragraph deletion refused', errors: [...errors]});
});
assert.deepEqual(Object.keys(outcomes).sort(), [...names].sort());
mkdirSync(outputDirectory); // Never overwrite a prior evidence directory.
for (const [name, {model, draft}] of Object.entries(outcomes)) {
  writeFileSync(join(outputDirectory, `${name}.json`), JSON.stringify(model, null, 2) + '\n', {flag: 'wx'});
  writeFileSync(join(outputDirectory, `${name}.draft.json`), JSON.stringify(draft, null, 2) + '\n', {flag: 'wx'});
}
const sha = file => createHash('sha256').update(readFileSync(file)).digest('hex');
writeFileSync(join(outputDirectory, 'report.json'), JSON.stringify({status: 'PASS', states: names, inputSha256: sha(inputFile), baseOracleSha256: sha(baseFile), stateOracleSha256: sha(expectedFile), receipts}, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({status: 'PASS', states: names.length, draftRoundtrips: names.length}));
