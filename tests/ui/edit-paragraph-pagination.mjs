// Actual frontend candidate/plugin/history snapshots. This is not browser,
// native keyboard, clipboard, or print pagination acceptance.
// Usage: node tests/ui/edit-paragraph-pagination.mjs IMPORT_JSON NEW_OUTPUT_DIR
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import {TextSelection} from '@tiptap/pm/state';
import {Fragment, Slice} from '@tiptap/pm/model';
import {closeHistory, undo, redo} from '@tiptap/pm/history';
import {joinBackward} from '@tiptap/pm/commands';
import {createWordBlockTypeTransaction} from './word-block-type-harness.mjs';
import {pagination, list, empty, page, indent, schema, snapshot, stateFor, fullEditor, key} from './word-paragraph-pagination-harness.mjs';
const [inputFile, outputDirectory] = process.argv.slice(2);
assert(inputFile && outputDirectory, 'Usage: node tests/ui/edit-paragraph-pagination.mjs IMPORT_JSON NEW_OUTPUT_DIR');
const bytes = readFileSync(inputFile), parsed = JSON.parse(bytes), raw = parsed.document ?? parsed;
for (const validate of [pagination.validateParagraphPaginationDocument, list.validateListGeometryDocument, empty.validateEmptyParagraphFontDocument, page.validatePageGeometryDocument, indent.validateParagraphIndentDocument]) validate(raw);
const document = schema.nodeFromJSON(raw); document.check();
function retained(source, restored) {
  assert.equal(restored.type, source.type); if (source.text !== undefined) assert.equal(restored.text, source.text);
  for (const [key, value] of Object.entries(source.attrs ?? {})) assert.deepEqual(restored.attrs?.[key], value, `Lost ${source.type}.${key}`);
  for (const mark of source.marks ?? []) {const found = restored.marks?.find(item => item.type === mark.type); assert(found); for (const [key, value] of Object.entries(mark.attrs ?? {})) assert.deepEqual(found.attrs?.[key], value);}
  assert.equal(restored.content?.length ?? 0, source.content?.length ?? 0); source.content?.forEach((child, index) => retained(child, restored.content[index]));
}
retained(raw, document.toJSON());
let target; document.forEach((node, pos, index) => {if (!target && ['paragraph', 'heading'].includes(node.type.name) && node.textContent.length > 1) target = {node, pos, index};});
assert(target, 'Need one direct body paragraph or heading with at least two text characters');
const position = target.pos + 1, errors = [], initial = stateFor(document, TextSelection.create(document, position), errors);
const outcomes = {initial: snapshot(document)}, equalities = [], operations = [];
const recipes = {'initial.json': {action: 'schema-roundtrip', input: resolve(inputFile), preserveEverySourceField: true}};
function exercise(name, state, transaction, description, expectedBase = 'initial') {
  const before = state.doc;
  if (transaction) {const result = state.applyTransaction(closeHistory(transaction)); assert.equal(result.transactions.length, 1, `Rejected ${name}`); state = result.state;}
  const edited = state.doc; outcomes[name] = snapshot(edited);
  if (transaction) {
    assert(undo(state, tr => {state = state.apply(tr);})); assert(state.doc.eq(before)); outcomes[`${name}-undo`] = snapshot(state.doc);
    equalities.push([`${expectedBase}.json`, `${name}-undo.json`]);
    assert(redo(state, tr => {state = state.apply(tr);})); assert(state.doc.eq(edited)); outcomes[`${name}-redo`] = snapshot(state.doc);
    equalities.push([`${name}.json`, `${name}-redo.json`]);
  }
  const draft = JSON.parse(JSON.stringify({version: 2, document: snapshot(edited)})); pagination.validateParagraphPaginationDocument(draft.document);
  const restored = schema.nodeFromJSON(draft.document); assert(restored.eq(edited)); outcomes[`${name}-draft`] = snapshot(restored); equalities.push([`${name}.json`, `${name}-draft.json`]);
  operations.push({name, description, changed: !!transaction});
  if (transaction) {recipes[`${name}-undo.json`] = {action: 'equal', base: `${expectedBase}.json`}; recipes[`${name}-redo.json`] = {action: 'equal', base: `${name}.json`};}
  recipes[`${name}-draft.json`] = {action: 'equal', base: `${name}.json`}; return state;
}
const edited = exercise('edited', initial, pagination.createParagraphPaginationTransaction(initial, {keepNext: false, keepLines: true, pageBreakBefore: false}),
  'Set only target paragraph keepNext=false, keepLines=true, pageBreakBefore=false through actual application candidate');
exercise('reset', edited, pagination.createParagraphPaginationTransaction(edited, {keepNext: null, keepLines: null, pageBreakBefore: null}),
  'Remove all three explicit pagination attributes from target only; style/profile defaults resume', 'edited');
