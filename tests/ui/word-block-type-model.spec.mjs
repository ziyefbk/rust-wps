import {test, expect} from '@playwright/test';
import {AllSelection, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {Schema} from '@tiptap/pm/model';
import {CellSelection} from '@tiptap/pm/tables';
import {closeHistory, redo, undo, undoDepth} from '@tiptap/pm/history';
import {schema, snapshot, stateFor, paragraph, heading, doc, blocks, createWordBlockTypeTransaction as convert} from './word-block-type-harness.mjs';
const firstAttrs = {textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180};
const secondAttrs = {textAlign: 'right', spaceBefore: 240, spaceAfter: 0, lineSpacing: 360, indentLeftTwips: 0, indentRightTwips: 100, firstLineIndentTwips: 120};
const table = cells => schema.node('table', null, [schema.node('tableRow', null, cells.map(content => schema.node('tableCell', null, content)))]);
const list = children => schema.node('bulletList', null, [schema.node('listItem', null, children)]);
const positions = (document, name) => {const result = []; document.descendants((node, pos) => {if (node.type.name === name) result.push(pos);}); return result;};
const all = document => stateFor(document, new AllSelection(document));
const apply = (state, level) => {const transaction = convert(state, level); expect(transaction).not.toBeNull(); const changed = state.apply(closeHistory(transaction)); return changed.apply(closeHistory(changed.tr));};
function checkPreserved(before, after, level) {
  expect(after.childCount).toBe(before.childCount);
  const oldBlocks = blocks(before), newBlocks = blocks(after);
  expect(newBlocks.length).toBe(oldBlocks.length);
  for (let i = 0; i < oldBlocks.length; i++) {
    const old = oldBlocks[i].node, node = newBlocks[i].node, {level: _, ...attrs} = old.attrs;
    expect(node.type.name).toBe(level ? 'heading' : 'paragraph');
    expect(node.attrs).toEqual(level ? {...attrs, level} : attrs);
    expect(node.content.eq(old.content)).toBe(true);
    expect(node.marks).toEqual(old.marks);
    expect(newBlocks[i].pos).toBe(oldBlocks[i].pos);
  }
}
for (const level of [0, 1, 2, 3]) test(`mixed paragraph/heading conversion to ${level} preserves each block format and explicit zeros`, () => {
  const document = doc([paragraph('A', firstAttrs), heading('B', {...secondAttrs, level: level === 2 ? 1 : 2}), paragraph('C')], {styleProfile: 'standard-v2'});
  const state = all(document), original = snapshot(document), result = apply(state, level);
  checkPreserved(document, result.doc, level);
  expect(snapshot(state.doc)).toEqual(original);
  expect(result.selection.toJSON()).toEqual(state.selection.toJSON());
  expect(result.doc.attrs).toEqual(document.attrs);
  expect(undoDepth(result)).toBe(1);
});

test('original positions 1–5 regression retains distinct fixed spacing through heading and body', () => {
  const document = doc([paragraph('A', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 241}), paragraph('B', {spaceBefore: 240, spaceAfter: 0, lineSpacing: 360})]);
  const state = stateFor(document, TextSelection.create(document, 1, 5)), changed = apply(state, 2), body = apply(changed, 0);
  checkPreserved(document, changed.doc, 2); checkPreserved(document, body.doc, 0);
  expect(body.doc.eq(document)).toBe(true);
});

