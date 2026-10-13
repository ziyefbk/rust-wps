import {test, expect} from '@playwright/test';
import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo, undoDepth} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';

const {ParagraphSpacing, createParagraphSpacingTransaction, selectedParagraphSpacing,
  omitDefaultParagraphSpacing, validateParagraphSpacingDocument, parseParagraphSpacePoints,
  parseLineSpacingLines} = await loadTsModule('./ui/word-paragraph-spacing.ts');
const {snapshotDocument, documentStyleProfile} = await loadTsModule('./ui/word-style-profile.ts');
const ProfileDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+',
  addAttributes: () => ({styleProfile: {default: null}})});
const extensions = [StarterKit.configure({document: false}), ProfileDoc, TableKit, ParagraphSpacing];
const schema = getSchema(extensions);
const paragraph = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const heading = (text, attrs = {}) => schema.node('heading', {level: 2, ...attrs}, schema.text(text));
const doc = (children, profile = 'standard-v2') => schema.node('doc', {styleProfile: profile}, children);
const list = items => schema.node('bulletList', null, items.map(children => schema.node('listItem', null, children)));
const table = cells => schema.node('table', null, [schema.node('tableRow', null,
  cells.map(children => schema.node('tableCell', null, children)))]);
const blocks = document => {
  const result = [];
  document.descendants((node, pos) => {
    if (['paragraph', 'heading'].includes(node.type.name)) result.push({node, pos});
  });
  return result;
};
const spacingOf = node => [node.attrs.spaceBefore, node.attrs.spaceAfter, node.attrs.lineSpacing];
const spacings = document => blocks(document).map(({node}) => spacingOf(node));
const snapshot = document => omitDefaultParagraphSpacing(snapshotDocument({getJSON: () => document.toJSON()}));
const stateFor = (document, selection = TextSelection.atStart(document)) =>
  EditorState.create({doc: document, selection, plugins: [history()]});
const selectAll = document => stateFor(document, new AllSelection(document));
const applySpacing = (state, patch) => {
  const transaction = createParagraphSpacingTransaction(state, patch);
  expect(transaction).not.toBeNull();
  const changed = state.apply(closeHistory(transaction));
  return changed.apply(closeHistory(changed.tr));
};
const editors = [];
const headless = document => {
  const editor = new Editor({element: null, extensions, content: snapshot(document)});
  // No DOM is mounted. Register real PM history explicitly because Tiptap
  // normally installs its plugins when mounting the browser view.
  editor.registerPlugin(history());
  editors.push(editor);
  return editor;
};
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

// This suite starts AFTER audited source resolution. These are fixed-format
// import results, not source styles or a simulation of Word's contextual rule.
// Editing must not infer style identity or recompute an adjacent paragraph gap.
test('normalized zero and nonzero import results roundtrip as ordinary V2 spacing snapshots', () => {
  const normalized = doc([
    paragraph('First', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 241}),
    paragraph('Second', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 360}),
    heading('Third', {spaceBefore: 61, spaceAfter: 0, lineSpacing: 480}),
  ]);
  const captured = snapshot(normalized);
  const draft = JSON.parse(JSON.stringify({version: 2, document: captured}));
  expect(draft.version).toBe(2);
  expect(documentStyleProfile(draft.document)).toBe('standard-v2');
  expect(() => validateParagraphSpacingDocument(draft.document)).not.toThrow();
  expect(schema.nodeFromJSON(draft.document).eq(normalized)).toBe(true);
  expect(spacings(schema.nodeFromJSON(draft.document))).toEqual([[240, 0, 241], [0, 120, 360], [61, 0, 480]]);
  expect(JSON.stringify(draft)).not.toMatch(/contextualSpacing|paragraphStyle|styleId|sourceStyle/);
  expect(captured).toEqual(snapshot(normalized));
});