exercise('text-edited', initial, initial.tr.insertText('Edited ', position), 'Insert literal Edited followed by one space at target text start; retain pagination and all original marks');
exercise('heading-converted', initial, createWordBlockTypeTransaction(initial, 2), 'Convert target to Heading2 through actual application block-type candidate; retain pagination');
// Set explicit true before a split even if the imported target is unflagged.
const splitReady = exercise('split-ready', initial, pagination.createParagraphPaginationTransaction(initial, {keepNext: true, keepLines: false, pageBreakBefore: true}),
  'Set target keepNext=true, keepLines=false, pageBreakBefore=true for split ownership proof');
const offset = Math.floor(target.node.content.size / 2), selected = splitReady.apply(splitReady.tr.setSelection(TextSelection.create(splitReady.doc, position + offset)));
const splitState = exercise('enter-split', selected, pagination.createParagraphPaginationSplitTransaction(selected),
  `Split target at UTF-16 ProseMirror content offset ${offset}; trailing keeps keepNext=true and keepLines=false, pageBreakBefore absent`, 'split-ready');
const leading = splitState.doc.child(target.index), trailing = splitState.doc.child(target.index + 1);
assert.equal(leading.attrs.pageBreakBefore, true); assert.equal(trailing.attrs.pageBreakBefore, null); assert.equal(trailing.attrs.keepNext, true); assert.equal(trailing.attrs.keepLines, false);
const mergeState = splitState.apply(splitState.tr.setSelection(TextSelection.create(splitState.doc, target.pos + leading.nodeSize + 1)));
let merge; assert(joinBackward(mergeState, tr => {merge = tr;})); assert.equal(mergeState.applyTransaction(merge).transactions.length, 0);
recipes['edited.json'] = {action: 'set-pagination', base: 'initial.json', blockIndexes: [target.index], patch: {keepNext: false, keepLines: true, pageBreakBefore: false}};
recipes['reset.json'] = {action: 'set-pagination', base: 'edited.json', blockIndexes: [target.index], patch: {keepNext: null, keepLines: null, pageBreakBefore: null}};
recipes['text-edited.json'] = {action: 'insert-text', base: 'initial.json', blockIndex: target.index, contentOffset: 0, text: 'Edited ', inheritMarksAtInsertion: true};
recipes['heading-converted.json'] = {action: 'convert-heading', base: 'initial.json', blockIndexes: [target.index], level: 2};
recipes['split-ready.json'] = {action: 'set-pagination', base: 'initial.json', blockIndexes: [target.index], patch: {keepNext: true, keepLines: false, pageBreakBefore: true}};
recipes['enter-split.json'] = {action: 'split-paragraph', base: 'split-ready.json', blockIndex: target.index, contentOffset: offset, trailingDropAttrs: ['pageBreakBefore', 'emptyParagraphFont'], keepOtherAttrs: true};
recipes['merge-refused.json'] = {action: 'equal', base: 'enter-split.json'};
outcomes['merge-refused'] = snapshot(mergeState.doc); equalities.push(['enter-split.json', 'merge-refused.json']);
assert.equal(errors.length, 1); assert(errors[0].includes('段落分页'));
// Safe ordinary all-false merge, including trailing explicit/default parity.
const compatible = exercise('merge-ready', initial, pagination.createParagraphPaginationTransaction(initial, {keepNext: false, keepLines: false, pageBreakBefore: false}),
  'Set target to explicit all-false pagination before a compatible split/merge');
recipes['merge-ready.json'] = {action: 'set-pagination', base: 'initial.json', blockIndexes: [target.index], patch: {keepNext: false, keepLines: false, pageBreakBefore: false}};
const compatibleSelection = compatible.apply(compatible.tr.setSelection(TextSelection.create(compatible.doc, position + offset)));
const compatibleSplit = exercise('merge-split', compatibleSelection, pagination.createParagraphPaginationSplitTransaction(compatibleSelection),
  'Split the all-false target at the same midpoint; only trailing pageBreakBefore becomes absent', 'merge-ready');
recipes['merge-split.json'] = {...recipes['enter-split.json'], base: 'merge-ready.json'};
const compatibleBoundary = target.pos + compatibleSplit.doc.child(target.index).nodeSize + 1;
const mergeSelected = compatibleSplit.apply(compatibleSplit.tr.setSelection(TextSelection.create(compatibleSplit.doc, compatibleBoundary)));
const merged = exercise('merge-compatible', mergeSelected, pagination.createParagraphPaginationMergeTransaction(mergeSelected, -1),
  'Merge equal-effective all-false same-type adjacent paragraphs; preserve the explicit flags and every inline mark', 'merge-split');