for (const direction of ['forward', 'backward']) test(`${direction} partial multi-block range preserves exact anchor head and outside nodes`, () => {
  const document = doc([paragraph('outside'), paragraph('abc', firstAttrs), heading('def', secondAttrs), paragraph('after')]);
  const from = blocks(document)[1].pos + 2, to = blocks(document)[2].pos + 2;
  const state = stateFor(document, TextSelection.create(document, direction === 'forward' ? from : to, direction === 'forward' ? to : from));
  const result = apply(state, 3);
  expect(result.selection.toJSON()).toEqual(state.selection.toJSON());
  expect(result.doc.firstChild.eq(document.firstChild)).toBe(true); expect(result.doc.lastChild.eq(document.lastChild)).toBe(true);
  expect(blocks(result.doc).map(({node}) => node.type.name)).toEqual(['paragraph', 'heading', 'heading', 'paragraph']);
  expect(result.doc.child(1).attrs).toEqual({...document.child(1).attrs, level: 3});
  expect(result.doc.child(2).attrs).toEqual({...document.child(2).attrs, level: 3});
});

for (const backwards of [false, true]) test(`${backwards ? 'backward' : 'forward'} range ending at next text start excludes next paragraph`, () => {
  const document = doc([paragraph('abc', firstAttrs), paragraph('def', secondAttrs)]);
  const end = document.firstChild.nodeSize + 1;
  const state = stateFor(document, TextSelection.create(document, backwards ? end : 1, backwards ? 1 : end));
  const result = apply(state, 2);
  expect(result.doc.firstChild.type.name).toBe('heading'); expect(result.doc.lastChild.eq(document.lastChild)).toBe(true);
});

test('empty paragraphs and caret conversion preserve defaults and explicit zeros', () => {
  const document = doc([paragraph('', firstAttrs), heading('', {spaceAfter: 0}), paragraph('')]);
  let state = stateFor(document, TextSelection.create(document, 1));
  state = apply(state, 1); expect(state.selection.anchor).toBe(1); expect(state.doc.lastChild.eq(document.lastChild)).toBe(true);
  const result = apply(all(document), 3); checkPreserved(document, result.doc, 3);
  expect(snapshot(result.doc).content[2].attrs).toEqual({level: 3, textAlign: null});
});

test('marks hard breaks Unicode and links remain exact with cursor stored marks', () => {
  const marks = [schema.marks.bold.create(), schema.marks.fontSize.create({halfPoints: 27}), schema.marks.fontFamily.create({name: 'DejaVu Sans'}), schema.marks.textColor.create({rgb: 'C03030'}), schema.marks.link.create({href: 'https://example.com'})];
  const content = [schema.text('中文😀', marks), schema.node('hardBreak'), schema.text('tail', [schema.marks.subscript.create()])];
  const document = doc([schema.node('paragraph', firstAttrs, content), paragraph('neighbor')]);
  const stored = [schema.marks.italic.create()], state = stateFor(document, TextSelection.create(document, 3), stored);
  const result = apply(state, 2);
  expect(result.doc.firstChild.content.eq(document.firstChild.content)).toBe(true);
  expect(result.storedMarks).toEqual(stored); expect(result.selection.toJSON()).toEqual(state.selection.toJSON());
  const typed = result.apply(result.tr.insertText('x')); expect(typed.doc.firstChild.child(1).marks).toEqual(stored);
});

test('explicit empty stored marks survive without resurrecting surrounding bold', () => {
  const document = doc([schema.node('paragraph', firstAttrs, schema.text('bold', [schema.marks.bold.create()]))]);
  const result = apply(stateFor(document, TextSelection.create(document, 2), []), 2);
  expect(result.storedMarks).toEqual([]);
});

test('NodeSelection remains a NodeSelection on the same converted block', () => {
  const document = doc([paragraph('first'), paragraph('selected', firstAttrs), paragraph('last')]);
  const state = stateFor(document, NodeSelection.create(document, document.firstChild.nodeSize)), result = apply(state, 2);
  expect(result.selection).toBeInstanceOf(NodeSelection); expect(result.selection.toJSON()).toEqual(state.selection.toJSON());
  expect(result.doc.firstChild.eq(document.firstChild)).toBe(true); expect(result.doc.lastChild.eq(document.lastChild)).toBe(true);
});