test('moving a normalized paragraph changes its neighbors without recalculating either fixed gap', () => {
  const first = paragraph('A', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 241});
  const middle = paragraph('B', {spaceBefore: 0, spaceAfter: 0, lineSpacing: 241});
  const last = paragraph('C', {spaceBefore: 0, spaceAfter: 180, lineSpacing: 360});
  let state = stateFor(doc([first, middle, last]));
  const original = state.doc, captured = snapshot(original);
  const transaction = state.tr.delete(first.nodeSize, first.nodeSize + middle.nodeSize);
  transaction.insert(transaction.doc.content.size, middle);
  state = state.apply(closeHistory(transaction));
  expect(blocks(state.doc).map(({node}) => node.textContent)).toEqual(['A', 'C', 'B']);
  expect(spacings(state.doc)).toEqual([[240, 0, 241], [0, 180, 360], [0, 0, 241]]);
  expect(state.doc.child(0).eq(first)).toBe(true);
  expect(state.doc.child(1).eq(last)).toBe(true);
  expect(state.doc.child(2).eq(middle)).toBe(true);
  expect(snapshot(original)).toEqual(captured);
  const reordered = state.doc;
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.eq(reordered)).toBe(true);
});

for (const [before, after] of [[0, 0], [0, 120], [240, 0], [31680, 1]]) {
  test(`single-paragraph heading and body commands retain fixed ${before}/${after} spacing`, () => {
    const marked = schema.node('paragraph', {spaceBefore: before, spaceAfter: after, lineSpacing: 241},
      schema.text('Fixed heading', [schema.marks.bold.create()]));
    const editor = headless(doc([marked, paragraph('Neighbor', {spaceBefore: 80, spaceAfter: 0})]));
    const beforeContent = editor.state.doc.firstChild.content, neighbor = editor.state.doc.lastChild;
    expect(editor.commands.setTextSelection(3)).toBe(true);
    expect(editor.commands.setHeading({level: 2})).toBe(true);
    expect(editor.state.doc.firstChild.type.name).toBe('heading');
    expect(editor.state.doc.firstChild.attrs.level).toBe(2);
    expect(spacingOf(editor.state.doc.firstChild)).toEqual([before, after, 241]);
    expect(editor.state.doc.firstChild.content.eq(beforeContent)).toBe(true);
    expect(editor.state.doc.lastChild.eq(neighbor)).toBe(true);
    expect(editor.commands.setParagraph()).toBe(true);
    expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
    expect(spacingOf(editor.state.doc.firstChild)).toEqual([before, after, 241]);
    expect(() => validateParagraphSpacingDocument(snapshot(editor.state.doc))).not.toThrow();
  });
}

test('existing limitation: a multi-paragraph heading command resets fixed spacing instead of retaining each block', () => {
  // This records the existing Tiptap conversion limitation, not desirable
  // contextual-spacing semantics. Remove this expectation when the separate
  // per-block format-preserving heading-conversion increment is implemented.
  const editor = headless(doc([
    paragraph('A', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 241}),
    paragraph('B', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 360}),
  ]));
  const original = editor.state.doc;
  expect(editor.commands.setTextSelection({from: 1, to: 5})).toBe(true);
  expect(editor.commands.setHeading({level: 2})).toBe(true);
  expect(blocks(editor.state.doc).map(({node}) => node.type.name)).toEqual(['heading', 'heading']);
  expect(blocks(editor.state.doc).map(({node}) => node.textContent)).toEqual(['A', 'B']);
  expect(spacings(editor.state.doc)).toEqual([[null, null, null], [null, null, null]]);
  expect(snapshot(editor.state.doc).content.map(node => node.attrs)).toEqual([{level: 2}, {level: 2}]);
  expect(editor.commands.undo()).toBe(true);
  expect(editor.state.doc.eq(original)).toBe(true);
  expect(editor.commands.redo()).toBe(true);
  expect(spacings(editor.state.doc)).toEqual([[null, null, null], [null, null, null]]);
});

for (const offset of [3, 6]) {
  test(`real Tiptap split at offset ${offset} copies fixed values without applying a contextual rule`, () => {
    const editor = headless(doc([paragraph('abcdef', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 241}),
      paragraph('Neighbor', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 360})]));
    const original = editor.state.doc;
    expect(editor.commands.setTextSelection(1 + offset)).toBe(true);
    expect(editor.commands.splitBlock()).toBe(true);
    expect(blocks(editor.state.doc).map(({node}) => node.textContent)).toEqual([
      'abcdef'.slice(0, offset), 'abcdef'.slice(offset), 'Neighbor',
    ]);
    expect(spacings(editor.state.doc)).toEqual([[0, 120, 241], [0, 120, 241], [240, 0, 360]]);
    const split = snapshot(editor.state.doc);
    expect(editor.commands.undo()).toBe(true);
    expect(editor.state.doc.eq(original)).toBe(true);
    expect(editor.commands.redo()).toBe(true);
    expect(snapshot(editor.state.doc)).toEqual(split);
  });
}