recipes['merge-compatible.json'] = {action: 'merge-paragraphs', base: 'merge-split.json', leadingBlockIndex: target.index, trailingBlockIndex: target.index + 1,
  preserveOtherAttrs: true, paginationValue: 'leading explicit else trailing explicit else absent'};
assert.deepEqual(snapshot(merged.doc), snapshot(compatible.doc)); equalities.push(['merge-ready.json', 'merge-compatible.json']);
// An open single-paragraph slice is selected inline content, not ownership of
// a paragraph. Source shell flags intentionally differ from destination flags.
let donorIndex = target.index;
for (let index = target.index + 1; index < document.childCount; index++) if (['paragraph', 'heading'].includes(document.child(index).type.name)) {donorIndex = index; break;}
const donor = document.child(donorIndex), sourceShellFlags = {keepNext: true, keepLines: true, pageBreakBefore: true};
const donorNode = donor.type.create({...donor.attrs, ...sourceShellFlags}, donor.content, donor.marks), pasteOffset = 1;
const pasteState = initial.apply(initial.tr.setSelection(TextSelection.create(initial.doc, position + pasteOffset)));
const pasted = exercise('clipboard-compatible', pasteState, pagination.createParagraphPaginationInlinePasteTransaction(pasteState, new Slice(Fragment.from(donorNode), 1, 1)),
  'Paste complete inline content of the donor paragraph at target content offset 1; preserve every destination shell attr and donor inline mark', 'initial');
recipes['clipboard-compatible.json'] = {action: 'paste-inline', base: 'initial.json', sourceBlockIndex: donorIndex, targetBlockIndex: target.index,
  contentOffset: pasteOffset, sourceShellFlags, preserveDestinationAttrs: true, preserveSourceInlineMarks: true};
assert.deepEqual(pasted.doc.child(target.index).attrs, document.child(target.index).attrs);
// Also exercise the full registered Enter chain on the same prepared snapshot.
const {editor, errors: fullErrors} = fullEditor(splitReady.doc);
try {
  editor.commands.setTextSelection(position + offset); const before = editor.state.doc;
  outcomes['registered-enter-before'] = snapshot(before);
  recipes['registered-enter-before.json'] = {action: 'ensure-trailing-paragraph', base: 'split-ready.json', appendedCount: before.childCount - splitReady.doc.childCount};
  assert(key(editor, 'Enter')); outcomes['registered-enter'] = snapshot(editor.state.doc);
  assert.equal(editor.state.doc.child(target.index).attrs.pageBreakBefore, true); assert.equal(editor.state.doc.child(target.index + 1).attrs.pageBreakBefore, null);
  const after = editor.state.doc; assert(editor.commands.undo()); assert(editor.state.doc.eq(before)); outcomes['registered-enter-undo'] = snapshot(editor.state.doc);
  assert(editor.commands.redo()); assert(editor.state.doc.eq(after)); outcomes['registered-enter-redo'] = snapshot(editor.state.doc);
  recipes['registered-enter.json'] = {...recipes['enter-split.json'], base: 'registered-enter-before.json'};
  recipes['registered-enter-undo.json'] = {action: 'equal', base: 'registered-enter-before.json'};
  recipes['registered-enter-redo.json'] = {action: 'equal', base: 'registered-enter.json'};
  equalities.push(['registered-enter-before.json', 'registered-enter-undo.json'], ['registered-enter.json', 'registered-enter-redo.json']); assert.deepEqual(fullErrors, []);
} finally {editor.destroy();}
const directory = resolve(outputDirectory); mkdirSync(directory, {recursive: true});
for (const [name, model] of Object.entries(outcomes)) {
  pagination.validateParagraphPaginationDocument(model); list.validateListGeometryDocument(model); page.validatePageGeometryDocument(model); indent.validateParagraphIndentDocument(model);
  writeFileSync(join(directory, `${name}.json`), JSON.stringify({version: 2, document: model}, null, 2) + '\n', {flag: 'wx'});
}
const manifest = {input: resolve(inputFile), inputSha256: createHash('sha256').update(bytes).digest('hex'),
  evidence: 'Actual frontend candidates, guards, full registered Enter and undo/redo; browser/native rendering not claimed',
  target: {blockIndex: target.index, blockPosition: target.pos, textPosition: position, splitContentOffset: offset}, operations,
  files: Object.keys(outcomes).map(name => `${name}.json`), equalities, recipes,
  assertions: {sourceFieldsRetained: true, allUndoExact: true, allRedoExact: true, draftsExact: true, mergeRejectedAtomically: true, registeredEnterVerified: true}};
writeFileSync(join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', {flag: 'wx'}); console.log(JSON.stringify(manifest, null, 2));