for (const backwards of [false, true]) test(`table CellSelection ${backwards ? 'backward' : 'forward'} converts selected cells only`, () => {
  const document = doc([paragraph('outside'), table([[paragraph('a', firstAttrs), heading('b', secondAttrs)], [paragraph('c', {spaceBefore: 0})], [paragraph('untouched')]]), paragraph('after')]);
  const cells = positions(document, 'tableCell'), selection = CellSelection.create(document, cells[backwards ? 1 : 0], cells[backwards ? 0 : 1]);
  const state = stateFor(document, selection), result = apply(state, 3);
  expect(result.selection).toBeInstanceOf(CellSelection); expect(result.selection.toJSON()).toEqual(selection.toJSON());
  expect(blocks(result.doc).map(({node}) => node.type.name)).toEqual(['paragraph', 'heading', 'heading', 'heading', 'paragraph', 'paragraph']);
  expect(result.doc.child(1).firstChild.child(2).eq(document.child(1).firstChild.child(2))).toBe(true);
  expect(result.doc.firstChild.eq(document.firstChild)).toBe(true); expect(result.doc.lastChild.eq(document.lastChild)).toBe(true);
});

test('single-cell caret conversion preserves table geometry and all neighboring cells', () => {
  const baseTable = table([[paragraph('a')], [paragraph('b', secondAttrs)]]);
  const document = doc([schema.node('table', {columnWidthsTwips: [2400, 3600], tableCellMarginsTwips: {left: 80, right: 120}}, baseTable.content)]);
  const state = stateFor(document, TextSelection.create(document, blocks(document)[1].pos + 2)), result = apply(state, 2);
  expect(result.doc.firstChild.attrs).toEqual(document.firstChild.attrs);
  expect(result.doc.firstChild.firstChild.firstChild.eq(document.firstChild.firstChild.firstChild)).toBe(true);
});

for (const level of [1, 2, 3]) test(`list first-paragraph conversion to heading ${level} rejects complete mixed operation`, () => {
  const document = doc([paragraph('would change', firstAttrs), list([paragraph('list first', {spaceBefore: 0}), paragraph('tail', {spaceAfter: 120})])]);
  const state = all(document), original = snapshot(document), shared = state.tr, chainable = Object.create(state);
  Object.defineProperty(chainable, 'tr', {get: () => shared});
  expect(() => convert(chainable, level)).toThrow('列表首段');
  expect(shared.steps).toEqual([]); expect(state.doc).toBe(document); expect(snapshot(state.doc)).toEqual(original); expect(undoDepth(state)).toBe(0);
});

test('list trailing paragraph can become heading and body without lifting or touching first item', () => {
  const document = doc([list([paragraph('first', {spaceBefore: 0}), paragraph('tail', {spaceAfter: 120, lineSpacing: 241})])]);
  const pos = blocks(document)[1].pos, state = stateFor(document, TextSelection.create(document, pos + 2));
  const result = apply(state, 2), restored = apply(result, 0);
  expect(result.doc.firstChild.type.name).toBe('bulletList');
  expect(result.doc.firstChild.firstChild.firstChild.eq(document.firstChild.firstChild.firstChild)).toBe(true);
  expect(restored.doc.eq(document)).toBe(true);
});

test('body conversion of mixed list and headings retains list structure and is atomic', () => {
  const document = doc([heading('body', firstAttrs), list([paragraph('first', {spaceBefore: 0}), heading('tail', {spaceAfter: 120})])]);
  const result = apply(all(document), 0); checkPreserved(document, result.doc, 0); expect(result.doc.child(1).type.name).toBe('bulletList');
});

test('unsupported selected code block is not flattened and earlier prospective changes are discarded', () => {
  const document = doc([paragraph('first', firstAttrs), schema.node('codeBlock', null, schema.text('code'))]);
  const state = all(document); expect(() => convert(state, 2)).toThrow('当前选区'); expect(state.doc).toBe(document); expect(undoDepth(state)).toBe(0);
});