test('an independently inserted default paragraph does not inherit a former source-style gap rule', () => {
  const original = doc([paragraph('Before', {spaceBefore: 240, spaceAfter: 0}),
    paragraph('After', {spaceBefore: 0, spaceAfter: 120})]);
  const state = stateFor(original);
  const inserted = state.apply(state.tr.insert(original.firstChild.nodeSize, paragraph('New paragraph')));
  expect(spacings(inserted.doc)).toEqual([[240, 0, null], [null, null, null], [0, 120, null]]);
  expect(snapshot(inserted.doc).content[1]).not.toHaveProperty('attrs');
  expect(inserted.doc.firstChild.eq(original.firstChild)).toBe(true);
  expect(inserted.doc.lastChild.eq(original.lastChild)).toBe(true);
});

test('list-item splitting and item reordering preserve paragraph values across list boundaries', () => {
  const editor = headless(doc([
    paragraph('Outside before', {spaceBefore: 240, spaceAfter: 0}),
    list([[paragraph('abcdef', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 241})],
      [paragraph('Other item', {spaceBefore: 80, spaceAfter: 0, lineSpacing: 360})]]),
    paragraph('Outside after', {spaceBefore: 0, spaceAfter: 180}),
  ]));
  const splitPosition = blocks(editor.state.doc)[1].pos + 4;
  expect(editor.commands.setTextSelection(splitPosition)).toBe(true);
  expect(editor.commands.splitListItem('listItem')).toBe(true);
  expect(spacings(editor.state.doc)).toEqual([[240, 0, null], [0, 120, 241], [0, 120, 241], [80, 0, 360], [0, 180, null]]);
  const currentList = editor.state.doc.child(1), listPosition = editor.state.doc.firstChild.nodeSize;
  const reorderedList = currentList.copy(currentList.content.cut(currentList.child(0).nodeSize)
    .append(currentList.content.cut(0, currentList.child(0).nodeSize)));
  editor.view.dispatch(editor.state.tr.replaceWith(listPosition, listPosition + currentList.nodeSize, reorderedList));
  expect(blocks(editor.state.doc).map(({node}) => node.textContent)).toEqual(['Outside before', 'def', 'Other item', 'abc', 'Outside after']);
  expect(spacings(editor.state.doc)).toEqual([[240, 0, null], [0, 120, 241], [80, 0, 360], [0, 120, 241], [0, 180, null]]);
  expect(() => validateParagraphSpacingDocument(snapshot(editor.state.doc))).not.toThrow();
});

test('cell-selection edits stay within that cell and keep explicit zeroes on both sides of table boundaries', () => {
  const original = doc([
    paragraph('Outside before', {spaceBefore: 240, spaceAfter: 0}),
    table([[paragraph('Cell one A', {spaceBefore: 0, spaceAfter: 0}), paragraph('Cell one B', {spaceBefore: 0, spaceAfter: 120})],
      [paragraph('Cell two', {spaceBefore: 80, spaceAfter: 0})]]),
    paragraph('Outside after', {spaceBefore: 0, spaceAfter: 180}),
  ]);
  const cells = [];
  original.descendants((node, pos) => {if (node.type.name === 'tableCell') cells.push(pos);});
  const state = stateFor(original, CellSelection.create(original, cells[0]));
  const changed = applySpacing(state, {spaceAfter: parseParagraphSpacePoints('6.05')});
  expect(spacings(changed.doc)).toEqual([[240, 0, null], [0, 121, null], [0, 121, null], [80, 0, null], [0, 180, null]]);
  expect(changed.selection).toBeInstanceOf(CellSelection);
  expect(changed.doc.child(1).firstChild.child(1).eq(original.child(1).firstChild.child(1))).toBe(true);
  expect(changed.doc.firstChild.eq(original.firstChild)).toBe(true);
  expect(changed.doc.lastChild.eq(original.lastChild)).toBe(true);
  expect(() => validateParagraphSpacingDocument(snapshot(changed.doc))).not.toThrow();
});