test('image and page-break atoms are untouched; atom-only selection is a no-op', () => {
  const image = schema.node('image', {src: 'data:image/png;base64,eA==', alt: 'unchanged'}), pageBreak = schema.node('pageBreak');
  const document = doc([paragraph('before', firstAttrs), image, pageBreak, heading('after')]);
  const result = apply(all(document), 3);
  expect(result.doc.child(1).eq(image)).toBe(true); expect(result.doc.child(2).eq(pageBreak)).toBe(true);
  const atomState = stateFor(document, NodeSelection.create(document, document.firstChild.nodeSize));
  expect(convert(atomState, 2)).toBeNull(); expect(undoDepth(atomState)).toBe(0);
});

test('unchanged conversions do not create transactions or history entries', () => {
  for (const [document, level] of [[doc([paragraph('a', firstAttrs), paragraph('b', secondAttrs)]), 0], [doc([heading('a', firstAttrs), heading('b', secondAttrs)]), 2]]) {
    const state = all(document); expect(convert(state, level)).toBeNull(); expect(undoDepth(state)).toBe(0); expect(state.doc).toBe(document);
  }
});

test('heading/body changes each undo once independently of prior and later typing', () => {
  let state = stateFor(doc([paragraph('first', firstAttrs), paragraph('second', secondAttrs)]));
  state = state.apply(state.tr.insertText('before')); const typedBefore = state.doc;
  state = state.apply(state.tr.setSelection(new AllSelection(state.doc))); state = apply(state, 2); const headings = state.doc;
  state = apply(state, 0); const body = state.doc;
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 2))); state = state.apply(state.tr.insertText('after')); const typedAfter = state.doc;
  for (const expected of [body, headings, typedBefore]) {expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(expected)).toBe(true);}
  for (const expected of [headings, body, typedAfter]) {expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(expected)).toBe(true);}
});

test('invalid levels and invalid prospective spacing or indentation never mutate caller shared transaction', () => {
  for (const [document, level, message] of [
    ...[-1, 4, 1.5, NaN, Infinity, '2', null].map(level => [doc([paragraph('x')]), level, '级别']),
    [doc([paragraph('good'), paragraph('bad', {spaceAfter: -1})]), 2, '间距'],
    [doc([paragraph('good'), paragraph('bad', {textAlign: 'invalid'})]), 2, '段落对齐'],
    [doc([paragraph('good'), paragraph('bad', {indentLeftTwips: 9026})]), 2, '正文宽度'],
  ]) {
    const state = all(document), shared = state.tr, chainable = Object.create(state);
    Object.defineProperty(chainable, 'tr', {get: () => shared});
    const selection = state.selection, marks = state.storedMarks;
    expect(() => convert(chainable, level)).toThrow(message); expect(shared.steps).toEqual([]); expect(state.doc).toBe(document); expect(undoDepth(state)).toBe(0);
    expect(state.selection).toBe(selection); expect(state.storedMarks).toBe(marks); expect(shared.selection.eq(selection)).toBe(true);
  }
});

test('node count text and draft budgets reject before mutation', () => {
  const cases = [
    [doc(Array.from({length: 5000}, () => paragraph('x'))), '结构'],
    [doc([paragraph('x'.repeat(2 * 1024 * 1024 + 1))]), '2 MiB'],
    [doc([paragraph('x'), schema.node('image', {src: 'x'.repeat(32 * 1024 * 1024)})]), '32 MiB'],
  ];
  for (const [document, message] of cases) {
    const state = all(document); expect(() => convert(state, 2)).toThrow(message); expect(state.doc).toBe(document); expect(undoDepth(state)).toBe(0);
  }
});