test('ordinary spacing-control parser and transaction path can modify and reset normalized fixed values', () => {
  const original = doc([paragraph('Body', {spaceBefore: 0, spaceAfter: 0, lineSpacing: 241}),
    heading('Heading', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 241})]);
  const state = selectAll(original);
  expect(selectedParagraphSpacing(state)).toEqual({spaceBefore: {value: null, mixed: true},
    spaceAfter: {value: 0, mixed: false}, lineSpacing: {value: 241, mixed: false}});
  const changed = applySpacing(state, {spaceBefore: parseParagraphSpacePoints('3.05'),
    spaceAfter: parseParagraphSpacePoints('0'), lineSpacing: parseLineSpacingLines('241/240')});
  expect(spacings(changed.doc)).toEqual([[61, 0, 241], [61, 0, 241]]);
  expect(createParagraphSpacingTransaction(changed, {spaceBefore: 61, spaceAfter: 0, lineSpacing: 241})).toBeNull();
  const captured = snapshot(changed.doc);
  const reset = applySpacing(changed, {spaceBefore: null, spaceAfter: null, lineSpacing: null});
  expect(snapshot(reset.doc).content[0]).not.toHaveProperty('attrs');
  expect(snapshot(reset.doc).content[1].attrs).toEqual({level: 2});
  expect(snapshot(reset.doc).attrs).toEqual({styleProfile: 'standard-v2'});
  expect(captured.content[0].attrs).toEqual({spaceBefore: 61, spaceAfter: 0, lineSpacing: 241});
  expect(selectedParagraphSpacing(reset).spaceBefore).toEqual({value: null, mixed: false});
});

test('normalization-derived values survive grouped spacing/reset undo redo and separate later typing', () => {
  const original = doc([paragraph('One', {spaceBefore: 240, spaceAfter: 0}),
    paragraph('Two', {spaceBefore: 0, spaceAfter: 120})]);
  let state = applySpacing(selectAll(original), {spaceAfter: 61});
  const modified = state.doc;
  state = applySpacing(state, {spaceBefore: null, spaceAfter: null, lineSpacing: null});
  const reset = state.doc;
  for (const expected of [modified, original]) {
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
    expect(state.doc.eq(expected)).toBe(true);
  }
  for (const expected of [modified, reset]) {
    expect(redo(state, tr => {state = state.apply(tr);})).toBe(true);
    expect(state.doc.eq(expected)).toBe(true);
  }
  state = state.apply(closeHistory(state.tr.setSelection(TextSelection.create(state.doc, 2))));
  state = state.apply(state.tr.insertText('x'));
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.eq(reset)).toBe(true);
  expect(() => validateParagraphSpacingDocument(snapshot(state.doc))).not.toThrow();
});

test('invalid mixed patches are atomic and leave the fixed snapshot selection and history unchanged', () => {
  const original = doc([paragraph('One', {spaceBefore: 0, spaceAfter: 120}), heading('Two', {spaceBefore: 240, spaceAfter: 0})]);
  const state = selectAll(original), captured = snapshot(original), selection = state.selection;
  const shared = state.tr, chainable = Object.create(state);
  Object.defineProperty(chainable, 'tr', {get: () => shared});
  for (const patch of [{spaceBefore: 40, spaceAfter: -1}, {spaceBefore: 40, spaceAfter: 31681},
    {spaceBefore: 40, lineSpacing: 119}, {spaceBefore: 40, lineSpacing: 2401}, {spaceBefore: 40, spaceAfter: .5},
    {spaceBefore: 40, spaceAfter: '0'}, {spaceBefore: 40, spaceAfter: undefined}, {spaceBefore: 40, lineSpacing: NaN}]) {
    expect(() => createParagraphSpacingTransaction(chainable, patch)).toThrow('段前');
    expect(state.doc).toBe(original);
    expect(state.selection).toBe(selection);
    expect(shared.steps).toEqual([]);
    expect(snapshot(state.doc)).toEqual(captured);
    expect(undoDepth(state)).toBe(0);
  }
});