test('draft JSON reconstruction retains all converted fields and legacy profile omission', () => {
  for (const profile of [null, 'standard-v2']) {
    const document = doc([paragraph('x', firstAttrs), heading('y', secondAttrs)], {styleProfile: profile});
    const result = apply(all(document), 3), raw = snapshot(result.doc), restored = schema.nodeFromJSON(JSON.parse(JSON.stringify(raw)));
    expect(restored.eq(result.doc)).toBe(true); expect(raw.attrs?.styleProfile ?? null).toBe(profile);
    expect(JSON.stringify(raw)).not.toMatch(/"(?:spaceBefore|spaceAfter|lineSpacing|indentLeftTwips|indentRightTwips|firstLineIndentTwips)":null/);
  }
});

test('backward multi-cell selection includes empty paragraphs without selecting outside cells', () => {
  const document = doc([table([[paragraph('outside')], [paragraph('', firstAttrs), paragraph('', secondAttrs)], [paragraph('last')]])]);
  const cells = positions(document, 'tableCell'), selection = CellSelection.create(document, cells[2], cells[1]);
  const result = apply(stateFor(document, selection), 1);
  expect(result.selection.toJSON()).toEqual(selection.toJSON());
  expect(blocks(result.doc).map(({node}) => node.type.name)).toEqual(['paragraph', 'heading', 'heading', 'heading']);
  expect(blocks(result.doc)[1].node.attrs).toEqual({...blocks(document)[1].node.attrs, level: 1});
  expect(blocks(result.doc)[2].node.attrs).toEqual({...blocks(document)[2].node.attrs, level: 1});
});

test('compatible schema attributes are copied per block and unrepresentable active attributes reject', () => {
  const original = schema.spec.nodes.get('paragraph'), target = schema.spec.nodes.get('heading');
  const pSpec = {...original, attrs: {...original.attrs, futureFormat: {default: null}}};
  const withMissingTarget = new Schema({nodes: schema.spec.nodes.update('paragraph', pSpec), marks: schema.spec.marks});
  const missing = withMissingTarget.node('doc', null, [withMissingTarget.node('paragraph', {futureFormat: 'important'}, withMissingTarget.text('x'))]);
  const state = all(missing); expect(() => convert(state, 2)).toThrow('目标段落不支持已有属性'); expect(undoDepth(state)).toBe(0);
  const withCompatibleTarget = new Schema({nodes: schema.spec.nodes.update('paragraph', pSpec).update('heading', {...target, attrs: {...target.attrs, futureFormat: {default: null}}}), marks: schema.spec.marks});
  const compatible = withCompatibleTarget.node('doc', null, ['first', 'second'].map(value => withCompatibleTarget.node('paragraph', {futureFormat: value}, withCompatibleTarget.text(value))));
  const changed = apply(all(compatible), 2); expect(blocks(changed.doc).map(({node}) => node.attrs.futureFormat)).toEqual(['first', 'second']);
});

test('deep prospective structure rejects without leaking changes or stored marks', () => {
  let nested = paragraph('deep'); for (let i = 0; i < 33; i++) nested = schema.node('blockquote', null, nested);
  const document = doc([paragraph('first'), nested]), state = all(document), shared = state.tr, chainable = Object.create(state);
  Object.defineProperty(chainable, 'tr', {get: () => shared});
  expect(() => convert(chainable, 2)).toThrow('结构'); expect(shared.steps).toEqual([]); expect(state.doc).toBe(document); expect(undoDepth(state)).toBe(0);
});

test('app-supported inline code keeps explicit font size color and family during type conversion', () => {
  const marks = [schema.marks.code.create(), schema.marks.fontSize.create({halfPoints: 27}), schema.marks.fontFamily.create({name: 'DejaVu Sans'}), schema.marks.textColor.create({rgb: 'C03030'})];
  const document = doc([schema.node('paragraph', firstAttrs, schema.text('inline code', marks)), heading('other', secondAttrs)]);
  const result = apply(all(document), 3); expect(result.doc.firstChild.content.eq(document.firstChild.content)).toBe(true);
  const restored = apply(result, 0); expect(restored.doc.firstChild.content.eq(document.firstChild.content)).toBe(true);
});