test('spacing commands reject invented contextual and named-style patch fields rather than creating metadata', () => {
  const state = selectAll(doc([paragraph('Fixed', {spaceBefore: 0, spaceAfter: 120})]));
  for (const [field, value] of [['contextualSpacing', true], ['paragraphStyle', 'Body'], ['styleId', 'Body'],
    ['sourceStyleId', 'Body'], ['contextualSpacing', false]]) {
    expect(() => createParagraphSpacingTransaction(state, {spaceBefore: 40, [field]: value})).toThrow('段前');
    expect(undoDepth(state)).toBe(0);
    expect(schema.nodes.paragraph.spec.attrs).not.toHaveProperty(field);
    expect(schema.nodes.heading.spec.attrs).not.toHaveProperty(field);
    expect(() => documentStyleProfile({type: 'doc', attrs: {styleProfile: 'standard-v2', [field]: value}})).toThrow('根属性');
  }
  // The frontend's spacing validator validates its own known fields only.
  // Raw paragraph unknown-field rejection is a Rust model-boundary concern,
  // not an additional validation guarantee claimed by this frontend suite.
  expect(snapshot(state.doc).content[0].attrs).toEqual({spaceBefore: 0, spaceAfter: 120});
});

test('incoming fixed snapshots reject explicit null invalid ranges and misplaced known spacing fields', () => {
  for (const type of ['paragraph', 'heading']) for (const field of ['spaceBefore', 'spaceAfter']) {
    for (const value of [null, -1, 31681, .5, '0', true, NaN, Infinity]) {
      const raw = {type: 'doc', content: [{type, attrs: {[field]: value}}]};
      expect(() => validateParagraphSpacingDocument(raw)).toThrow(field);
    }
  }
  for (const type of ['doc', 'text', 'bulletList', 'listItem', 'table', 'tableCell']) {
    expect(() => validateParagraphSpacingDocument({type, attrs: {spaceAfter: 0}})).toThrow('spaceAfter');
  }
  for (const value of [0, 1, 31680]) {
    const valid = snapshot(doc([paragraph('Body', {spaceBefore: value, spaceAfter: value})]));
    expect(() => validateParagraphSpacingDocument(valid)).not.toThrow();
    expect(snapshot(schema.nodeFromJSON(valid))).toEqual(valid);
  }
});

test('snapshot omission distinguishes explicit zero from defaults and preserves legacy document bytes', () => {
  const legacy = {type: 'doc', content: [{type: 'paragraph', content: [{type: 'text', text: 'Legacy'}]},
    {type: 'heading', attrs: {level: 2}, content: [{type: 'text', text: 'Heading'}]}]};
  const internal = schema.nodeFromJSON(legacy);
  const internalJson = internal.toJSON();
  expect(JSON.stringify(snapshot(internal))).toBe(JSON.stringify(legacy));
  expect(internalJson.content[0].attrs).toEqual({spaceBefore: null, spaceAfter: null, lineSpacing: null});
  const zero = snapshot(doc([paragraph('Zero', {spaceBefore: 0, spaceAfter: 0}), paragraph('Default')]));
  expect(zero.content[0].attrs).toEqual({spaceBefore: 0, spaceAfter: 0});
  expect(zero.content[1]).not.toHaveProperty('attrs');
  expect(JSON.stringify(zero)).not.toMatch(/"(?:spaceBefore|spaceAfter|lineSpacing)":null/);
  expect(internal.toJSON()).toEqual(internalJson);
});

test('paragraph and heading DOM specs render fixed zero margins and exact twips without contextual metadata', () => {
  for (const node of [paragraph('Body', {spaceBefore: 0, spaceAfter: 61, lineSpacing: 241}),
    heading('Heading', {spaceBefore: 0, spaceAfter: 61, lineSpacing: 241})]) {
    const [tag, attrs] = node.type.spec.toDOM(node);
    expect(tag).toBe(node.type.name === 'paragraph' ? 'p' : 'h2');
    expect(attrs).toEqual({'data-space-before': '0', 'data-space-after': '61', 'data-line-spacing': '241',
      style: 'margin-top: 0pt; margin-bottom: 3.05pt; line-height: 1.0041666666666667'});
  }
});
